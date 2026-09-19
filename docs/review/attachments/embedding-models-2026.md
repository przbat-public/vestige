# Local vs Cloud Embeddings — Research Report (as of 2026-09-19)

## PART 1 — Google's hosted embedding API

| Model | Dates | Dims | Max input | Task types | Google-reported MTEB | Price /1M input | Local? |
|---|---|---|---|---|---|---|---|
| `gemini-embedding-exp-03-07` | Released **2025-03-07**; shut down **2025-10-30** | 3072 | 2048 | `task_type` | n/a | retired | No |
| `gemini-embedding-001` | Released/GA **2025-07-14**; shutdown announced **2028-05-14** | 3072 default, MRL 128–3072 | **2,048** | `SEMANTIC_SIMILARITY, CLASSIFICATION, CLUSTERING, RETRIEVAL_DOCUMENT, RETRIEVAL_QUERY, CODE_RETRIEVAL_QUERY, QUESTION_ANSWERING, FACT_VERIFICATION` | MTEB multilingual (Google's table): **68.16**@2048, 68.17@1536, 67.99@768, 67.55@512, 66.19@256, 63.31@128 | **Not listed** on the current pricing page (see caveats) | No |
| `gemini-embedding-2-preview` | Released **2026-03-10**; shutdown **2026-08-10** | 128–3072 | 8,192 | prompt prefixes | n/a | $0.20 text | No |
| `gemini-embedding-2` | **GA 2026-04-22** | 128–3072 (rec. 768/1536/3072) | **8,192** text tokens | prompt prefixes: `task: search result \| query:`, `question answering`, `fact checking`, `code retrieval`, `classification`, `clustering`, `sentence similarity`; docs `title: {t} \| text: {c}` | not stated on model page | **$0.20** text; $0.45 image; $6.50 audio; $12.00 video; Batch = 50% off ($0.10 text) | No |

**Key changes in Gemini Embedding 2:** first natively multimodal Gemini embedder (text/image/video/audio/PDF, one space, 100+ languages); `task_type` is **removed** — inline the task as a prompt prefix; truncated dims are **auto-renormalized** (for `gemini-embedding-001` you must L2-normalize non-3072 dims yourself). Limits: 6 images, 180s audio, 120s video (32 frames), 6 PDF pages.

**Caveats.** (a) `gemini-embedding-001` **no longer appears on the Gemini pricing page**; the widely cited $0.15/1M is **unverified** against a live primary source. (b) Google's GA blog (2025-07-14) claims a "top spot on MTEB Multilingual" but gives no number. (c) Vertex AI serves the same models; no separate Vertex-only embedding model found.

**Open-weights from Google beyond EmbeddingGemma:** **none found.** `google/embeddinggemma-300m` (HF created **2025-07-17**; paper [arXiv:2509.20354](https://arxiv.org/abs/2509.20354), submitted **2025-09-24**) is still the newest; the `google` HF org has only that model plus QAT variants. No "EmbeddingGemma 2", no Gemma embedding successor in 2026. Searching surfaced only third-party fine-tunes (e.g. `NorskHelsenett/eti-embeddinggemma-v2`), which are **not** Google releases. **API-only is the answer for all Gemini embedding models — no downloadable weights.**

## PART 2 — Notable embedding models, open vs API (2026)

### Open-weights

| Model | Release | Params | Dims (MRL) | Ctx | License | ONNX | MTEB |
|---|---|---|---|---|---|---|---|
| **microsoft/harrier-oss-v1-27b** | 2026-03-30 | 27B | 5376 | 32,768 | MIT | **No** | **74.3** MTEB v2 (multilingual) |
| **harrier-oss-v1-0.6b** | 2026-03-30 | 0.6B | 1024 | 32,768 | MIT | No | **69.0** MTEB v2 |
| **harrier-oss-v1-270m** | 2026-03-30 | 270M | 640 | 32,768 | MIT | No | **66.5** MTEB v2 |
| **jina-embeddings-v5-text-small** | 2026-02-18 | 677M | 1024 (32–1024) | 32,768 | **CC-BY-NC-4.0** | No | **71.7** MTEB English v2 / 67.7 MMTEB |
| **jina-embeddings-v5-text-nano** | 2026-02-18 | 239M | 768 (32–768) | 8,192 | CC-BY-NC-4.0 | No | 71.0 MTEB Eng v2 / 65.5 MMTEB |
| **voyageai/voyage-4-nano** | 2026-01-06 | 180M+160M emb | 2048 (256–2048) | 32,000 | **Apache-2.0** | Community only | "outperforms voyage-3.5-lite" (vendor) |
| **Qwen3-Embedding-8B / 4B / 0.6B** | 2025-06-03 | 8B/4B/0.6B | ≤4096 (32–4096) | 32,768 | Apache-2.0 | 0.6B yes | **70.58** MTEB multilingual (2025-06-05) |
| **snowflake-arctic-embed-l-v2.0** | 2024-11-08 | 568M (303M non-emb) | 1024 (MRL@256) | 8,192 | Apache-2.0 | **Yes** | BEIR-15 **55.6**, MIRACL 55.8, CLEF 52.9 |
| **mxbai-embed-large-v1** | 2024-03-07 | 335M | 1024 | 512 | Apache-2.0 | **Yes** | **64.68** (card, MTEB 2024) |
| **jina-embeddings-v3** | 2024-09-05 | ~570M | 1024 (32–1024) | 8,192 | CC-BY-NC-4.0 | **Yes** | 30 tuned languages |
| **jina-embeddings-v4** | 2025-05-07 | 3B (Qwen2.5-VL) | 2048 (128–2048) | 32,768 | **Qwen Research License** | GGUF | multimodal; arXiv 2506.18902 |
| **gte-large-en-v1.5 / base-en-v1.5** | 2024 | 434M / 137M | 1024 / 768 | 8,192 | Apache-2.0 | Yes | 65.39 / 64.11 (card) |
| **thenlper/gte-large** | 2023-07-27 | 335M | 1024 | 512 | MIT | Yes | superseded |
| **gte-Qwen2-1.5B / 7B-instruct** | 2024-06 | 1.5B / 7B | 1536 / 3584 | 32,768 | Apache-2.0 | No | 7B ≈ top-5 MTEB 2024 |
| **dunzhang/stella_en_1.5B_v5** | 2024-07-12 | 1.5B | 1024 (512–8192) | 8,192 | MIT | **No** | SOTA-class 2024 |
| **stella_en_400M_v5** | 2024-07-12 | 400M | 1024 | 8,192 | MIT | No | — |
| **BAAI/bge-m3** | 2024-01-27 | 568M | 1024 | 8,192 | MIT | **Yes** | BEIR-15 48.8, MIRACL **56.8** |
| **BAAI/bge-en-icl** | 2024 | 7B | 4096 | 32,768 | Apache-2.0 | No | strong nDCG@10 (0.7048) |

**Successors that do NOT exist** (verified via HF API org listings, 2026-09-19): no `mxbai-embed-2` (newest mixedbread embedding: `mxbai-embed-xsmall-v1`, 2024-09-13); no arctic-embed v3 (newest: l-v2.0); no `BAAI/bge-m4`; no stella past v5; no 2026 GTE.

### API-only

| Model | Release | Dims | Ctx | Price /1M |
|---|---|---|---|---|
| voyage-4-large / 4 / 4-lite | 2026-01-15 | 2048 (256–2048) | 32,000 | $0.12 / $0.06 / $0.02 |
| voyage-3.5 / 3.5-lite | 2025-05-20 | 2048 (256–2048) | 32,000 | $0.06 / $0.02 |
| voyage-3 / 3-large | 2024-09-17 / 2024-12-11 | 1024 / 2048 | 32,000 | legacy |
| Cohere embed-v4.0 | 2025-04-15 | 256/512/1024/**1536** | **128k** | **Not public** |
| OpenAI text-embedding-3-large / 3-small | 2024-01-25 | 3072 / 1536 | 8,191 | $0.13 / $0.02 |

No Cohere embed-v5 found. Per-token Cohere pricing is **not published** on `cohere.com/pricing` (only Model Vault instance rates: Embed 4 Small $4.00/hr, $2,500/mo).

## MTEB state of play (2026)

The live leaderboard Space is now a Docker app (`models.py` only — no static data), so I **could not verify ranks directly**. A news snapshot ([the-decoder, 2026-04-07](https://the-decoder.com/microsofts-bing-team-open-sources-harrier-embedding-model/), multilingual MTEB v2 Borda rank) lists: **1. harrier-oss-v1-27b** (78% zero-shot), 2. KaLM-Embedding-Gemma3-12B-2511, 3. llama-embed-nemotron-8b, 4. Qwen3-Embedding-8B, 5. gemini-embedding-001, 6. Qwen3-Embedding-4B, 10. harrier-oss-v1-0.6b. The benchmark itself is **MMTEB**, [arXiv:2502.13595](https://arxiv.org/abs/2502.13595) (2025-02-19, v4 2025-11-13).

**Disagreement to flag:** harrier-oss-v1-27b max tokens is **32,768** on the model card but **131,072** in the news table. Google's "top spot on MTEB Multilingual" claim (2025) vs. rank 5 in 2026 is a genuine reversal, not a conflict.

## Practical note for `fastembed` (Rust)

`fastembed-rs` supports bge-m3, mxbai-embed-large-v1, gte-base/large-en-v1.5, `embeddinggemma-300m`, nomic-embed-text-v1.5/v2-moe, `Qwen3-Embedding-0.6B/4B/8B` (candle), and snowflake-arctic-embed **xs/s/m/m-long/l (v1 only)**. It does **not** expose arctic-embed **v2.0**, jina v3/v4/v5, stella, gte-Qwen2, Harrier, or voyage-4-nano — those need custom ONNX (arctic v2.0, jina v3, mxbai ship ONNX; Harrier, jina v5, voyage-4-nano do not officially).

## Sources
Gemini: [embeddings docs](https://ai.google.dev/gemini-api/docs/embeddings) · [embedding-2 model page](https://ai.google.dev/gemini-api/docs/models/gemini-embedding-2) (updated 2026-04-28) · [embedding-001 model page](https://ai.google.dev/gemini-api/docs/models/gemini-embedding-001) · [changelog](https://ai.google.dev/gemini-api/docs/changelog) · [deprecations](https://ai.google.dev/gemini-api/docs/deprecations) · [pricing](https://ai.google.dev/gemini-api/docs/pricing) · [GA blog 2025-07-14](https://developers.googleblog.com/en/gemini-embedding-available-gemini-api/) · [Embedding 2 blog 2026-04-30](https://developers.googleblog.com/en/building-with-gemini-embedding-2/) · [EmbeddingGemma paper](https://arxiv.org/abs/2509.20354)
Models: [harrier-oss-v1-27b](https://huggingface.co/microsoft/harrier-oss-v1-27b) · [jina v5-small](https://huggingface.co/jinaai/jina-embeddings-v5-text-small) · [jina v4](https://huggingface.co/jinaai/jina-embeddings-v4) · [voyage-4-nano](https://huggingface.co/voyageai/voyage-4-nano) · [voyage-4 blog 2026-01-15](https://blog.voyageai.com/2026/01/15/voyage-4/) · [Voyage pricing](https://docs.voyageai.com/docs/pricing) · [Qwen3-Embedding-8B](https://huggingface.co/Qwen/Qwen3-Embedding-8B) · [arctic-embed-l-v2.0](https://huggingface.co/Snowflake/snowflake-arctic-embed-l-v2.0) · [Arctic-Embed 2.0 paper](https://arxiv.org/abs/2412.04506) · [mxbai-embed-large-v1](https://huggingface.co/mixedbread-ai/mxbai-embed-large-v1) · [bge-m3](https://huggingface.co/BAAI/bge-m3) · [gte-large-en-v1.5](https://huggingface.co/Alibaba-NLP/gte-large-en-v1.5) · [Cohere Embed 4 blog 2025-04-15](https://cohere.com/blog/embed-4) · [Cohere embed docs](https://docs.cohere.com/docs/cohere-embed) · [Cohere pricing](https://cohere.com/pricing) · [OpenAI text-embedding-3-large](https://developers.openai.com/api/docs/models/text-embedding-3-large) · [MMTEB paper](https://arxiv.org/abs/2502.13595) · [fastembed-rs](https://github.com/Anush008/fastembed-rs)
