# ANN Libraries & Vector Quantization for a Local Rust + SQLite Memory Server
Verified **2026-09-19**. Uncertainties are flagged at the end; bracketed numbers map to Sources.

## 1. USearch
crates.io `usearch` latest = **2.26.2 (2026-08-31)**, **Apache-2.0**, edition 2024 [1]. Maintained by ashvardanian, roughly monthly releases [2]. The app's 2.25.2 (2026-05-02) is four releases behind [2]:
- **2.25.3** (2026-05-24) + **2.26.0** (2026-07-10): quantized-cast guards (#758); `stats()` in the Rust SDK (#768); Rust **compact binding** (#771); `slot_lookup_` tombstone reclaim (#769).
- **2.26.1** (2026-08-22): **"Bound file-controlled sizes in `view()`" (#776)** — OOB read on a crafted index (levels region + dense-vector span).
- **2.26.2** (2026-08-31): "Retry Linux file mapping without `O_NOATIME`" (#784).

**Quantization.** Types `f64, f32, bf16, f16, e5m2, e4m3, e3m2, e2m3, u8, i8, b1x8`; the Rust crate exposes f32/f64/i8/u8/f16/b1x8 [3][4]. Docs recommend **bf16**. **i8 is valid only for cosine-like metrics** (L2-normalized, scaled to [−127, 127]); **b1x8 only for binary metrics** (Jaccard/Hamming); quantized "get" calls cannot recover originals [3]. **No recall tables are published**; savings are implicit in element width (f16/bf16 2×, i8/u8 4×, b1x8 32×), never measured [3].

**SimSIMD → NumKong.** 2.25.2 shipped "Switch from SimSIMD v6 to NumKong v7 submodule"; features are now `default=["numkong"]` with `simsimd = ["numkong"]` as an alias, dep `numkong >=7.5.0,<8` [1][4]. SimSIMD was renamed incompatibly upstream and last-rited in Gentoo (bug 972000, 2026-04-04; removed 2026-05-19) [5][6].

**"TurbQuant" is not a USearch feature**; the real thing is **TurboQuant** [7], implemented in `sqliteai/sqlite-vector` (below).

## 2. Rust alternatives (crates.io API, 2026-09-19)
- `usearch` 2.26.2 (2026-08-31), Apache-2.0 — active; needs a **C++ toolchain** (cxx + numkong) [1].
- `hnsw_rs` 0.3.4 (2026-02-28), MIT/Apache-2.0 — active (pushed 2026-09-12); master unreleased 0.3.5.
- `arroy` 0.8.0 (2026-08-12), MIT — pure Rust on heed/LMDB [10].
- `diskann` 0.59.0 (2026-09-11), MIT — **Microsoft**, newly pure-Rust deps [9].
- `lancedb` 0.39.0 / `lance` 12.0.0 (2026-09-17), Apache-2.0 — very active.
- `qdrant-edge` 0.8.0 (2026-08-05), Apache-2.0 — Qdrant's only published embedded crate [11].
- `faiss` (faiss-rs) 0.13.0 (2025-11-15), MIT/Apache-2.0 — ~10 months idle; needs C++ FAISS.
- `kannolo`: crates.io 0.3.8 (2026-03-04) vs GitHub v0.11.2 (2026-09-13), MIT — **version mismatch**.
- `tantivy` 0.26.2 (2026-09-08), MIT — **no vector/ANN module**; `src/` has no `vector` dir and the CHANGELOG has zero hnsw/knn entries [13].
- Dead: `instant-distance` 0.6.1 (2023-06-26) **ARCHIVED**; `hora` 0.1.1 (2021-08-07); `hnsw` rust-cv 0.11.0 (2021-07-19); `annoy` 0.1.0 (2019, **GPL-3.0**).

**Qdrant's HNSW has no standalone crate** — it lives in the unpublished workspace crate `lib/segment` [11]. **`symphony` is not an ANN crate**; **`pgvector`** is a PostgreSQL extension, not embeddable.

## 3. SQLite vector extensions
**sqlite-vec** — latest **v0.1.10-alpha.4 (2026-05-18, prerelease)**; last stable **0.1.9 (2026-03-31)**. Crate = **FFI bindings compiling the C amalgamation**; **MIT/Apache-2.0**; repo not archived, 8,117★, `pushed_at` 2026-05-18 (~4 months quiet) [12]. README: _"pre-v1, so expect breaking changes."_
- **Brute-force only** — author's docs: "sqlite-vec … is (currently) brute-force only" [15].
- Types `float32`, `int8`, `bit`; `vec_quantize_binary()`, `vec_distance_hamming()`; **`vec_quantize_i8` is "todo"**; `vec_normalize()`+`vec_slice()` for Matryoshka.
- Limits: max 16 metadata columns, 4 partition keys, 16 auxiliary columns; KNN needs `k = ?` (or `LIMIT` on SQLite ≥3.41); **no range queries**. **No max-dimension limit is documented** [16].
- Its "Benchmarks" sections are **empty placeholders**. Independent July-2026 measurement: 1M×512-dim exact scan = **2.5 QPS (395 ms/query)**; "fine below ~50k vectors, not an ANN competitor at scale" [17].

**vectorlite** — Apache-2.0, hnswlib + Google Highway SIMD. Latest **tagged release v0.2.0 (2024-08-19)**, but `main` pushed **2026-09-13**. Limits: **float32 only** (f16/int8 unchecked on roadmap), index always in memory (explicit `insert into t(operation,path) values('save',…)`), **no transactions**, one vector column per table. Self-reported 3×–100× faster than sqlite-vec; recall at ef=10 falls to **13–29% at 20k vectors** [18].

**sqlite-vss** — last push **2024-05-05**, MIT, effectively dead; sqlite-vec calls itself its successor [12].

**sqliteai/sqlite-vector** — **1.1.2 (2026-09-11)**, Apache-2.0; **TurboQuant lives here** [7][8]. Float32/Float16/BFloat16/Int8/UInt8/1Bit plus TurboQuant 2/3/4-bit; no virtual tables, **no preindexing**. Bench, 1M×768-dim cosine, k=20, Apple M5 Pro: FLOAT32 2930 MB / 484.4 ms / 100%; **INT8 740 MB / 37.6 ms / 99.5%**; 1BIT 99 MB / 2.5 ms / 10.0%; TURBO4 378 MB / 151.5 ms / 81.8%; TURBO2 195 MB / 48.0 ms / 45.2% [8].

**Turso/libSQL** — native `vector32/64`, `vector32_sparse`, `vector8` (~4×), `vector1bit` (~32×). Limits: **max dimensionality 65,536**; L2 unsupported for `vector1bit`, whose cosine returns Hamming distance; _"Similarity searches use a linear scan over the table"_ — **no ANN index** [19].

## 4. Quantization for embeddings
**int8 (4×).** Best 384d/768d evidence: MTEB **nDCG@10** — bge-small 384d **0.595 → 0.574**; nomic-embed-text-v1.5 768d **0.593 → 0.584**; it also finds **float8_e4m3 beats int8 at the same 4×** [20]. Qdrant: error "usually <1%"; Arxiv-titles-384 precision 0.989→0.986 @ef=128 [21]. Weaviate docs: SQ **95–97%** recall, RQ **98–99%** at the same 4× [22].

**Binary + rescore (32×).** The cited "~96%" traces to a HuggingFace vendor benchmark (2024-03-22): "up to ~96% with rescoring, ~92.5% without." Model-specific — miniLM 384d **93.79%**, nomic 768d **87.7%**, e5-base 768d **74.77%** (dimension collapse) [23]. Qdrant DBpedia 100K recall@100: Gemini 768d **0.9563** @3× oversampling, ada-002 0.98 @4×; Qdrant warns BQ is poor **below 1024 dims** [21][24]. Weaviate measured 768d BQ recall **0.745** [25]; the pattern originates in BPR, which *trains* the hash [26].

**Best 384d/768d recall@10 table:** 2-bit sign-magnitude + float32 rerank, 1M vectors — MiniLM 384d **88.1% @ef=64 → 98.4% @ef=512**; Cohere 768d **95.1% → 99.6%**. Failure boundary is sharp: SIFT-128 (Euclidean) 14.85%, random-768 **0.40%** [27].

**PQ is largely superseded**: Qdrant recommends it only "if memory footprint is the top priority and accuracy and speed are not critical"; it is slower than SQ [21]. TurboQuant 4-bit beat trained FAISS PQ by **8.5–8.9 pp Recall@5** at equal budget [7]. **Matryoshka (768→384):** nomic-embed-text-v1.5 retains **95.8% at 3×, 90% at 6×** compression [23].

## 5. Persistence & durability
USearch supports file, stream, buffer and **mmap view** ("view large indexes from disk without loading into RAM"; README claims up to 20× cost reduction) [3]. Recent mmap/format fixes: `view()` OOB read in **2.26.1** (#776), `O_NOATIME` retry in **2.26.2** (#784), FreeBSD mmap flags in 2.24.0 (#711), allocator accounting in 2.25.2 (#608) [2]. **No index-format version/compatibility guarantee is documented** (unverified).

Embedded-app guidance: **ctxd ADR-014 (2026-04-24)** uses `hnsw_rs` 0.3.4 with sidecars (`.hnsw.graph/.data/.meta/.map`), a magic header + version byte, SQLite as source of truth with rebuild on element-count mismatch, flush every 1000 inserts, **~5 ms rebuild at 10k vectors**; `hnsw_rs` **panics on a malformed graph file** and needs a `Box::leak` hack [28]. horosvec (2026-07) reports a SQLite file + fp16 mmap sidecar that "opens without a rebuild and survives restarts", warning the storage medium alone moved p50 from ~2.9 s to **7.8–27.6 ms** [17].

## Uncertainty flags
1. **"TurbQuant"** does not exist in USearch; TurboQuant is a separate quantizer in `sqliteai/sqlite-vector` [7][8].
2. **USearch publishes no recall numbers** for i8/bf16/b1x8 [3]; **no primary source gives int8 recall@k for 384d/768d** [20].
3. Vendors **disagree on BQ below 1024 dims**: Qdrant warns against it [21], Elasticsearch defaults ≥384d to `bbq_hnsw`, yet the 384d study reports 88.1% [27].
4. **float8 vs int8** conflict: [20] favours float8; no vendor doc addresses it.
5. sqlite-vec's **max dimensions are undocumented** [12][16].
6. vectorlite/sqlite-vec benchmarks are **self-reported by competing authors** [17][18]; Elastic/Lucene's exact scalar-quantization recall figures exist only inside a figure.

## Sources
[1]https://crates.io/api/v1/crates/usearch
[2]https://github.com/unum-cloud/USearch/releases.atom
[3]https://raw.githubusercontent.com/unum-cloud/usearch/main/README.md
[4]https://docs.rs/usearch/latest/usearch/
[5]https://bugs.gentoo.org/972000
[6]https://github.com/ashvardanian/NumKong
[7]https://arxiv.org/abs/2504.19874
[8]https://github.com/sqliteai/sqlite-vector
[9]https://github.com/microsoft/DiskANN
[10]https://github.com/meilisearch/arroy
[11]https://github.com/qdrant/qdrant
[12]https://github.com/asg017/sqlite-vec
[13]https://github.com/quickwit-oss/tantivy
[14]https://crates.io/api/v1/crates/sqlite-vec
[15]https://alexgarcia.xyz/sqlite-vec/guides/binary-quant.html
[16]https://alexgarcia.xyz/sqlite-vec/features/vec0.html
[17]https://github.com/hazyhaar/horosvec/blob/main/docs/BENCHMARK-2026-07.md
[18]https://github.com/1yefuwang1/vectorlite
[19]https://docs.turso.tech/guides/vector-search
[20]https://arxiv.org/abs/2505.00105
[21]https://qdrant.tech/documentation/manage-data/quantization/
[22]https://docs.weaviate.io/weaviate/starter-guides/managing-resources/compression
[23]https://huggingface.co/blog/embedding-quantization
[24]https://qdrant.tech/articles/binary-quantization/
[25]https://weaviate.io/blog/binary-quantization
[26]https://arxiv.org/abs/2106.00882
[27]https://arxiv.org/abs/2605.02171
[28]https://github.com/keeprlabs/ctxd/blob/feat/onboard-v0.4/docs/decisions/014-hnsw-persistence.md
