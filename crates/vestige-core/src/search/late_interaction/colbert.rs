//! ColBERTv2 token embedder via ONNX Runtime (`ort`).
//!
//! fastembed cannot expose ColBERT's per-token output (its `TextEmbedding`
//! pools to a single vector), so we drive the ONNX graph directly. The graph
//! (`colbert-ir/colbertv2.0`, `model.onnx`) already contains the linear
//! projection to 128 dims, so its output is `[batch, seq_len, 128]` of raw
//! token embeddings; we L2-normalize each row to make MaxSim a dot product.
//!
//! ## ColBERT tokenization (Khattab & Zaharia 2020, §3.1)
//!
//! Query and document are encoded asymmetrically:
//! - **Query:** `[CLS] [Q] t1 … tn [SEP]` then padded with `[MASK]` up to
//!   `query_maxlen` (32). The attention mask is 1 over the whole padded span —
//!   ColBERT deliberately attends to the `[MASK]` slots (query augmentation).
//! - **Document:** `[CLS] [D] t1 … tm [SEP]`, attention mask over real tokens
//!   only. Punctuation token positions are dropped from the output.
//!
//! `[Q]`/`[D]` are the BERT `[unused0]`/`[unused1]` ids (1 and 2 in the
//! bert-base-uncased vocab). This embedder is feature-gated (`late-interaction`)
//! and constructed once at startup; runtime quality is validated on the LoCoMo
//! harness, not in unit tests (no model in CI).

use std::path::Path;
use std::sync::Mutex;

use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

use super::TokenEmbedder;

/// bert-base-uncased special-token ids (stable across the ColBERT vocab).
const CLS_ID: i64 = 101;
const SEP_ID: i64 = 102;
const MASK_ID: i64 = 103;
/// `[Q]` marker = `[unused0]`, `[D]` marker = `[unused1]`.
const Q_MARKER_ID: i64 = 1;
const D_MARKER_ID: i64 = 2;

/// Tunables for the ColBERT embedder.
#[derive(Debug, Clone)]
pub struct ColbertConfig {
    /// Fixed query length; queries are padded/truncated to this with `[MASK]`.
    pub query_maxlen: usize,
    /// Hard cap on document tokens (before markers) to bound latency.
    pub doc_maxlen: usize,
    /// Projection dimensionality the ONNX graph emits (128 for ColBERTv2).
    pub dim: usize,
}

impl Default for ColbertConfig {
    fn default() -> Self {
        Self {
            query_maxlen: 32,
            doc_maxlen: 220,
            dim: 128,
        }
    }
}

/// Errors from loading or running the ColBERT model.
#[derive(Debug)]
pub enum ColbertError {
    /// Model or tokenizer file missing / unreadable.
    Load(String),
    /// Tokenization failed.
    Tokenize(String),
    /// ONNX inference failed.
    Inference(String),
    /// The model produced an output shape we did not expect.
    Shape(String),
}

impl std::fmt::Display for ColbertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ColbertError::Load(e) => write!(f, "ColBERT load failed: {e}"),
            ColbertError::Tokenize(e) => write!(f, "ColBERT tokenize failed: {e}"),
            ColbertError::Inference(e) => write!(f, "ColBERT inference failed: {e}"),
            ColbertError::Shape(e) => write!(f, "ColBERT output shape error: {e}"),
        }
    }
}

impl std::error::Error for ColbertError {}

/// A loaded ColBERTv2 model + tokenizer. `Session` is wrapped in a `Mutex`
/// because inference is serialized through the blocking pool (same rationale as
/// the embedding model) and to keep the embedder `Sync`.
pub struct ColbertEmbedder {
    session: Mutex<Session>,
    tokenizer: Tokenizer,
    config: ColbertConfig,
}

impl ColbertEmbedder {
    /// Load `model.onnx` + `tokenizer.json` from `model_dir`.
    pub fn from_dir(model_dir: &Path, config: ColbertConfig) -> Result<Self, ColbertError> {
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        let session = Session::builder()
            .and_then(|mut b| b.commit_from_file(&model_path))
            .map_err(|e| ColbertError::Load(format!("{}: {e}", model_path.display())))?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| ColbertError::Load(format!("{}: {e}", tokenizer_path.display())))?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            config,
        })
    }

    /// Load from the directory in `VESTIGE_COLBERT_MODEL_DIR` (expects
    /// `model.onnx` + `tokenizer.json`). Returns `Ok(None)` when the env var is
    /// unset/empty — the feature is then inert — and `Err` when it points at a
    /// broken path, so the caller can warn and fall back to the cross-encoder.
    pub fn from_env(config: ColbertConfig) -> Result<Option<Self>, ColbertError> {
        match std::env::var("VESTIGE_COLBERT_MODEL_DIR") {
            Ok(dir) if !dir.trim().is_empty() => {
                Self::from_dir(Path::new(dir.trim()), config).map(Some)
            }
            _ => Ok(None),
        }
    }

    /// Tokenize `text` into BERT subword ids (no special tokens — we add the
    /// ColBERT markers ourselves).
    fn subword_ids(&self, text: &str) -> Result<Vec<i64>, ColbertError> {
        let enc = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| ColbertError::Tokenize(e.to_string()))?;
        Ok(enc.get_ids().iter().map(|&id| id as i64).collect())
    }

    /// Build (input_ids, attention_mask) for a query: `[CLS][Q] … [SEP]` then
    /// `[MASK]`-padded to `query_maxlen`, attention 1 over the whole span.
    fn build_query(&self, text: &str) -> Result<(Vec<i64>, Vec<i64>), ColbertError> {
        let body = self.subword_ids(text)?;
        let max_body = self.config.query_maxlen.saturating_sub(3); // CLS, Q, SEP
        let body = &body[..body.len().min(max_body)];

        let mut ids = Vec::with_capacity(self.config.query_maxlen);
        ids.push(CLS_ID);
        ids.push(Q_MARKER_ID);
        ids.extend_from_slice(body);
        ids.push(SEP_ID);
        while ids.len() < self.config.query_maxlen {
            ids.push(MASK_ID);
        }
        // Query augmentation: attend to the [MASK] padding too → all ones.
        let mask = vec![1_i64; ids.len()];
        Ok((ids, mask))
    }

    /// Build (input_ids, attention_mask) for a document: `[CLS][D] … [SEP]`,
    /// attention over real tokens only (no padding for a single sequence).
    fn build_document(&self, text: &str) -> Result<(Vec<i64>, Vec<i64>), ColbertError> {
        let body = self.subword_ids(text)?;
        let body = &body[..body.len().min(self.config.doc_maxlen)];

        let mut ids = Vec::with_capacity(body.len() + 3);
        ids.push(CLS_ID);
        ids.push(D_MARKER_ID);
        ids.extend_from_slice(body);
        ids.push(SEP_ID);
        let mask = vec![1_i64; ids.len()];
        Ok((ids, mask))
    }

    /// Run the ONNX graph and return L2-normalized per-token vectors, dropping
    /// the rows whose attention mask is 0 (padding never reaches here because
    /// query masks are all-ones and docs are unpadded, but we keep the guard).
    fn run(&self, ids: Vec<i64>, mask: Vec<i64>) -> Result<Vec<Vec<f32>>, ColbertError> {
        let seq = ids.len();
        let id_tensor = Tensor::from_array(([1_usize, seq], ids))
            .map_err(|e| ColbertError::Inference(e.to_string()))?;
        let mask_tensor = Tensor::from_array(([1_usize, seq], mask.clone()))
            .map_err(|e| ColbertError::Inference(e.to_string()))?;

        let mut session = self
            .session
            .lock()
            .map_err(|_| ColbertError::Inference("session mutex poisoned".into()))?;

        let outputs = session
            .run(ort::inputs![
                "input_ids" => id_tensor,
                "attention_mask" => mask_tensor,
            ])
            .map_err(|e| ColbertError::Inference(e.to_string()))?;

        // The contextualized projection is the single float output of the graph.
        let mut output_iter = outputs.iter();
        let (_name, output_value) = output_iter
            .next()
            .ok_or_else(|| ColbertError::Shape("no output tensor".into()))?;
        let (shape, data) = output_value
            .try_extract_tensor::<f32>()
            .map_err(|e| ColbertError::Shape(e.to_string()))?;

        let dim = self.config.dim;
        if shape.len() != 3 || shape[2] as usize != dim {
            return Err(ColbertError::Shape(format!(
                "expected [1, seq, {dim}], got {shape:?}"
            )));
        }

        let mut tokens: Vec<Vec<f32>> = Vec::with_capacity(seq);
        for (t, keep) in mask.iter().enumerate().map(|(t, m)| (t, *m == 1)) {
            if !keep {
                continue;
            }
            let start = t * dim;
            let row = &data[start..start + dim];
            tokens.push(l2_normalize(row));
        }
        Ok(tokens)
    }
}

/// L2-normalize a token vector so MaxSim's dot product equals cosine.
fn l2_normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter().map(|x| x / norm).collect()
    } else {
        v.to_vec()
    }
}

impl TokenEmbedder for ColbertEmbedder {
    fn embed_query_tokens(&self, text: &str) -> Result<Vec<Vec<f32>>, String> {
        let (ids, mask) = self.build_query(text).map_err(|e| e.to_string())?;
        self.run(ids, mask).map_err(|e| e.to_string())
    }

    fn embed_document_tokens(&self, text: &str) -> Result<Vec<Vec<f32>>, String> {
        let (ids, mask) = self.build_document(text).map_err(|e| e.to_string())?;
        self.run(ids, mask).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2_normalize_unit_length() {
        let n = l2_normalize(&[3.0, 4.0]);
        let len = (n[0] * n[0] + n[1] * n[1]).sqrt();
        assert!((len - 1.0).abs() < 1e-6);
        assert!((n[0] - 0.6).abs() < 1e-6 && (n[1] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn l2_normalize_zero_vector_is_safe() {
        assert_eq!(l2_normalize(&[0.0, 0.0]), vec![0.0, 0.0]);
    }

    #[test]
    fn config_defaults_match_colbertv2() {
        let c = ColbertConfig::default();
        assert_eq!(c.query_maxlen, 32);
        assert_eq!(c.dim, 128);
    }
}
