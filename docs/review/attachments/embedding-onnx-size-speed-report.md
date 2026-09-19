# ONNX/CPU Size & Speed Reality Check — Text Embedding Models

All file sizes pulled from the HF API tree endpoint (bytes, direct from the file listing). MB = 10^6 bytes. "fastembed dl" = exactly what `fastembed` 7.0.1 fetches, from its source list.

## A. File sizes (repo · filename · bytes · precision)

| Model | Repo | File(s) | Bytes | MB | Prec. |
|---|---|---|---|---|---|
| nomic-embed-text-v1.5 | `nomic-ai/nomic-embed-text-v1.5` (created 2024-02-10) | `onnx/model.onnx` | 547,310,275 | 547.3 | fp32 |
| | | `onnx/model_quantized.onnx` (fastembed dl) | 137,296,292 | 137.3 | int8 |
| | | `onnx/model_q4.onnx` / `model_q4f16.onnx` / `model_fp16.onnx` | 165,113,221 / 111,076,526 / 273,859,028 | 165.1 / 111.1 / 273.9 | 4b / 4b-f16 / fp16 |
| embeddinggemma-300m | `onnx-community/embeddinggemma-300m-ONNX` (2025-08-22, last mod 2025-09-04) | `onnx/model.onnx` + `model.onnx_data` | 479,932 + 1,234,521,088 | **1235.0** | fp32 |
| | | `model_quantized.onnx` + `_data` (fastembed dl) | 567,874 + 308,890,624 | 309.5 | int8 |
| | | `model_q4.onnx` + `_data` | 519,322 + 196,725,760 | 197.2 | 4-bit |
| | | `model_q4f16.onnx` + `_data` | 705,221 + 175,410,176 | 176.1 | 4b-f16 |
| | | `tokenizer.json` (extra, always downloaded) | 20,323,312 | 20.3 | — |
| Qwen3-Embedding-0.6B | `onnx-community/Qwen3-Embedding-0.6B-ONNX` (2025-06-05) | `onnx/model.onnx` + `model.onnx_data` | 307,161,415 + 2,093,436,928 | **2400.6** | fp32 |
| | | `model_q4.onnx` (single file) | 914,121,462 | 914.1 | 4-bit |
| | | `model_quantized.onnx` / `model_uint8.onnx` | 613,527,631 | 613.5 | int8 |
| | | `model_q4f16.onnx` / `model_int8.onnx` / `model_fp16`+`_data` | 567,458,583 / 613,527,539 / 1,200,510,818 | 567.5 / 613.5 / 1200.5 | — |
| | `Qdrant/Qwen3-Embedding-0.6B-onnx` (2026-08-18) | `onnx/model.onnx` + `_data` | 4,031,521 + 2,383,106,048 | 2387.1 | fp32 |
| | | `onnx/model_quantized.onnx` | 1,120,604,114 | 1120.6 | int8 |
| granite-107m-multiling. | `ibm-granite/granite-embedding-107m-multilingual` (2024-12-04) | `model.onnx` (root, no `onnx/` dir) | 428,102,968 | 428.1 | fp32 |
| granite-278m-multiling. | `ibm-granite/granite-embedding-278m-multilingual` | `model.onnx` | 1,112,413,925 | 1112.4 | fp32 |
| bge-m3 | `BAAI/bge-m3` (2024-01-27) | `onnx/model.onnx` + `model.onnx_data` | 724,923 + 2,266,820,608 | 2267.5 | fp32 |
| mxbai-embed-large-v1 | `mixedbread-ai/mxbai-embed-large-v1` | `onnx/model.onnx` / `model_quantized.onnx` / `model_fp16.onnx` | 1,336,854,282 / 336,983,163 / 668,828,772 | 1336.9 / 337.0 / 668.8 | fp32/int8/fp16 |
| arctic-embed-l-v2.0 | `Snowflake/snowflake-arctic-embed-l-v2.0` | `onnx/model.onnx` + `_data` | 702,280 + 2,266,886,160 | 2267.6 | fp32 |
| | | `model_quantized.onnx` (=int8=uint8) | 569,721,975 | 569.7 | int8 |

**Granite has no quantized ONNX in the official repos — one fp32 file only.** My arithmetic: fp32 ≈ 4 bytes/param (nomic 547 MB/137M, granite-107m 428 MB). No arithmetic needed for quantized — those are published files.

## B. Speed numbers (none are x86/Apple Silicon CPU for the target models)

| Number | Metric | Hardware | Source / date | Class |
|---|---|---|---|---|
| <15 ms | 256-token embedding | **EdgeTPU** | [Google blog](https://developers.googleblog.com/en/introducing-embeddinggemma/) 2025-09-04 | vendor |
| <200 MB | RAM, QAT-quantized | unspecified | same | vendor |
| 374 ms | warm, 128-token query (n=30) | Snapdragon 8 Gen 3, stock **ORT CPU EP**, `model_quantized.onnx` | [HF discussion #25](https://huggingface.co/onnx-community/embeddinggemma-300m-ONNX/discussions/25) 2026-05-22 | 3rd party |
| 417 tok/s | peak, @512 tokens | same | same | 3rd party |
| 1.82 GB | peak RSS (6× file size) | same | same | 3rd party |
| 299.9 / 418.2 ms | short / long p50 | unstated CPU | [claude-dejavu bench](https://github.com/HthSolid/claude-dejavu/blob/main/docs/MODEL_BENCHMARK.md) 2026-04 | 3rd party |
| 2.0 docs/s | corpus throughput, 2000-char docs | unstated CPU | same | 3rd party |
| 1603 MB | RSS, nomic-v1.5 | unstated CPU | same | 3rd party |
| 2.7–3.4× | INT8 vs FP32 CPU speedup (94–98% quality kept) | Graviton3/4, **ONNX Runtime** | [Vespa blog](https://blog.vespa.ai/embedding-tradeoffs-quantified/) 2026-01-14 | 3rd party |
| 2.5 ms | e5-small-v2 INT8 query | Graviton3 | same | 3rd party |
| 2076.8 ms | CPU XNNPACK 4 threads (LiteRT, **not ORT**) | Pixel 8a Tensor G3 | [LiteRT card](https://huggingface.co/litert-community/Qwen3-Embedding-0.6B-LiteRT) 2026-08-27 | 3rd party |

## C. NOT FOUND — do not extrapolate

- **No vendor CPU (x86 or Apple Silicon) latency for any of the 6 models.** EmbeddingGemma's only vendor number is EdgeTPU; its [arXiv paper 2509.20354](https://arxiv.org/abs/2509.20354) (2025-09-24) contains **zero** latency/throughput/memory figures (verified full-text).
- **Qwen3-Embedding**: [blog](https://qwenlm.github.io/blog/qwen3-embedding/) and [tech report 2506.05176](https://arxiv.org/abs/2506.05176) (2025-06-05) publish MTEB scores only — no latency, throughput, or RAM.
- **nomic**: [arXiv 2402.01613](https://arxiv.org/abs/2402.01613) (2024-02-02) and the model card publish no speed/memory numbers.
- **Granite**: only "twice as fast as other models with similar embedding dimensions" — no absolute value, no hardware, no date.
- **bge-m3 / mxbai / arctic-v2.0**: no CPU latency or RAM found in model cards or papers.
- **No published ONNX Runtime CPU benchmark on Apple Silicon for these models.** Only qualitative: [fastembed #535](https://github.com/qdrant/fastembed/issues/535) (2025-06-19) — on M2 Max, fastembed was "way slower" than sentence-transformers because ORT ran CPU-only while PyTorch used MPS. No ms figures.

## D. fastembed 7.0.1 integration gotchas (source-verified)

- **Qwen3-Embedding-0.6B is candle-only** (`qwen3` feature), **not** ONNX. `Qdrant/Qwen3-Embedding-0.6B-onnx` (2026-08-18) exists but is unlisted in fastembed.
- **granite-embedding-107m/278m-multilingual: zero matches** in fastembed's README and model list → `UserDefinedEmbeddingModel` only.
- Supported on ONNX: nomic-v1.5 (+Q), embeddinggemma-300m (+Q4/+Q), bge-m3, mxbai-embed-large-v1 (+Q), **snowflake-arctic-embed-l v1** — not v2.0.
- `ort` 2.0.0-rc.12 released 2026-03-05 ([crates.io](https://crates.io/crates/ort/versions)); rc.13 is 2026-07-28.
