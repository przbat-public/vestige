# Production operational research: long-lived local-first MCP memory server

**Scope:** Rust binary, SQLite (WAL + FTS5 + vector index), ~28 MCP tools over stdio + streamable HTTP, local ONNX embeddings (nomic-embed-text-v1.5) and local cross-encoder reranker (Jina Reranker v2 Base Multilingual), background consolidation loops.

**Research date:** 2026-09-19. Every claim below carries a source URL and a publication / last-updated / version date. Unverified, deprecated, or conflicting items are flagged explicitly. I did not run benchmarks myself; all numbers are from cited sources.

---

## 🚨 LICENSING LANDMINE (read this first)

**`jinaai/jina-reranker-v2-base-multilingual` is CC-BY-NC-4.0 — NON-COMMERCIAL.** This is the single highest-severity finding in this report.

- HuggingFace API `cardData.license` = `cc-by-nc-4.0`; tag `license:cc-by-nc-4.0`; region `eu` ([HF API, model lastModified 2025-10-21](https://huggingface.co/jinaai/jina-reranker-v2-base-multilingual)).
- The model card states verbatim: *"This model repository is licenced for research and evaluation purposes under CC-BY-NC-4.0. For commercial usage, please refer to Jina AI's APIs, AWS Sagemaker or Azure Marketplace offerings."* ([model card README.md, line 41](https://huggingface.co/jinaai/jina-reranker-v2-base-multilingual/raw/main/README.md), retrieved 2026-09-19).
- Model is 278,437,633 parameters ([HF API `safetensors.total`](https://huggingface.co/api/models/jinaai/jina-reranker-v2-base-multilingual)).
- `fastembed-rs` ships this model as `RerankerModel::JinaRerankerV2BaseMultiligual` ([fastembed README, Reranking section](https://github.com/Anush008/fastembed-rs), retrieved 2026-09-19) — but fastembed's own Apache-2.0 licence **does not and cannot** relicense the weights. fastembed's Cargo.toml declares `license = "Apache-2.0"` for the *code only* ([fastembed Cargo.toml, v7.0.1](https://raw.githubusercontent.com/Anush008/fastembed-rs/main/Cargo.toml)).

**Consequences for Vestige:** shipping a commercial product that downloads/uses these weights, or bundling them, is a licence violation. Options: (a) make the reranker an explicit opt-in that the *user* downloads, with the NC notice surfaced (legally weaker — redistribution is still redistribution); (b) replace with a permissively-licensed reranker (e.g. `BAAI/bge-reranker-base` / `bge-reranker-v2-m3`, both Apache-2.0 per their HF cards — **I did not re-verify these two licences via the HF API in this session; verify before switching**); (c) license the Jina API/enterprise offering. This must be resolved before any commercial release.

---

# 1. SQLite in production for a concurrent MCP server

## 1.1 Current SQLite version and the WAL-reset bug (time-critical)

- **Current release: SQLite 3.53.4, 2026-07-24.** ([sqlite.org/changes.html](https://sqlite.org/changes.html), page retrieved 2026-09-19)
- **WAL-reset corruption bug:** "The bug is likely present in all version of SQLite from 3.7.0 (2010-07-21) through 3.51.2 (2026-01-09). It is fixed in version 3.51.3 (2026-03-13) and later." Backports exist for 3.44.6 and 3.50.7. ([sqlite.org/wal.html §11](https://sqlite.org/wal.html), page last updated 2026-08-25)
- **Exact trigger conditions** (this matters for a multi-client MCP server): "The bug only affects databases in WAL mode **when there are two or more database connections open on the same file, in separate threads or processes, and when those two connections attempt to write or checkpoint at the same instant.**" ([sqlite.org/wal.html §11](https://sqlite.org/wal.html), last updated 2026-08-25). **Vestige's exact deployment shape (Claude Desktop + Cursor + dashboard all opening one DB) is the affected configuration.** The 3.53.0 release notes also list "Fix the WAL-reset database corruption bug" ([sqlite.org/changes.html](https://sqlite.org/changes.html), 2026-04-09).
- SQLite's own severity assessment: "the occurrence rate of this problem in the wild appears to be less than or equal to the expected occurrence rate of SSD malfunctions and/or cosmic-ray hits." A non-test-harness reproducer was later published by Phil Eaton ([The Consensus, 2026-08-23](https://theconsensus.dev/p/2026/08/23/another-look-at-sqlite-wal-reset.html), cited in wal.html update 2026-08-24).
- **Action:** pin `libsqlite3-sys` to ≥ 3.53.4 (bundled SQLite 3.53.4 — verified at [`libsqlite3-sys/sqlite3/sqlite3.h`](https://raw.githubusercontent.com/rusqlite/rusqlite/master/libsqlite3-sys/sqlite3/sqlite3.h) → `#define SQLITE_VERSION "3.53.4"`, retrieved 2026-09-19). Current `rusqlite` 0.40.2 / `libsqlite3-sys` 0.38.2 (both published 2026-08-08, [crates.io](https://crates.io/api/v1/crates/rusqlite)).

## 1.2 WAL semantics — the exact documented rules

Source for this subsection unless noted: [sqlite.org/wal.html](https://sqlite.org/wal.html), last updated 2026-08-25.

- **Single-writer rule:** "since there is only one WAL file, there can only be one writer at a time." (§2.2)
- **Reader/writer concurrency:** "readers do not block writers and a writer does not block readers. Reading and writing can proceed concurrently." (§1) Each reader gets a stable point-in-time snapshot via its own "end mark" (§2.2). **No documented upper bound on reader count.**
- **Checkpoint starvation (the real scalability trap):** "if a database has many concurrent overlapping readers and there is always at least one active reader, then no checkpoints will be able to complete and hence the WAL file will grow without bound." Mitigation documented by SQLite: "ensuring that there are 'reader gaps'" or using `SQLITE_CHECKPOINT_RESTART` / `SQLITE_CHECKPOINT_TRUNCATE` — "the disadvantage ... is that readers might block while the checkpoint is running." (§6)
- **A long-running read transaction blocks checkpoint progress:** "a long-running read transaction can prevent a checkpointer from making progress." (§2.2)
- **Read performance degrades with WAL size:** "read performance deteriorates as the WAL file grows in size ... performance still falls off with increasing WAL file size." (§2.3)
- **Network filesystems are unsupported:** "All processes using a database must be on the same host computer; WAL does not work over a network filesystem." (§1) — relevant if the user puts the memory DB on iCloud Drive / Dropbox / a NAS / SMB share. This *will* corrupt.
- **Default autocheckpoint: 1000 pages** ("about 4MB in size" at the default 4096-byte page size). (§2.1, §6)
- **`PRAGMA wal_autocheckpoint=N`:** "a checkpoint will be run automatically whenever the write-ahead log equals or exceeds N pages in length. Setting the auto-checkpoint size to zero or a negative value turns auto-checkpointing off. ... All automatic checkpoints are PASSIVE. Autocheckpointing is enabled by default with an interval of 1000 or SQLITE_DEFAULT_WAL_AUTOCHECKPOINT." ([sqlite.org/pragma.html#pragma_wal_autocheckpoint](https://sqlite.org/pragma.html#pragma_wal_autocheckpoint), retrieved 2026-09-19)
- **WAL mode is persistent across connections:** "The WAL journal mode will be set on all connections to the same database file if it is set on any one connection." (§3.3). So one client setting WAL is enough — but see §1.3 for what breaks anyway.
- **The `-wal` and `-shm` files are part of the database.** "The WAL file is part of the persistent state of the database and should be kept with the database if the database is copied or moved. If a database file is separated from its WAL file, then transactions that were previously committed to the database might be lost, or the database file might become corrupted." (§4). Also: "The only safe way to remove a WAL file is to open the database file ... then immediately close the database."

### `PRAGMA synchronous` — what each level actually guarantees on power loss

Source: [sqlite.org/pragma.html#pragma_synchronous](https://sqlite.org/pragma.html#pragma_synchronous), retrieved 2026-09-19. SQLite publishes this exact matrix:

| | Rollback Mode | WAL Mode |
|---|---|---|
| EXTRA (3) | ACID | ACID |
| FULL (2) | Maybe not durable | **ACID** |
| NORMAL (1) | Maybe not consistent | **Maybe not durable** |
| OFF (0) | Not consistent | Not consistent |

Exact wording per level:
- **EXTRA (3):** "EXTRA is no different from FULL in WAL mode."
- **FULL (2):** "**FULL is atomic, consistent, isolated, and durable (ACID) in WAL mode.**" In WAL mode "an additional sync operation of the WAL file happens after each transaction commit."
- **NORMAL (1):** "WAL mode is safe from corruption with synchronous=NORMAL... **WAL mode is always consistent with synchronous=NORMAL, but WAL mode does lose durability.** A transaction committed in WAL mode with synchronous=NORMAL might roll back following a power loss or system crash. **Transactions are durable across application crashes regardless of the synchronous setting or journal mode.**" And: "The synchronous=NORMAL setting provides the best balance between performance and safety for most applications running in WAL mode."
- **OFF (0):** "the database might become corrupted if the operating system crashes or the computer loses power."
- In WAL+NORMAL, "the checkpoint is the only operation to issue an I/O barrier or sync operation" ([wal.html §2.3](https://sqlite.org/wal.html)).
- TEMP schema is always `synchronous=OFF` and attempts to change it "are silently ignored."

**Recommendation for a memory store:** `synchronous=NORMAL` is the documented "best balance"; you lose at most the last few commits on power loss, never consistency. `FULL` buys per-commit durability at the cost of an fsync per commit — that matters a lot given §1.4's single-writer throughput ceiling.

## 1.3 `SQLITE_BUSY`, `busy_timeout`, and `SQLITE_BUSY_SNAPSHOT`

- **`SQLITE_BUSY` (5):** "the database file could not be written (or in some cases read) because of concurrent activity by some other database connection... SQLite only supports one writer at a time." **Key escape hatch:** "To avoid encountering SQLITE_BUSY errors in the middle of a transaction, the application can use **BEGIN IMMEDIATE** instead of just BEGIN... The BEGIN IMMEDIATE command might itself return SQLITE_BUSY, but if it succeeds, then SQLite guarantees that no subsequent operations on the same database through the next COMMIT will return SQLITE_BUSY." ([sqlite.org/rescode.html §5(5)](https://sqlite.org/rescode.html), retrieved 2026-09-19)
- **`SQLITE_BUSY_SNAPSHOT` (517):** "occurs on WAL mode databases when a database connection tries to promote a read transaction into a write transaction but finds that another database connection has already written to the database and thus invalidated prior reads." Documented scenario: Process A starts a read txn and keeps it open; Process B updates; A tries to write → `SQLITE_BUSY_SNAPSHOT`. **This cannot be resolved by retrying on the same snapshot — the transaction must be rolled back and restarted.** ([sqlite.org/rescode.html (517)](https://sqlite.org/rescode.html)). A SQLite forum thread reports a case where `SQLITE_BUSY_SNAPSHOT` is *not* returned on transaction upgrade ([SQLite User Forum](https://sqlite.org/forum/forumpost/a2049876cc?t=h)) — flagged as a possible edge case, not confirmed as a bug by me.
- **`SQLITE_BUSY_RECOVERY` (261):** "another process is busy recovering a WAL mode database file following a crash." ([sqlite.org/rescode.html (261)](https://sqlite.org/rescode.html))
- **`SQLITE_BUSY_TIMEOUT` (773):** only produced when SQLite is compiled with `SQLITE_ENABLE_SETLK_TIMEOUT`; blocking POSIX advisory locks are "a proprietary SQLite extension." ([sqlite.org/rescode.html (773)](https://sqlite.org/rescode.html))
- **WAL still returns BUSY in documented cases** ([sqlite.org/wal.html §9](https://sqlite.org/wal.html)): (a) another connection holds `locking_mode=EXCLUSIVE`; (b) when the *last* connection is closing it takes an exclusive lock to clean up WAL/SHM — a concurrent open can get BUSY; (c) after a crash, the first new connection holds an exclusive lock during recovery, so a third connection gets BUSY. **Implication: a stdio MCP server that starts and stops frequently (one process per client session) will hit (b) and (c) routinely.** This is the strongest technical argument for the single-daemon pattern.
- **`PRAGMA busy_timeout` semantics:** "Each database connection can only have a single busy handler. This PRAGMA sets the busy handler for the process, possibly overwriting any previously set busy handler." ([sqlite.org/pragma.html#pragma_busy_timeout](https://sqlite.org/pragma.html#pragma_busy_timeout)). **SQLite does not publish a recommended value.** Widely-cited practitioner value: 5000 ms — e.g. Simon Willison's writeup on diagnosing `SQLITE_BUSY` despite a timeout ([simonwillison.net, 2025-02-17](https://simonwillison.net/2025/Feb/17/sqlite-busy/)). **Treat 5000 ms as a community convention, not a documented recommendation.** Note the ceiling: `busy_timeout` cannot help with `SQLITE_BUSY_SNAPSHOT`, and a very long timeout converts a fast failure into a long stall, which is bad for an interactive MCP tool call.
- **`PRAGMA journal_size_limit`** — exact semantics: "in WAL mode, the write-ahead log file is not truncated following a checkpoint. Instead, SQLite reuses the existing file... Each time a transaction is committed or a WAL file resets, SQLite compares the size of the rollback journal file or WAL file left in the file-system to the size limit set by this pragma and if the journal or WAL file is larger it is truncated to the limit... **The default journal size limit is -1 (no limit).** ... To always truncate rollback journals and WAL files to their minimum size, set the journal_size_limit to zero." ([sqlite.org/pragma.html#pragma_journal_size_limit](https://sqlite.org/pragma.html#pragma_journal_size_limit)). **Concrete recommendation:** set a non-default limit (e.g. 64 MiB) on a long-lived local DB — the default leaves the WAL at whatever high-water mark it reached, and §1.2 says read perf degrades with WAL size.

## 1.4 How many concurrent readers can WAL actually support? — **published benchmark gap**

**I could not find any authoritative, primary-source benchmark from SQLite, Fly.io, Cloudflare, Litestream, Turso or rqlite that states a concrete concurrent-reader count or read-QPS figure for SQLite WAL.** I searched specifically for this. What *is* published:

- **Fly.io's SQLite-internals article** explains the WAL mechanism in depth (SHM index layout: 32 KB blocks, 4096 page numbers + 8192 hash slots per block, hash function `(pgno * 383) % 8192`) but contains **no QPS or reader-count benchmarks**. It only says "the vast majority of applications will benefit from WAL mode." ([fly.io/blog/sqlite-internals-wal](https://fly.io/blog/sqlite-internals-wal/), author Ben Johnson; page retrieved 2026-09-19, no last-updated date exposed).
- **Cloudflare D1 publishes an explicit throughput rule of thumb** for its single-threaded, Durable-Object-backed SQLite: "Each individual D1 database is inherently single-threaded, and processes queries one at a time. Your maximum throughput is directly related to the duration of your queries. **If your average query takes 1 ms, you can run approximately 1,000 queries per second. If your average query takes 100 ms, you can run 10 queries per second.** A database that receives too many concurrent requests will first attempt to queue them. If the queue becomes full, the database will return an 'overloaded' error." ([developers.cloudflare.com/d1/platform/limits](https://developers.cloudflare.com/d1/platform/limits/), **last updated Apr 21, 2026**). D1 caps: 10 GB per DB (Workers Paid), 2 MB max row/BLOB, 30 s max query duration, 6 simultaneous connections per Worker invocation, Time Travel PITR 30 days (Paid) / 7 days (Free).
- **rqlite:** "Depending on your machine (particularly its disk IO performance) and network, **write throughput could be anything from 10 requests per second to hundreds of requests per second.**" rqlite's writes are gated by Raft `fsync()` per log entry; **writes are blocked during snapshot creation**, and also blocked during `VACUUM`. ([rqlite.io/docs/guides/performance](https://rqlite.io/docs/guides/performance/), retrieved 2026-09-19, no last-updated date exposed).
- **Community measurement of the single-writer ceiling (unverified):** a SQLite User Forum post claims "SQLite WAL mode caps independent-transaction throughput at **~80K commits/s on consumer NVMe**, while the same drive sustains 500K+ random-write IOPS... Baseline ~80K/s, batch upper bound ~500K/s, fsync latency ~4ms: all measured." The post proposes a "Multi-Segment Epoch WAL" with 16 segment files. **This is an unrefereed forum proposal by a community member, not SQLite-project output, and the end-to-end patch is explicitly "projected," not measured.** ([SQLite User Forum: Multi-Segment Epoch WAL](https://www.sqlite.org/forum/forumpost/97ef578678fc8c78), retrieved 2026-09-19). Treat as directional only.

**Practical reading for Vestige:** the binding constraint is *write* serialization, not reader count. For a personal memory store (writes on ingest/dream/consolidation, reads on search) at human-interaction rates, single-digit thousands of writes/day is nowhere near the ceiling. The realistic risks are (a) the checkpoint-starvation scenario if a long-running read transaction is held open, and (b) WAL growth from a long consolidation loop that writes continuously.

## 1.5 Multi-process access: two MCP clients on the same SQLite file

**What SQLite documents** ([wal.html](https://sqlite.org/wal.html), [howtocorrupt.html](https://sqlite.org/howtocorrupt.html), [pragma.html](https://sqlite.org/pragma.html), all retrieved 2026-09-19):

- Multi-process WAL access on one host is **supported and intended** — "Readers can exist in separate processes" (§2.2). It works because the wal-index is an mmapped `-shm` file "in the same directory as the database itself" (§7).
- **What breaks:**
  1. **Transient `SQLITE_BUSY`** on connect/disconnect from the exclusive lock during WAL/SHM cleanup, and during post-crash recovery (§9).
  2. **`SQLITE_BUSY_SNAPSHOT`** on read→write upgrade races ([rescode.html (517)](https://sqlite.org/rescode.html)).
  3. **The WAL-reset corruption bug** if running SQLite < 3.51.3 / < 3.53.0 (§1.1) — *this is the "what breaks" that actually loses data.*
  4. **Checkpoint starvation** if any one process holds a long read transaction (§6).
  5. **Autocheckpoint thrash**: with `N` processes each doing PASSIVE autocheckpoints at 1000 pages, checkpoint attempts interleave; harmless but wasteful.
  6. **Any network filesystem = corruption.** This includes the very common "put my memory DB in iCloud Drive / Dropbox / OneDrive / on a NAS" user behaviour. WAL "does not work over a network filesystem" (§1) and SQLite lists "Filesystems with broken or missing lock implementations... especially... network filesystems and NFS in particular" as a corruption cause ([howtocorrupt.html §2.1](https://sqlite.org/howtocorrupt.html)).
- **What published MCP memory servers do about it:**
  - **`@modelcontextprotocol/server-memory` (official):** does **not** use SQLite at all. It persists a knowledge graph to a **JSONL file** via `MEMORY_FILE_PATH` (default `memory.jsonl` in the server directory) ([README](https://github.com/modelcontextprotocol/servers/blob/main/src/memory/README.md), retrieved 2026-09-19). It sidesteps the concurrency question entirely — and equally has no locking story. **No multi-client guidance is documented.**
  - **`mcp-memory-service`:** explicitly runs **one local `sqlite-vec` database as a shared backend for four different agents over stdio** ("Claude Code, Claude Desktop, Codex CLI and OpenCode all talk to the same sqlite-vec DB over stdio on my Mac, ~5,900 memories and counting"), plus **two Docker containers sharing one SQLite backend** (MCP + web UI) ([README](https://github.com/doobidoo/mcp-memory-service/blob/main/README.md), retrieved 2026-09-19; versions v11.8.4 dated August 25, 2026). **So multi-process sharing of one SQLite file is the de-facto pattern in the wild** — but note their README also documents real production incidents at scale (tag-filtered retrieval 4xx above 16 results from a `topK` ceiling, #1259; hybrid pull sync treating equal counts as "in sync" leaving divergent stores, #1255; soft-deleted rows resurfacing, #1262). **No daemon is used.**
  - **`basic-memory`:** "Just files plus a local SQLite index. No servers required"; supports SQLite (default) or Postgres; embeddings on a background thread "never blocks the CLI" ([README](https://github.com/basicmachines-co/basic-memory/blob/main/README.md), retrieved 2026-09-19). Note: the project now leads with a **$15/mo cloud workspace**, with local Markdown sync as the free tier.
  - **`mem0`, `Letta/MemGPT`, `Zep/Graphiti`:** all are **client-server / hosted-API architectures**, not local-file multi-process. Mem0's published benchmarks are explicitly "Mem0's managed platform, which includes proprietary optimizations not available in the open-source SDK" ([mem0 README, April 2026 section](https://github.com/mem0ai/mem0/blob/main/README.md)). Zep requires a running Graphiti service + Neo4j/FalkorDB. Letta is a server.
- **Is a single-writer daemon recommended?** **No source I found recommends it explicitly as a named pattern.** But the evidence converges on it: SQLite's own §9 documents BUSY on connect/disconnect and post-crash recovery; the 3.7.0–3.51.2 WAL-reset bug only fires with ≥2 connections; the checkpoint-starvation failure mode disappears with one writer; and the MCP security doc recommends local servers "Use the `stdio` transport to limit access to just the MCP client" or "Restrict access if using an HTTP transport, such as: Require an authorization token; Use unix domain sockets or other Interprocess Communication (IPC) mechanisms with restricted access" ([MCP Security Best Practices, Local MCP Server Compromise → Mitigation](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md), protocol version 2026-07-28). **A single daemon holding the only write connection, with stdio MCP servers as thin proxies over a Unix domain socket, is the defensible design.** Note the trade-off: the MCP spec for 2026-07-28 is explicitly **stateless** ("MCP is stateless and has no protocol-level sessions", [security_best_practices.md](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md)), so a daemon must key state itself.

## 1.6 FTS5 production notes

Source unless noted: [sqlite.org/fts5.html](https://sqlite.org/fts5.html), retrieved 2026-09-19.

- **`optimize`:** "merges all individual b-trees that currently make up the full-text index into a single large b-tree structure. This ensures that the full-text index consumes the minimum space within the database and is in the fastest form to query." ⚠️ "**Because it reorganizes the entire FTS index, the optimize command can take a long time to run.**" (§6.9)
- **`merge` (the incremental alternative):** `INSERT INTO ft(ft, rank) VALUES('merge', 500);` merges b-trees "until roughly N pages of merged data have been written." You can detect a no-op via `sqlite3_total_changes()`: "If the difference between the two values is 2 or greater, then work was performed." Recommended pattern: **"only the first call to 'merge' should specify a negative parameter. Each subsequent call to 'merge' should specify a positive value."** (§6.8) **This is the right primitive for a background consolidation loop** — it lets you amortize index maintenance across many small steps instead of one long blocking `optimize`.
- **Merge thresholds:** `automerge` default **4**, maximum 16, `0` disables; `crisismerge` default **16**, no maximum; `usermerge` default **4**, minimum 2. (§6.1, §6.2, §6.14)
- **`pgsz`:** "The default value is **4050**" bytes per blob. (§6.10)
- **`detail` option:** "reduces the space that the index consumes within the database file, but also reduces the capability and efficiency of the system." **SQLite quantifies neither the space saving nor the capability loss** — flagged as a documentation gap. (§4.6)
- **External-content tables:** `CREATE VIRTUAL TABLE ft USING fts5(t, content='t1', content_rowid='a')`. **Critical operational pitfall:** "**It is the responsibility of the user to ensure that an FTS5 external content table ... is kept consistent with the content table itself.**" Three concrete failure modes documented (§4.4.4):
  1. `SELECT * FROM ft` (no MATCH) passes through to the content table and returns rows; `SELECT rowid, t FROM ft('gold')` uses the index and returns different rows — **silent inconsistency**.
  2. Creating the triggers after populating the content table leaves existing rows unindexed: "creating the triggers does not copy existing rows from the content table into the FTS index."
  3. Recovery: `INSERT INTO ft(ft) VALUES('rebuild');` "may be used to completely discard the contents of the FTS index and rebuild it based on the current contents of the content table."
  Standard trigger pattern is documented (§4.4.3) with the important note that "Like contentless tables, external content tables do not support REPLACE conflict handling. Any operations that specify REPLACE conflict handling are handled using ABORT."
- **Contentless vs external content:** "Unless backwards compatibility is required, new code should prefer **contentless-delete** tables to contentless tables." (§4.4.2)
- **Index size overhead — the only quantitative data point I found is community-measured, not official.** A SQLite User Forum thread reports on a 2.8 GB content DB: regular FTS5 **9.9 GB**, contentless **5.4 GB**, external-content **5.4 GB** (standard and trigram tokenizers) — i.e. **~1.9× the content size for external-content FTS5, ~3.5× for content-storing FTS5** ([SQLite User Forum: Differences between FTS5 'external content' and 'contentless' vTabs, 2024-05-20](https://sqlite.org/forum/forumpost/53e724d12dbd2097?t=c)). **Flagged: single unreplicated community measurement on one dataset; SQLite publishes no official figure.**
  - Same thread's practical note (from Roger Binns): "The problem with contentless is that any auxiliary function that wants the content of a column will get null. For example the `snippet` and `highlight` functions get the original content... In order to delete from an FTS5 table you need to have the original content available — either via the delete command or a trigger." **Vestige uses `snippet`-style output? If so, contentless is not an option.**
- **`integrity-check`:** `INSERT INTO ft(ft) VALUES('integrity-check');` verifies the index; for external content tables, index-vs-content comparison requires `VALUES('integrity-check', 1)`. Fails with `SQLITE_CORRUPT_VTAB`. (§6.7) **This is a cheap startup self-check worth running after a crash or a Time Machine restore.**
- **Tokenizer choices documented:** `unicode61` (default), `ascii`, `porter` (English stemming), `trigram` (substring/LIKE-style matching). (§4.3) The forum thread above notes a user hit **empty results from trigram + contentless/external-content** ([2024-05-20](https://sqlite.org/forum/forumpost/53e724d12dbd2097?t=c)) — **unresolved in that thread; test before relying on trigram with a contentless table.**

## 1.7 Vector search in SQLite — **the landscape changed materially**

### sqlite-vec (the incumbent choice in Vestige's plan)
- **Pre-v1. The README carries a hard warning: "_`sqlite-vec` is a pre-v1, so expect breaking changes!_**" ([README](https://github.com/asg017/sqlite-vec), retrieved 2026-09-19).
- **Current versions (crates.io, 2026-09-19):** max/newest `0.1.10-alpha.4` (published 2026-05-18), **max *stable* `0.1.9`** (published 2026-03-31). ([crates.io API](https://crates.io/api/v1/crates/sqlite-vec))
- v0.1.9 release note: "Bug fix for DELETE operations — Fixes #274, which discovered that DELETE operations on `vec0` tables with metadata text columns that are long (>12 chars) would erroneously report a `SQLITE_DONE` error." ([GitHub release v0.1.9, 31 Mar 2026](https://github.com/asg017/sqlite-vec/releases/tag/v0.1.9)). **Note the date ordering: the stable release fixes a DELETE bug found after the alpha line had moved on.**
- Licence: **MIT/Apache-2.0** ([crates.io](https://crates.io/api/v1/crates/sqlite-vec)). Pure C, no dependencies, `vec0` virtual tables, supports float/int8/binary vectors and metadata/auxiliary/partition-key columns ([README](https://github.com/asg017/sqlite-vec)).
- Funding: Mozilla Builders project, with Fly.io, Turso, SQLite Cloud, Shinkai sponsorship. **Sponsorship is not a stability guarantee** — the pre-v1 warning stands.

### sqlite-vss — **DEPRECATED**
- Verbatim deprecation notice at the top of the README: "**`sqlite-vss` is not in active development.** Instead, my effort is now going towards `sqlite-vec`, which is a similar vector search SQLite extension, but should be much easier to install and use than `sqlite-vss`." ([sqlite-vss README](https://github.com/asg017/sqlite-vss), retrieved 2026-09-19). sqlite-vss was Faiss-based; it is architecturally the closest thing to Vestige's current USearch sidecar approach.

### ⚠️ **NEW: SQLite now ships an official vector extension — "Vec1"**
This is a significant 2026 development that changes the build-vs-buy calculus.
- **Vec1 is an official SQLite extension** hosted at `sqlite.org/vec1`, "implemented in portable C and has no external dependencies. It uses AVX2 on x86 and NEON on ARM. **Vec1 uses IVFADC (Inverted File with Asymmetric Distance Computation) with OPQ (Optimized Product Quantization).**" Supports L2 and cosine. "**The current release is version 0.7.**" ([sqlite.org/vec1](https://sqlite.org/vec1/), page generated 2026-08-28)
- **Explicitly not 1.0:** roadmap says "**No further features are required before a 1.0 release. But: Testing is insufficient.** Other things to be added and/or investigated following 1.0 release: Almost all paths require optimization... Support for partition keys. **Add an option for a modern graph-based index as an alternative to IVFADC. HNSW? DiskANN? Some variant?**" ([sqlite.org/vec1](https://sqlite.org/vec1/), generated 2026-08-28)
- **Status as of 2026-03-30 (from the SQLite developer, Dan Kennedy):** "There is still no vec1 release, but we are getting closer... **The main things to address before a first release are that: Some paths that use SIMD on x86 are not yet using SIMD on ARM (or WASM), and testing is woefully inadequate.** And of course almost all code paths need optimization." ([SQLite User Forum: Vec1 update, 2026-03-30 20:36:36](https://www.sqlite.org/forum/forumpost/c9d69d74c6644dd1?t=c))
- The same thread (through 2026-04-15) documents **36 MSVC compiler warnings vs 1 for clang v21**, a leftover `printf` behind `#if 1`, and WASM build problems. Signals genuine immaturity.
- **No HNSW.** Vec1 is IVFADC/OPQ; the roadmap floats HNSW/DiskANN as post-1.0. So Vec1 is **not** a drop-in replacement for Vestige's HNSW semantics, and it **requires a training step** with retraining heuristics (`coarse_error`, `reconstruction_error`, bucket distribution) that the forum thread discusses as an open UX problem.

### HNSW sidecar indexes
- **USearch** (what Vestige uses): `usearch` crate **2.26.2**, published 2026-08-31, **Apache-2.0** ([crates.io](https://crates.io/api/v1/crates/usearch)). README self-describes "10x faster HNSW implementation than FAISS"; supports "View large indexes from disk without loading into RAM" and serialization "Into a stream ... serializing or reconstructing incrementally", `index.save("index.usearch")` ([usearch README](https://github.com/unum-cloud/usearch), retrieved 2026-09-19). ⚠️ The "10x faster" claim links to a **vendor blog post** ([unum.cloud, 2023-11-07](https://www.unum.cloud/blog/2023-11-07-scaling-vector-search-with-intel)) — vendor-published, not independently replicated.
- **`hnsw_rs` 0.3.4** (2026-02-28, MIT/Apache-2.0) and **`instant-distance` 0.6.1** (2023-06-26, last updated 3 years ago — **effectively stale**) are the pure-Rust alternatives ([crates.io](https://crates.io/api/v1/crates/hnsw_rs), [crates.io](https://crates.io/api/v1/crates/instant-distance)).

### Durability/consistency of an in-memory ANN index vs SQLite — **published guidance exists, and it answers the rebuild question**

**Primary source: "How Should We Evaluate Data Deletion in Graph-Based ANN Indexes?", Tomohiro Yamashita, Daichi Amagata, Yusuke Matsui — [arXiv:2512.06200](https://arxiv.org/abs/2512.06200), submitted 5 Dec 2025, accepted at NeurIPS 2025 Workshop on Machine Learning for Systems (4 pages, 4 figures).** This is the only peer-reviewed-ish work I found that directly formalizes the trade-off. Its findings:

| Deletion method | Search accuracy (1-Recall@10) | QPS-delete | Memory efficiency |
|---|---|---|---|
| Reconstruction (full rebuild) | **Highest** | Lowest | Medium |
| Eager deletion (physical node removal) | Medium | Medium | **Best** |
| Lazy deletion (tombstone/mark-deleted) | **Lowest** | **Highest** | Worst |

Quantitative findings from the paper (its Table/ablation, SIFT1M/SIFT1B, b=10⁵, α=0.84):
- **Lazy-deletion accuracy degrades approximately *linearly* with the number of deletion rounds**, and "can be accurately predicted using Δ = (R_S − R_0)/S."
- **Eager-deletion accuracy degradation is *not* unbounded — it converges to a stable floor θ** after repeated insert/delete rounds. This contradicts the intuition that eager deletion monotonically destroys graph connectivity.
- The proposed "Deletion Control" algorithm alternates lazy deletion with periodic reconstruction; it "requires only **10% of queries as a calibration set** to accurately estimate θ and π," and achieves "**Minimum** [total deletion time] among all strategies satisfying the accuracy constraint."
- At high-frequency small-batch deletion (b=10³), "the relative overhead of reconstruction is larger, making eager deletion more advantageous."
- **Stated limitations (important for honesty):** "As a Workshop paper, the experimental scope is relatively limited — validation is conducted primarily on HNSW without testing other graph-based indexes such as DiskANN and NSG"; "**The performance impact of concurrent deletion and querying is not discussed**"; only the three basic approaches compared (not FreshDiskANN edge-rewiring or IPGM neighbor recomputation).

**Answer to "rebuild-on-startup vs incremental persistence":** the literature says **incremental tombstone deletion is functionally correct but linearly degrades recall, so a periodic rebuild is the practical answer.** Vestige's existing "persist to `vestige.hnsw` + meta JSON, load on startup via row-count-validated fast path, rebuild from SQLite on mismatch" is exactly the reconstruction fallback — but note the paper's finding that **row-count validation is a weak staleness signal**: a same-count delete+insert is invisible to it. A content hash or a monotonic change counter (bumped on every node/embedding mutation) would be strictly better. **I found no published guidance quantifying "how stale is too stale" for a fast path — that threshold is a judgement call, not a documented number.**

**Reconciliation hazard specific to a sidecar index:** SQLite's WAL gives you crash-atomicity for the *table* (with `synchronous=NORMAL`, always consistent — §1.2); the sidecar file gets none of that. There is no WAL for `vestige.hnsw`. A crash mid-write leaves a truncated index that must be detected and discarded. **SQLite's own documentation gives no guidance on this because it is out of scope for SQLite** — flagged as a design area with no primary-source support.

---

# 2. Backup, restore, durability

## 2.1 Official SQLite backup guidance

Source: [sqlite.org/backup.html](https://sqlite.org/backup.html) and [sqlite.org/howtocorrupt.html §1.2](https://sqlite.org/howtocorrupt.html), both retrieved 2026-09-19.

**SQLite enumerates the safe methods explicitly** (howtocorrupt.html §1.2), "safe in the sense that they generate a correct, uncorrupted backup. In no particular order":
1. **`sqlite3_rsync`** — "available beginning with SQLite 3.47.0 (2024-10-21) and later) will make a copy of a live SQLite over SSH using a bandwidth-efficient protocol."
2. **`VACUUM INTO filename`** — "copies out the current state of an SQLite database into a separate file."
3. **The backup API** — "a C-language interface that can make a consistent copy of an SQLite database."

And the caveat: "It is also safe to make a copy of an SQLite database file **as long as there are no transactions in progress** while the copy is taking place. If the previous write transaction failed, then it is important that any rollback journal (the `*-journal` file) or write-ahead log (the `*-wal` file) be copied together with the database file itself."

**Why copying a live DB file is unsafe:** "Systems that run automatic backups in the background might try to make a backup copy of an SQLite database file while it is in the middle of a transaction. **The backup copy then might contain some old and some new content, and thus be corrupt.**" (howtocorrupt.html §1.2). Also listed under "How to corrupt" (§1.3): "**Copying a database file without also copying its journal.**"

**Online Backup API properties** ([backup.html §1](https://sqlite.org/backup.html)):
- "The effect of completing the backup call sequence is to make the destination a **bit-wise identical copy** of the source database as it was when the copying commenced. (The destination becomes a 'snapshot.')"
- "The copy operation may be done incrementally, in which case **the source database does not need to be locked for the duration of the copy**, only for the brief periods of time when it is actually being read from."
- ⚠️ **Documented failure mode:** "Writes to an in-memory source database, or writes to a file-based source database **by an external process or thread using a database connection other than pDb**, are significantly more expensive than writes made to a file-based source database using pDb (as the entire backup operation must be restarted in the former two cases). **If the backup process is restarted frequently enough it may never run to completion and the backupDb() function may never return.**" — **This is directly relevant to Vestige: with multiple MCP clients writing, a long `.backup`/backup-API backup can livelock.** `VACUUM INTO` does not have this problem (it takes a consistent snapshot at statement start).

**`VACUUM INTO` specifics** ([sqlite.org/lang_vacuum.html](https://sqlite.org/lang_vacuum.html), retrieved 2026-09-19):
- "The file named by the INTO clause **must not previously exist**, or else it must be an empty file, or the VACUUM INTO command will fail with an error." **So a rotating-backup scheme must delete/rename first.**
- "The VACUUM INTO command is transactional in the sense that the generated output database is a consistent snapshot of the original database. **However, if the VACUUM INTO command is interrupted by an unplanned shutdown or power loss, then the generated output database might be incomplete and corrupt.** However, if the PRAGMA synchronous setting of the original database is NORMAL or FULL, then SQLite invokes fsync() ... so a power failure ... after the VACUUM INTO command has completed should not corrupt the database."
- "**all deleted content is purged from the backup, leaving behind no forensic traces.**" (vs. the backup API, which "uses fewer CPU cycles and can be executed incrementally.") — **This is the GDPR-relevant property: `VACUUM INTO` produces a backup with no recoverable deleted rows; a file copy of a DB with free pages retains them.**
- VACUUM requires "as much as twice the size of the original database file ... in free disk space" (for `VACUUM`; `VACUUM INTO` uses the target file in place of the temp copy).
- "A VACUUM will fail if there is an open transaction on the database."

## 2.2 Litestream — status after Fly.io

- **Maintained again.** The Litestream site header reads: "**v0.5.x — Latest — Actively maintained with new features and bug fixes** | View v0.3.14 (Previous)". ([litestream.io](https://litestream.io/), retrieved 2026-09-19)
- **History (from a third-party but well-documented account):** "Litestream is owned by Fly.io, and they **paused development on Litestream for almost two years in favor of an alternative project called LiteFS**. Two weeks ago [~2025-09-30], Ben Johnson, Litestream's creator and lead developer, announced that they were shifting focus back to Litestream and had just published a new release, **0.5.0**." ([mtlynch.io, October 14, 2025, updated October 17, 2025](https://mtlynch.io/notes/hold-off-on-litestream-0.5.0/))
- **Migration hazards that author hit (0.5.0 → fixed by 0.5.2):** the backup format changed so **"Litestream 0.5.0 cannot restore from backups created in previous versions"**; `replicas:` (array) → `replica:` (dict) in `litestream.yml`; Backblaze S3 custom-endpoint URI failure (issue #789); `-if-replica-exists` flag accidentally removed. "**Update 2 (2025-10-17): Litestream 0.5.2 wraps up all the bugs I ran into**, so I plan to roll it out." Same source notes 0.5.8 is current in later packaging ([bcasci/litestream-ruby issue](https://github.com/bcasci/litestream-ruby/issues/1)).
- **RPO/RTO — what Litestream actually documents** ([litestream.io/how-it-works](https://litestream.io/how-it-works/), retrieved 2026-09-19):
  - Mechanism: "Litestream works by effectively **taking over the checkpointing process**. It starts a long-running read transaction to prevent any other process from checkpointing and restarting the WAL file."
  - **⚠️ This directly conflicts with the multi-process model in §1.5**: Litestream holds a long-running read transaction by design. Combined with SQLite's §6 warning that a long-running read transaction blocks checkpoint progress, running Litestream alongside other writers on the same file is an architectural tension. **I found no Litestream documentation addressing multi-process co-existence** — flagged as unverified/unaddressed.
  - Exactly-once-ish replication with **TXIDs**: "Litestream assigns each of these files the next monotonically incrementing transaction ID (TXID) and stores checksums alongside the pages to ensure consistency." On restore, "Because TXIDs form a contiguous sequence, Litestream can verify that no transactions are missing before restoring."
  - **L0→L3 compaction intervals: 30 seconds, 5 minutes, 1 hour; snapshot level every 24 hours (defaults).** `max-sync-wal-bytes` default **64 MiB**.
  - **Documented RPO shape:** L0 accumulates "uncompacted per-sync batches written continuously during replication" — so the practical RPO is the sync interval (sub-minute by design), and PITR granularity is bounded by L0 file density.
  - A `-txid` restore target is **inclusive**: "a file is eligible when its maximum TXID is less than or equal to [the target]."
  - ⚠️ **I could not find a published numeric RPO/RTO guarantee (e.g. "RPO ≤ 1 s") on litestream.io.** Flagged: litestream publishes mechanism, not an SLA. Any RPO claim should be measured locally.

## 2.3 Alternatives and concrete numbers

| Tool | Status / version | Concrete published numbers | Source + date |
|---|---|---|---|
| **`sqlite3 .backup` in cron** | Built into the CLI; the shell wraps the online backup API | "bit-wise identical copy"; **can livelock under continuous external writes** (see §2.1) | [backup.html](https://sqlite.org/backup.html) / [cli.html](https://sqlite.org/cli.html), retrieved 2026-09-19 |
| **`VACUUM INTO`** | Built in | Consistent snapshot; **purges deleted content**; needs 2× disk for full `VACUUM`; **target file must not exist** | [lang_vacuum.html](https://sqlite.org/lang_vacuum.html), retrieved 2026-09-19 |
| **`sqlite3_rsync`** | New in **SQLite 3.47.0 (2024-10-21)** | Bandwidth-efficient copy **over SSH** — i.e. remote only | [howtocorrupt.html §1.2](https://sqlite.org/howtocorrupt.html), retrieved 2026-09-19 |
| **Litestream** | **v0.5.x, actively maintained** | L0/L1/L2/L3 = 30 s / 5 min / 1 h; snapshot 24 h; `max-sync-wal-bytes` 64 MiB | [litestream.io/how-it-works](https://litestream.io/how-it-works/), retrieved 2026-09-19 |
| **LiteFS** | Fly.io project that *replaced* Litestream during the 2-year pause | **I did not fetch/verify LiteFS's current status or numbers in this session** — flagged as unverified | [fly.io/docs/litefs](https://fly.io/docs/litefs/) (link retrieved, content not analysed) |
| **Turso embedded replicas** | Turso docs page fetched | **I did not extract concrete RPO/RTO numbers from the Turso embedded-replicas page** — flagged as unverified | [docs.turso.tech/features/embedded-replicas](https://docs.turso.tech/features/embedded-replicas/introduction) (fetched, not analysed in depth) |
| **rqlite** | Raft-replicated SQLite | **Write throughput "anything from 10 requests per second to hundreds"**; **writes blocked during snapshot creation**; `-auto-vacuum-int` for scheduled VACUUM; auto `PRAGMA optimize` daily | [rqlite.io/docs/guides/performance](https://rqlite.io/docs/guides/performance/), retrieved 2026-09-19 |
| **Cloudflare D1 Time Travel** | Hosted only, not applicable locally | **PITR window: 30 days (Paid) / 7 days (Free); max 10 restores per 10 minutes per database** | [D1 limits, last updated Apr 21, 2026](https://developers.cloudflare.com/d1/platform/limits/) |

**Point-in-time recovery for SQLite / WAL archiving:** SQLite itself provides **no PITR**. There is no WAL-archive-and-replay facility in the library. PITR is only available via (a) Litestream's LTX chain (TXID-addressed, contiguous-sequence-verified), (b) Cloudflare D1 Time Travel (hosted), or (c) your own periodic `VACUUM INTO` snapshots plus an application-level changelog. **For a local-first app, the pragmatic PITR is: periodic `VACUUM INTO` snapshots + Litestream for the continuous tail.** Vestige's existing JSON `export`/`restore` tools are application-level PITR of a sort, but they are lossy on internal state unless the export covers all tables.

## 2.4 Encryption at rest

| Option | Version / licence | What it supports | Rust `rusqlite` usable? |
|---|---|---|---|
| **SQLCipher Community** | **4.18.0 (August 2026)**, baselined on "SQLite 3.53.4"; 4.19.0 in progress as of CHANGELOG ([SQLCipher CHANGELOG](https://raw.githubusercontent.com/sqlcipher/sqlcipher/master/CHANGELOG.md), retrieved 2026-09-19) | Full DB encryption, 256-bit AES; **Community = "Core functions only"**; Commercial adds "value-level encryption, encrypted virtual tables, performance statistics" and "Optimized implementation delivers **up to 4x faster** encryption operations compared to Community edition" | ✅ **`bundled-sqlcipher`** and **`bundled-sqlcipher-vendored-openssl`** features exist in rusqlite |
| **SQLCipher licence** | **BSD-style (3-clause).** Verbatim: "Redistribution and use in source and binary forms... **Neither the name of the ZETETIC LLC nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.**" Copyright (c) 2025, ZETETIC LLC | — | — |
| **⚠️ SQLCipher attribution requirement** | Zetetic's licence page: "The Community Edition is available under a BSD-style license and **requires attribution and reproduction of license grants in the application interface and/or materials. This can be in an about or licensing screen in the application, in the product documentation on a website linked from the application, but must be a user accessible location.**" ([zetetic.net/sqlcipher/license](https://www.zetetic.net/sqlcipher/license/), retrieved 2026-09-19) — **This is stricter than stock BSD: it mandates user-visible attribution.** |
| **SQLite SEE** | "AES-256 in OFB mode (recommended for all new development)"; also AES-128 OFB, AES-128 CCM, AES-256 GCM, RC4 (legacy) ([sqlite.org/see](https://sqlite.org/see/doc/trunk/www/readme.wiki), retrieved 2026-09-19) | **Licensed, not open source.** "Your license is perpetual. You have paid a one-time fee... **You can ship as many copies of the software to your customers as you want so long as you ensure that only compiled binaries are shipped (you cannot distribute source code)** and that your customers cannot make additional copies of the software to use for other purposes. You can create multiple products that use this software as long as all products are developed and maintained by the same team. For the purposes of this paragraph, a 'team' is a work unit where **everybody knows each others names**." | ❌ **No rusqlite feature exists.** Would require swapping the amalgamation manually. |
| **SEE price** | **Not verified.** `https://sqlite.org/see/purchase.wiki` returned **HTTP 404** in my fetch; the `/see` index page rendered no pricing in the extracted text. **Flagged: SEE pricing could not be verified in this session.** | — | — |
| **sqlite3mc / SQLite3MultipleCiphers (wxSQLite3)** | **Not verified.** `raw.githubusercontent.com/utelle/SQLite3MultipleCiphers/master/README.md` returned **HTTP 404** (branch name may be `main`, or the path changed). **Flagged as unverified in this session.** | — | No first-class rusqlite feature |
| **OS-level: FileVault / LUKS / eCryptfs / fscrypt** | See §2.5 for FileVault | Transparent, no code change, no key management in the app | ✅ trivially compatible |

**`rusqlite` feature flags, verified verbatim from [rusqlite Cargo.toml](https://raw.githubusercontent.com/rusqlite/rusqlite/master/Cargo.toml)** (v0.40.1 in-tree; crates.io latest 0.40.2, 2026-08-08):
```toml
bundled = ["libsqlite3-sys?/bundled", "modern_sqlite"]
bundled-sqlcipher = ["libsqlite3-sys?/bundled-sqlcipher", "bundled"]
bundled-sqlcipher-vendored-openssl = [
    "libsqlite3-sys?/bundled-sqlcipher-vendored-openssl",
    "bundled-sqlcipher",
]
sqlcipher = ["libsqlite3-sys?/sqlcipher"]
```
README clarifications ([rusqlite README](https://github.com/rusqlite/rusqlite), retrieved 2026-09-19): "`sqlcipher` looks for the SQLCipher library to link against instead of SQLite. **This feature overrides `bundled`.**"; "`bundled-sqlcipher` uses a bundled version of SQLCipher. This **searches for and links against a system-installed crypto library** to provide the crypto implementation."; "`bundled-sqlcipher-vendored-openssl` ... uses the `openssl-sys` crate, with the `vendored` feature enabled in order to build and bundle the OpenSSL crypto library."

**Practical recommendation:** `bundled-sqlcipher-vendored-openssl` is the only fully self-contained option and works on macOS without a Homebrew OpenSSL. Costs: (a) you must surface Zetetic attribution in a user-visible location; (b) you now own key management (see §3.4); (c) you lose the ability to open the DB with the `sqlite3` CLI without the key, which complicates support/debugging; (d) **litestream + SQLCipher compatibility is not documented by either project — flagged as unverified.**

## 2.5 macOS specifics

- **FileVault:** Apple: "**If you have a Mac with Apple silicon or an Apple T2 Security Chip, your data is encrypted automatically.** Turning on FileVault provides an extra layer of security by keeping someone from decrypting or getting access to your data without entering your login password. **If you use a Mac that doesn't have Apple silicon or the T2 chip, you need to turn on FileVault to encrypt your data.**" ([Apple Support: Protect data on your Mac with FileVault](https://support.apple.com/guide/mac-help/protect-data-on-your-mac-with-filevault-mh11785/mac), retrieved 2026-09-19)
  - **Practical consequence:** on every Apple-silicon Mac, the volume is **already encrypted at rest** even with FileVault off (the key is just derived from hardware, so an attacker with the physical disk can still be defeated but a logged-in/at-boot attacker cannot). FileVault adds protection against offline decryption without the login password.
  - **There is no Apple-documented CLI to *check* FileVault status in the sources I fetched.** The standard command is `fdesetup status` — **I did not verify this against an official Apple doc page in this session; flagged as unverified-doc.**
- **Time Machine and WAL-mode SQLite — ⚠️ NO OFFICIAL DOCUMENTATION FOUND. This is a genuine gap.**
  - Apple's Time Machine documentation covers setup, hourly backups, encryption of the backup, and restore, but **says nothing about open database files, WAL files, or SQLite** ([Apple Support: Back up your Mac with Time Machine](https://support.apple.com/en-us/104984), retrieved 2026-09-19).
  - The strongest available evidence is a **2018 sqlite-users mailing list thread** titled *"Mac: Users receive 'database disk image is malformed' errors after restoring database files"*. In it, **D. Richard Hipp (SQLite's creator)** replied: "Nothing about SQLite has changed that should make a difference here. Do you know if the corruption is occurring when TimeMachine makes its backup, or is occurring when the backed up database is restored?" The reported theory: *"TimeMachine somehow captures the sqlite DB files a few milliseconds apart 'sometimes' so that a journal file that has just been committed is in the TimeMachine backup captured still in its uncommitted state while the DB itself is in the committed state already (or perhaps vice-versa)."* ([marc.info sqlite-users, 2018-12-12](https://marc.info/?l=sqlite-users&m=154462708903146&w=2))
  - Consistent with this, a SQLite forum thread on FTS5 tables notes a user reporting that after restore "the sqlite file exists but sqlite-wal does not — it won't open without that file" ([Apple Developer Forums result surfaced via search](https://developer.apple.com/forums/), and [SQLite User Forum](https://www.sqlite.org/forum/forumpost/53e724d12dbd2097?raw)) — **I did not open the Apple forum thread directly; flagged as second-hand.**
  - **Mechanism-level reading (my analysis, not a sourced claim):** Time Machine's filesystem-level snapshot should be atomic per-volume, but the *restore* path (and any network/external-disk backup with per-file copy) is not. Excluding the `-wal` and `-shm` files from the backup, or better, using `VACUUM INTO` to produce a single self-contained snapshot file *before* Time Machine runs, removes the hazard entirely.
  - **Actionable, low-cost mitigation:** have Vestige write its periodic backup as a `VACUUM INTO` file (single file, no `-wal`, no `-shm`, and — per §2.1 — no recoverable deleted content) into a directory that Time Machine backs up, and mark the live `-wal`/`-shm` as excluded. `NSURLIsExcludedFromBackupKey` / `tmutil addexclusion` are the macOS mechanisms — **I did not verify these against Apple docs in this session; flagged.**

---

# 3. Data residency & privacy for local-first memory

## 3.1 GDPR articles that bite

All article texts quoted from [gdpr-info.eu](https://gdpr-info.eu/) (mirror of Regulation (EU) 2016/679; the official OJ text is at [eur-lex.europa.eu/eli/reg/2016/679/oj](https://eur-lex.europa.eu/eli/reg/2016/679/oj)), retrieved 2026-09-19.

**Article 2(2)(c) — the household exemption (material scope):** "This Regulation does not apply to the processing of personal data: ... **(c) by a natural person in the course of a purely personal or household activity**." ([Art. 2](https://gdpr-info.eu/art-2-gdpr/))

**Recital 18 — and the sentence that matters commercially:** "This Regulation does not apply to the processing of personal data by a natural person in the course of a purely personal or household activity and thus with no connection to a professional or commercial activity. ... **However, this Regulation applies to controllers or processors which provide the means for processing personal data for such personal or household activities.**" ([Recital 18](https://gdpr-info.eu/recitals/no-18/), retrieved 2026-09-19)

> **This is the decisive point for a local-first tool vendor.** The *end user* storing their own memories on their own laptop is likely outside GDPR scope via Art. 2(2)(c). **The vendor shipping the software is not** — Recital 18's third sentence explicitly brings "controllers or processors which provide the means for processing personal data for such personal or household activities" into scope. A local-first architecture removes the *data residency* problem and the *processor* problem, but it does **not** remove the vendor's obligations around Art. 25 (data protection by design and by default), Art. 32 (security of processing), and transparency.

**Article 3 — territorial scope:** applies to processing "in the context of the activities of an establishment of a controller or a processor in the Union, regardless of whether the processing takes place in the Union or not," and to non-EU controllers offering goods/services to, or monitoring the behaviour of, data subjects in the Union. ([Art. 3](https://gdpr-info.eu/art-3-gdpr/)) **Note Art. 3(1)'s "regardless of whether the processing takes place in the Union or not"** — an EU-established vendor cannot escape by making the processing local to the user's machine.

**CJEU *Ryneš*, C-212/13 (11 December 2014)** — the household exemption is construed **narrowly**. Operative ruling: "the operation of a camera system, as a result of which a video recording of people is stored on a continuous recording device such as a hard disk drive, installed by an individual on his family home for the purposes of protecting the property, health and life of the home owners, **but which also monitors a public space, does not amount to the processing of data in the course of a purely personal or household activity**." ([EUR-Lex CELEX:62013CJ0212](https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:62013CJ0212), judgment date 11 December 2014)

> **Reading:** the exemption protects purely private activity that does not extend into other people's space. A memory store containing **only the user's own** notes is at the strong end of the exemption. A memory store that ingests **colleagues' names, client details, meeting notes about identifiable third parties** (exactly what Vestige's `person` node type and entity extraction encourage) is at the weak end. **Vestige's entity-extraction pipeline auto-tags proper nouns (`entity:john-smith`) — i.e. it deliberately builds a filing system of identifiable third parties.** That is a design decision with legal consequence, and it is undocumented in the MCP ecosystem (see §3.5). **I am not a lawyer; this is a reading of primary sources, not legal advice.**

**Article 5 (principles)** — the six principles plus accountability: lawfulness/fairness/transparency; **purpose limitation**; **data minimisation** ("adequate, relevant and limited to what is necessary"); accuracy; **storage limitation**; **integrity and confidentiality**; and 5(2) "The controller shall be responsible for, and be able to demonstrate compliance" (accountability). ([Art. 5](https://gdpr-info.eu/art-5-gdpr/))

**Article 17 (right to erasure)** — the grounds are exhaustive: (a) no longer necessary; (b) consent withdrawn; (c) objection under Art. 21(1)/(2); (d) unlawful processing; (e) legal obligation; (f) Art. 8(1) child data. 17(3) exceptions: freedom of expression; legal obligation/public interest; public health; **archiving/scientific/historical/statistical purposes under Art. 89(1) "in so far as the right referred to in paragraph 1 is likely to render impossible or seriously impair the achievement of the objectives"**; legal claims. ([Art. 17](https://gdpr-info.eu/art-17-gdpr/))

> **Note 17(3)'s Art. 89(1) research exception is the only plausible hook for retaining data in an ML model or index** — and it is narrow, requires Art. 89(1) safeguards, and does not obviously cover a personal-productivity memory store. **Do not rely on it.**

**Article 32 (security of processing)** — "Taking into account the **state of the art**, the costs of implementation and the nature, scope, context and purposes of processing as well as the risk ... the controller and the processor shall implement appropriate technical and organisational measures ... **including inter alia as appropriate: (a) the pseudonymisation and encryption of personal data; (b) the ability to ensure the ongoing confidentiality, integrity, availability and resilience of processing systems and services; (c) the ability to restore the availability and access to personal data in a timely manner in the event of a physical or technical incident; (d) a process for regularly testing, assessing and evaluating the effectiveness of technical and organisational measures.**" ([Art. 32](https://gdpr-info.eu/art-32-gdpr/))

> **Art. 32(1)(c) is literally a backup-and-restore obligation, and 32(1)(d) is literally an obligation to test the restore.** Vestige already exposes `backup` / `restore` tools; the compliance gap is that nothing *verifies* a restore works, and `automationTriggers.needsBackup` is advisory (agent-triggered), not enforced. That is a defensible finding to report: **the backup trigger is agent-dependent, so a user who never opens an agent that honours `automationTriggers` never gets a backup.**

**Article 25 (data protection by design and by default)** — "the controller shall ... implement appropriate technical and organisational measures, such as pseudonymisation, which are designed to implement data-protection principles, such as data minimisation, in an effective manner"; 25(2) requires that "**by default, only personal data which are necessary for each specific purpose of the processing are processed** ... applies to the amount of personal data collected, the extent of their processing, **the period of their storage** and their accessibility." ([Art. 25](https://gdpr-info.eu/art-25-gdpr/))

> **Art. 25(2)'s "period of their storage" is directly in tension with a memory system whose core value proposition is indefinite retention with FSRS-6 retention decay.** "Active forgetting" and `gc(min_retention)` are the mitigation; whether they are aggressive enough is a product decision that should be documented.

**2025/2026 EU AI Act / EDPB developments — NOT VERIFIED.** I did not fetch EDPB Guidelines 3/2018 on territorial scope, the EDPB's 2025/2026 AI-model opinions, or the AI Act text in this session. **Flagged as a research gap.**

## 3.2 Right to erasure in a vector index — what the research actually says

**Short answer: no source I found says a full HNSW rebuild is legally *required*. The primary-source research says tombstones work but cost recall, linearly.**

- **The one directly-on-point paper:** Yamashita, Amagata, Matsui, *"How Should We Evaluate Data Deletion in Graph-Based ANN Indexes?"*, [arXiv:2512.06200](https://arxiv.org/abs/2512.06200), **submitted 5 Dec 2025**, NeurIPS 2025 Workshop on ML for Systems. It formalizes exactly three options and measures them:
  - **Lazy deletion** (tombstone: "graph structure is not modified; only a deletion marker set F is maintained, and marked nodes are excluded from search results") — **highest delete throughput, worst memory, lowest accuracy, and "accuracy degradation under lazy deletion is approximately linear."**
  - **Eager deletion** (physical removal + neighbor-list update) — best memory, medium accuracy, and **"accuracy degradation under eager deletion is not unbounded ... it converges to a stable value θ."**
  - **Reconstruction** (full rebuild) — highest accuracy, lowest delete throughput.
  - **Its own stated limitation: "The performance impact of concurrent deletion and querying is not discussed."**
- **⚠️ No published work I found states that a tombstoned vector is *recoverable* from an HNSW graph.** The privacy question (can an attacker reconstruct a deleted embedding from residual graph structure?) is **not answered in the literature I retrieved**. Flagged explicitly as an open question. The practical engineering answer — used by every vector DB I know of (ChromaDB, FAISS, Weaviate all noted for "avoiding the severe I/O penalties of graph restructuring") — is tombstone + periodic compaction, but **that is engineering practice, not a compliance determination.**
- **Compliance reading (mine, from primary sources):** Art. 17 requires erasure of *personal data*. If you delete the row and the embedding, and the ANN index only contains a tombstoned node ID with no vector content, you have a strong argument. If the ANN index still holds the **vector** (which it usually does — tombstones typically skip the node during search but keep the vector for graph connectivity), you have **not** erased the derived personal data, and §3.3 says the vector *is* personal data. **Tombstone-with-retained-vector is the risky case. Vector-purge-plus-tombstone is the safe case.** Vestige's `gdpr` storage module and `memory(action="delete")` should be checked for which one it does.

## 3.3 Are embeddings personal data? — yes, per the strongest available evidence

- **Morris, Kuleshov, Shmatikov, Rush, "Text Embeddings Reveal (Almost) As Much As Text", [arXiv:2310.06816](https://arxiv.org/abs/2310.06816), submitted 10 Oct 2023, accepted at EMNLP 2023.** Verbatim: "**a multi-step method that iteratively corrects and re-embeds text is able to recover 92% of 32-token text inputs exactly.** We train our model to decode text embeddings from two state-of-the-art embedding models, and also show that **our model can recover important personal information (full names) from a dataset of clinical notes.**"
  - **Caveat I must flag:** the paper's headline number is for **32-token inputs** and it targets **two unspecified "state-of-the-art embedding models"** (the abstract does not name them; I did not verify whether nomic-embed-text-v1.5 is among them). **Inversion difficulty scales badly with text length** — 92% exact recovery on 32 tokens does not extrapolate to 100-token memories. So the correct claim is: **short memories are near-fully invertible; longer ones are progressively less so, but the paper provides no length-scaling curve I extracted.** Do not overstate.
- **Regulatory position:** **I found no EDPB or DPA statement specifically classifying embeddings as personal data.** Flagged as unverified. The defensible engineering position, absent authority, is to treat embeddings as pseudonymised personal data (Recital 26's "means reasonably likely to be used" test) and apply Art. 32 encryption accordingly.

## 3.4 Key management for a desktop app (macOS)

All from [Apple Developer Documentation: Restricting keychain item accessibility](https://developer.apple.com/documentation/security/restricting-keychain-item-accessibility.md) and [kSecAttrAccessibleWhenUnlocked](https://developer.apple.com/documentation/security/ksecattraccessiblewhenunlocked.md), retrieved 2026-09-19 (Apple docs render as JS pages; I used the `.md` variants; page footer shows "Copyright © 2026 Apple Inc.").

Apple documents four accessibility levels, **"in order of decreasing restrictiveness"**:

| Value | Apple's exact guarantee | Restorable to another device? |
|---|---|---|
| **`kSecAttrAccessibleWhenPasscodeSet`** | "If the user hasn't set a passcode, you can't store an item with this setting. **If the user removes the passcode from a device, any items with this setting are automatically deleted from the keychain.** You can only access items with this setting if the device is unlocked." | (not stated for this level) |
| **`kSecAttrAccessibleWhenUnlocked`** (default) | "The data in the keychain item can be accessed only while the device is unlocked by the user." "**This is the default value for keychain items added without explicitly setting an accessibility constant.**" Apple: "recommended for items that need to be accessible only while the application is in the foreground. **Items with this attribute migrate to a new device when using encrypted backups.**" | **Yes** (encrypted backups) |
| **`kSecAttrAccessibleAfterFirstUnlock`** | "This condition becomes true once the user unlocks the device for the first time after a restart... It remains true until the device restarts again. **Use this level of accessibility when your app needs to access the item while running in the background.**" | (not stated for this level) |
| **`kSecAttrAccessibleAlways`** | "The item is always accessible, regardless of the locked state of the device. **This option isn't recommended.**" | (not stated for this level) |

- **The `ThisDeviceOnly` suffix:** "If the attribute ends with the string `ThisDeviceOnly`, **the item can be restored to the same device that created a backup, but it isn't migrated when restoring another device's backup data.**"
- **Apple's own recommendation:** "**Always use the most restrictive option that makes sense for your app.** For extremely sensitive data that you never want stored in iCloud, you might choose `kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly`."
- **Step-up auth:** you can demand user presence per retrieval via `SecAccessControlCreateWithFlags(..., kSecAccessControlUserPresence, ...)`, or specifically biometrics/passcode.
- **⚠️ Direct implication for Vestige:** a **background consolidation loop** that must open the DB while the screen is locked cannot use `WhenUnlocked`. It needs `AfterFirstUnlock` (or `AfterFirstUnlockThisDeviceOnly` to prevent the key migrating in backups). **`WhenUnlocked` + a 6-hour background loop is a functional bug, not just a security choice.** Conversely, `AfterFirstUnlock` means the DB key is available to any process running as the user while the machine is merely logged in — which materially weakens encryption-at-rest against local malware, and is worth documenting in the threat model.
- **NIST SP 800-57 Part 1 Rev 5 and OWASP desktop-secrets guidance: NOT FETCHED.** Flagged as a gap.

## 3.5 What the MCP security spec says about storing sensitive data — **it largely doesn't**

**Source: [MCP Security Best Practices, protocol version 2026-07-28](https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md), fetched 2026-09-19 (HTTP 200).**

**This is the headline finding for §3.5: the document is overwhelmingly about *authorization* (OAuth proxy flows), not about server-side data storage.** Its stated scope: "This document provides security considerations for the Model Context Protocol (MCP), **complementing the MCP Authorization specification.** ... The primary audience for this document includes **developers implementing MCP authorization flows**, MCP server operators, and security professionals evaluating MCP-based systems."

Exhaustive list of document sections: Confused Deputy Problem · Token Passthrough · Server-Side Request Forgery (SSRF) · State Handle Hijacking · **Local MCP Server Compromise** · OAuth Authorization URL Validation · stdio Transport Security in Proxy Scenarios · Mix-Up Attacks · Localhost Redirect URI Impersonation · CIMD Trust Policies · Scope Minimization.

**Every normative statement I could extract (the complete MUST/MUST NOT set):**
1. "MCP servers that implement authorization **MUST** verify all inbound requests. **MCP servers MUST NOT treat possession of a state handle as authentication.**" (State Handle Hijacking → Mitigation)
2. "MCP servers **SHOULD** use secure, non-deterministic handles generated with secure random number generators. **Avoid predictable or sequential identifiers** that could be guessed by an attacker. Expiring handles can also reduce the risk."
3. "MCP servers **SHOULD** bind handles server-side to the authenticated user, for example by keying stored state as `<user_id>:<handle>` where the user ID is derived from the verified token rather than supplied by the client, and reject a handle presented by any other principal."
4. "MCP servers **MUST NOT** accept any tokens that were not explicitly issued for the MCP server" (Token Passthrough → Mitigation). "Token passthrough is explicitly forbidden in the [authorization spec]."
5. Local MCP server guidance: "**MCP servers intending for their servers to be run locally SHOULD implement measures to prevent unauthorized usage from malicious processes:** Use the `stdio` transport to limit access to just the MCP client; **Restrict access if using an HTTP transport, such as: Require an authorization token; Use unix domain sockets or other Interprocess Communication (IPC) mechanisms with restricted access.**"
6. Local MCP server **risks** include: "**Data Exfiltration.** Attackers can access sensitive data, configuration files, and credentials stored on the user's system" and "Warn that MCP servers run with the same privileges as the client."
7. Client-side SHOULD list (for local servers): highlight dangerous command patterns; "**Display warnings for commands that access sensitive locations (home directory, SSH keys, system directories)**"; "Execute MCP server commands in a sandboxed environment with minimal default privileges"; "**Launch MCP servers with restricted access to the file system, network, and other system resources**"; "Use platform-appropriate sandboxing technologies (containers, chroot, application sandboxes, etc.)".
8. OAuth URL validation: clients **MUST** only allow `http://` and `https://` schemes (http only for loopback); **MUST** use `https://`; **MUST NOT** use shell commands to open URLs.

**What the doc does NOT say — verified by exhaustive grep for `storage|persist|secret|credential|PII|personal data|at rest|encrypt|privacy|GDPR|minimi[sz]`:**
- ❌ **Nothing about encrypting a server's own datastore.**
- ❌ **Nothing about PII, personal data, GDPR, or data-subject rights.**
- ❌ **Nothing about retention periods or deletion.**
- ❌ **Nothing about secrets management for a server's own configuration.**
- The only occurrences of "encrypt" and "sensitive" are: "Store the `state` value server-side (in a secure session store or **encrypted cookie**)"; "Display warnings for commands that access **sensitive locations** (home directory, SSH keys...)"; "**encrypted cookie**" in the CSRF mitigation; and Scope Minimization's "attacker obtains (via log leakage, memory scraping, or local interception) an access token."

**Conclusion to report:** **there is no MCP normative guidance on storing user memories, PII, or secrets in a server's own datastore.** The relevant normative statements that *do* exist are (a) never treat a state handle as authentication, (b) bind state to the authenticated user, (c) prefer stdio / UDS / token-gated HTTP for local servers, and (d) sandbox and restrict filesystem access. **Vestige should not claim MCP-spec compliance for its at-rest posture; there is nothing to comply with. The applicable regime is GDPR Art. 25/32 plus platform guidance (Apple), not MCP.** Note also the doc references a *different* doc for older protocol revisions: "For guidance on securing the server-assigned session IDs used by protocol version 2025-11-25 and earlier, see [Session Hijacking in the 2025-11-25 version of this page]" — if Vestige targets an older protocol revision, read that one instead; I did not fetch it.

---

# 4. Embedding / reranker model licensing

## 4.1 nomic-embed-text-v1.5 — ✅ Apache-2.0, commercially safe

- **Licence: `apache-2.0`.** Verified two ways: HuggingFace API `cardData.license = "apache-2.0"`, tag `license:apache-2.0`; and the raw card frontmatter line `license: apache-2.0`. ([HF API](https://huggingface.co/api/models/nomic-ai/nomic-embed-text-v1.5) · [raw card](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/raw/main/README.md), retrieved 2026-09-19)
- **Model metadata:** 136,731,648 params; created 2024-02-10; **lastModified 2026-04-07**; 14,708,283 downloads; 935 likes. Files include `onnx/model.onnx`, `onnx/model_fp16.onnx`, `onnx/model_int8.onnx`, `onnx/model_q4.onnx`, `onnx/model_bnb4.onnx` — **so official ONNX exports exist, including int8 and 4-bit quantised variants.** ([HF API](https://huggingface.co/api/models/nomic-ai/nomic-embed-text-v1.5))
- **Matryoshka truncation — the exact published table** (MTEB, SeqLen 8192 for all rows) ([model card](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/raw/main/README.md)):

| Model | SeqLen | Dim | MTEB |
|---|---|---|---|
| nomic-embed-text-v1 | 8192 | 768 | **62.39** |
| nomic-embed-text-v1.5 | 8192 | 768 | 62.28 |
| nomic-embed-text-v1.5 | 8192 | 512 | 61.96 |
| nomic-embed-text-v1.5 | 8192 | 256 | 61.04 |
| nomic-embed-text-v1.5 | 8192 | 128 | 59.34 |
| nomic-embed-text-v1.5 | 8192 | 64 | 56.10 |

  ⚠️ **The card publishes 768/512/256/128/64 — it does NOT publish a 384-dim row.** Vestige uses **768 → 384 Matryoshka truncation**. **The MTEB score at 384 dims is not published by Nomic.** By linear interpolation between 512 (61.96) and 256 (61.04) it lands ≈ 61.5, but **that is my extrapolation, not a sourced number — flagged.** Nomic's own framing is "flexibility to trade off the embedding size for a **negligible** reduction in performance," which the table shows is fair down to 256 (Δ 1.24 MTEB) but less so at 64 (Δ 6.18).
- **Truncation procedure is documented and non-obvious:** you must `layer_norm` first, *then* slice, *then* L2-normalise: `embeddings = F.layer_norm(embeddings, (768,)); embeddings = embeddings[:, :matryoshka_dim]; embeddings = F.normalize(embeddings, p=2, dim=1)`. ([model card](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/raw/main/README.md)) **Skipping the layer_norm step silently degrades quality.**
- **⚠️ TASK PREFIXES ARE MANDATORY, PER THE MODEL CARD.** Verbatim: "**Important**: the text prompt *must* include a *task instruction prefix*, instructing the model which task is being performed. For example, if you are implementing a RAG application, you embed your documents as `search_document: <text here>` and embed your user queries as `search_query: <text here>`." Documented prefixes: `search_document:`, `search_query:`, `clustering:`, `classification:`. ([model card](https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/raw/main/README.md))
  - **Direct finding for Vestige: `VESTIGE_NOMIC_PREFIXES` defaults to *off*.** The project is running the model off-label. This is a correctness/quality bug, not just a config choice — and it is coupled to `regenerate_embeddings`, meaning enabling it invalidates the whole existing vector store. **Mixing prefixed and unprefixed embeddings in one index silently degrades recall.** This is the highest-value engineering finding in §4.
- **Context length:** natively 8192 via RoPE scaling. The card gives the exact recipe: tokenizer `model_max_length=8192` + `rope_parameters = {"rope_theta": 1000.0, "rope_type": "dynamic", "factor": 2.0}`. **This requires `trust_remote_code` in older transformers; the card notes "From transformers v5.5.0 and sentence transformers v5.3.0, `trust_remote_code=True` will no longer be necessary."** ⚠️ **`trust_remote_code` is an arbitrary-code-execution vector — relevant to §3.5's supply-chain concern.**
- **No commercial-use restriction.** Apache-2.0 permits commercial use, modification, and redistribution, with the requirement to include the licence text and NOTICE, and to state changes. Training data is released in full ([contrastors repo](https://github.com/nomic-ai/contrastors)).
- **nomic-embed-text-v2-moe exists and is newer** — ✅ **also `apache-2.0`** ([HF API](https://huggingface.co/api/models/nomic-ai/nomic-embed-text-v2-moe), created 2025-02-07, lastModified 2025-04-01). Key spec differences: MoE, **475M total params / 305M active**, 768-dim (Matryoshka to 256), **⚠️ "Maximum Sequence Length: 512 tokens"** — a **16× reduction** from v1.5's 8192. That is likely disqualifying for long memory entries. Published BEIR/MIRACL: Nomic Embed v2 = BEIR 52.86 / MIRACL 65.80 (best MIRACL in class). ([v2-moe model card](https://huggingface.co/nomic-ai/nomic-embed-text-v2-moe/raw/main/README.md)) **fastembed supports it only behind the `nomic-v2-moe` feature, on the Candle backend, not ONNX** ([fastembed README](https://github.com/Anush008/fastembed-rs)).

## 4.2 Jina Reranker v2 — ❌ **NON-COMMERCIAL. See the LICENSING LANDMINE box at the top.**

Additional verified specs, should you license it commercially: **context length 1024 tokens**, sliding-window chunking beyond that, flash attention, trained for multilingual + function-calling-aware + text-to-SQL-aware reranking and code retrieval ([model card](https://huggingface.co/jinaai/jina-reranker-v2-base-multilingual/raw/main/README.md), retrieved 2026-09-19). The repo **does ship `onnx/model.onnx`** ([HF API file list](https://huggingface.co/api/models/jinaai/jina-reranker-v2-base-multilingual)), which is why fastembed can use it. Licences of the alternatives **were not verified in this session**: `BAAI/bge-reranker-base` and `BAAI/bge-reranker-v2-m3` are both listed in fastembed ([README](https://github.com/Anush008/fastembed-rs)) and are widely believed Apache-2.0 — **verify before relying on it.**

## 4.3 fastembed (the Rust crate)

- **Version `7.0.1`, published 2026-09-16** (i.e. three days before this report) — **a very fast-moving crate.** Release cadence is roughly weekly: 7.0.1 (2026-09-16), 7.0.0 (2026-09-16), 6.1.0 (2026-09-12), 6.0.3 (2026-09-07), 6.0.2 (2026-08-27), 6.0.1 (2026-08-23). 3,462,128 downloads. **Licence: `Apache-2.0`** (code only — not the weights). ([crates.io API](https://crates.io/api/v1/crates/fastembed))
  - ⚠️ **Semver-operational warning: a major version bump every few weeks (6.x → 7.x within 9 days of 6.1.0) means `fastembed = "7"` will pull breaking changes.** Pin an exact version.
- **It does NOT bundle ONNX Runtime as vendored source; it downloads prebuilt binaries by default.** From `fastembed` Cargo.toml `[features]`: `default = ["ort-download-binaries-native-tls", "hf-hub-native-tls", "image-models"]`, where `ort-download-binaries-native-tls = ["ort/download-binaries", "ort/tls-native"]`. Alternative: `ort-load-dynamic = ["ort/load-dynamic"]`. ([fastembed Cargo.toml v7.0.1](https://raw.githubusercontent.com/Anush008/fastembed-rs/main/Cargo.toml))
- **Backends:** `ort = { version = "=2.0.0-rc.13", default-features = false, features = ["ndarray", "std", "api-24"] }` — **exact-pinned to a release candidate of `ort`.** Candle is opt-in via `qwen3` / `nomic-v2-moe` features.
- **`ort` (the ONNX Runtime binding):** version **`2.0.0-rc.13`**, licence **`MIT OR Apache-2.0`**, badge reads "ONNX Runtime v1.30.0". ([ort Cargo.toml](https://raw.githubusercontent.com/pykeio/ort/main/Cargo.toml) · [ort README](https://github.com/pykeio/ort)). ⚠️ **`ort` is at a release candidate, not 1.0+.** ⚠️ **`fastembed` requests `api-24` while `ort`'s own default is `api-30`** — I did not verify which ONNX Runtime release each API level maps to; **flagged as unverified.**
- **ONNX Runtime itself: MIT Licence, "Copyright (c) Microsoft Corporation"** ([onnxruntime LICENSE](https://raw.githubusercontent.com/microsoft/onnxruntime/main/LICENSE)). ✅ Commercially safe.
- **Accelerator features fastembed exposes:** `directml`, `cuda`, `cudnn`, `mkl`, **`metal`**, **`accelerate`**. ⚠️ **Critical detail: `metal = ["qwen3", "nomic-v2-moe", "candle-core/metal", "candle-nn/metal"]` and `accelerate = ["qwen3", "nomic-v2-moe", "dep:accelerate-src", ...]` — both are gated on the *Candle* backends. There is NO CoreML feature and no accelerated EP for the ONNX path.** ([fastembed Cargo.toml](https://raw.githubusercontent.com/Anush008/fastembed-rs/main/Cargo.toml))
  > **So on an Apple-silicon Mac, Vestige's `nomic-embed-text-v1.5` (ONNX path) runs on the ONNX Runtime CPU execution provider — no Metal, no CoreML, no ANE.** Even though `ort` itself has a `coreml` feature ([ort Cargo.toml](https://raw.githubusercontent.com/pykeio/ort/main/Cargo.toml)), **fastembed does not surface it.** Enabling CoreML would require either patching/vendoring fastembed or using `ort` directly. **This is a concrete, actionable performance finding.**
- **`nomic-embed-text-v1.5` in fastembed uses `Pooling::Mean`** and has both a full and a dynamically-quantised variant (`NomicEmbedTextV15Q` → `QuantizationMode::Dynamic`). ([fastembed `src/text_embedding/impl.rs`](https://raw.githubusercontent.com/Anush008/fastembed-rs/main/src/text_embedding/impl.rs), retrieved 2026-09-19)
- **Reranking** is a first-class `TextRerank` API with `RerankInitOptions` (`max_length`, `model_name`, `execution_providers`, `cache_dir`, `intra_threads`, `session_config`). Model list is `reranker_model_list()`. ([fastembed `src/reranking/impl.rs`](https://raw.githubusercontent.com/Anush008/fastembed-rs/main/src/reranking/impl.rs))

## 4.4 Local ONNX inference: memory, cold start, CPU vs accelerator — **published numbers are scarce; flagged**

**I could not find authoritative, quantitative published benchmarks for nomic-embed-text-v1.5 (or Jina Reranker v2) memory footprint / cold-start latency / CPU-vs-CoreML on Apple silicon.** Specifics of what I checked and found:

- **ONNX Runtime's own performance documentation** ([onnxruntime.ai/docs/performance](https://onnxruntime.ai/docs/performance/), retrieved 2026-09-19) is a **table of contents only** — the landing page lists sections ("Tune performance", "Model optimizations", "Transformers optimizer", "End to end optimization with Olive", "Device tensors", "Tune Mobile Performance") and contains **no numbers**. Flagged.
- **`ort`'s README** advertises "super quick" and "light enough to run on your users' devices" with **no quantified figures** ([ort README](https://github.com/pykeio/ort)).
- **A rough, defensible *estimate* from verified metadata (my calculation, clearly labelled as such):** nomic-embed-text-v1.5 is 136.7M params ([HF API](https://huggingface.co/api/models/nomic-ai/nomic-embed-text-v1.5)). At fp32 that is ≈ 547 MB of weights; the shipped `model_int8.onnx` would be ≈ 137 MB and `model_q4.onnx` ≈ 68 MB. Jina Reranker v2 at 278.4M params ([HF API](https://huggingface.co/api/models/jinaai/jina-reranker-v2-base-multilingual)) is ≈ 1.11 GB at fp32, ≈ 278 MB at int8. **These are arithmetic from parameter counts, not measurements.** Combined resident footprint will exceed weights because of activations, tokenizer, and the ORT arena allocator. **A ~1.1 GB reranker is a serious problem for a "local-first desktop app" — and Vestige loads it in the same process as the MCP server.**
- **No published cold-start latency figure found for either model.** Expect the dominant cost to be first-call session initialisation + tokenizer load; the `hf-hub` cache means weights download once ([fastembed Cargo.toml](https://raw.githubusercontent.com/Anush008/fastembed-rs/main/Cargo.toml) default features include `hf-hub-native-tls`). **Flagged: unverified; measure locally and report the number.**
- **Supply-chain note:** with default features, `fastembed` **downloads model weights and the ONNX Runtime binary at runtime from HuggingFace and from pyke.io/GitHub releases.** For a security-sensitive local tool this is a meaningful trust boundary, and it interacts with §3.5's finding that the MCP spec says nothing about it. `ort-load-dynamic` + a user-supplied ORT is the hardened alternative.

## 4.5 Licence compliance: what each implies for redistributing a binary that downloads weights

| Licence | Commercial use | Redistribute weights | Redistribute *your binary* that downloads them at runtime | Key obligations |
|---|---|---|---|---|
| **Apache-2.0** (nomic v1.5, nomic v2-moe, fastembed code, ONNX Runtime, usearch, sqlite-vec) | ✅ Yes | ✅ Yes | ✅ Yes | Include the licence text + `NOTICE` file if present; **state significant changes**; patent grant; trademark use not granted |
| **MIT** (rusqlite, `ort` option, sqlite-vec dual) | ✅ Yes | ✅ Yes | ✅ Yes | Include copyright + permission notice |
| **MIT/Apache-2.0 dual** (sqlite-vec, hnsw_rs) | ✅ Yes | ✅ Yes | ✅ Yes | Choose either; Apache-2.0 adds the patent grant |
| **BSD-3-Clause** (SQLCipher Community) | ✅ Yes | ✅ Yes | ✅ Yes | Include the licence; **no endorsement using Zetetic's name**. ⚠️ **Additionally, Zetetic's own licence page imposes a user-visible attribution requirement** ("must be a user accessible location") ([zetetic.net/sqlcipher/license](https://www.zetetic.net/sqlcipher/license/)) |
| **CC-BY-NC-4.0** (Jina Reranker v2) | ❌ **NO** | ❌ **NO** | ❌ **NO** | **NonCommercial only.** "Research and evaluation purposes." Commercial use requires Jina's API/SageMaker/Azure marketplace or a licence |
| **Public domain** (SQLite core) | ✅ Yes | ✅ Yes | ✅ Yes | None |
| **SEE (proprietary)** | ✅ With paid licence | ❌ **"you cannot distribute source code"**; compiled binaries only; one licence per "team" | ✅ | Perpetual, one-time fee; **price not verified** |

**Two practical notes:**
1. **"Downloads at runtime" is not a licence loophole.** CC-BY-NC-4.0 restricts *use*, not just distribution. A commercial product that instructs the software to download and run NC-licensed weights is using them commercially. This is the most common misconception in this space and it does not hold.
2. **Apache-2.0's "state changes" clause** matters if you quantise or re-export any Apache-licensed weights yourself (e.g. producing your own ONNX export): you must mark the files as changed.

---

# 5. Token-budget management for context injection

## 5.1 Mem0's published numbers — the exact figures, and an important correction

**Source A — the peer-review-style paper:** Chhikara, Khant, Aryan, Singh, Yadav, *"Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory"*, **[arXiv:2504.19413](https://arxiv.org/abs/2504.19413), submitted 28 Apr 2025 (v1 only; no revisions)**.

Abstract claims, verbatim: "**Mem0 achieves 26% relative improvements in the LLM-as-a-Judge metric over OpenAI**, while Mem0 with graph memory achieves around **2% higher overall score** than the base configuration. ... Mem0 attains a **91% lower p95 latency** and **saves more than 90% token cost**."

**Source B — the paper's Table 2 (the actual measured data, LOCOMO benchmark)** ([arXiv HTML v1](https://arxiv.org/html/2504.19413v1)). This is far more informative than the abstract:

| Method | Memory tokens | Search p50 (s) | Search p95 (s) | Total p50 (s) | Total p95 (s) | Overall J (%) |
|---|---|---|---|---|---|---|
| **Full-context** | **26,031** | — | — | 9.870 | **17.117** | **72.90 ± 0.19** |
| Mem0 | **1,764** | 0.148 | **0.200** | **0.708** | **1.440** | 66.88 ± 0.15 |
| Mem0^g (graph) | 3,616 | 0.476 | 0.657 | 1.091 | 2.590 | 68.44 ± 0.17 |
| Zep | 3,911 | 0.513 | 0.778 | 1.292 | 2.926 | 65.99 ± 0.16 |
| OpenAI (memory) | 4,437 | — | — | 0.466 | 0.889 | 52.90 ± 0.14 |
| A-Mem | 2,520 | 0.668 | 1.485 | 1.410 | 4.374 | 48.38 ± 0.15 |
| LangMem | 127 | 17.99 | **59.82** | 18.53 | 60.40 | 58.10 ± 0.21 |
| Best RAG (k=2, chunk 256) | — | 0.255 | 0.699 | 0.802 | 1.907 | 60.97 ± 0.20 |

**🚩 CORRECTION TO THE COMMONLY-REPEATED CLAIM — the full-context baseline SCORED HIGHEST (72.90% vs Mem0 66.88%).** The paper is explicit: "**a full-context method that ingests a chunk of roughly 26,000 tokens still achieves the highest J score (approximately 73%).** However ... it also incurs a very high total p95 latency—around 17 seconds." Mem0's win is **cost and latency, not accuracy**. Any citation of Mem0 as "more accurate than full context" is a misreading. Mem0^g came closest (68.44%) but still trailed.
- **Token reduction, computed from Table 2 (my arithmetic, not a paper claim):** 1,764 / 26,031 = **93.2% fewer tokens** for Mem0; 3,616 / 26,031 = **86.1%** for Mem0^g. The abstract's ">90% token cost" saving is consistent for base Mem0 but **overstated for Mem0^g**.
- **Latency reduction (my arithmetic vs the paper's own statements):** p95 17.117 → 1.440 s = **91.6% reduction** (paper says "92% reduction" in §4.3 and "91%" in the abstract — **the paper is internally inconsistent between 91% and 92%; flag this**). Mem0^g: 17.117 → 2.590 = **84.9%** (paper says "85%").
- **RAG chunk-size sweep is the most directly actionable data for a memory system** (Table 2, k=1 and k=2, chunk sizes 128–8192). At **k=2**, J score by chunk size: 128 → 59.56, **256 → 60.97 (peak)**, 512 → 58.19, 1024 → 50.68, 2048 → 48.57, 4096 → 51.79, 8192 → 60.53. **Accuracy peaks at 256-token chunks and degrades sharply through 2048 tokens before recovering at 8192.** At k=1, the peak is at 256 (50.15) and degrades monotonically to 4096 (36.84). **This is quantitative evidence that smaller retrieved units beat larger ones in this benchmark family** — and it supports Vestige's "atomic memory" rule empirically, not just anecdotally.
- **Methodology caveats I must flag:** (a) LOCOMO is a single benchmark; (b) the paper reports a **"26% relative improvement over OpenAI"** in the abstract but the OpenAI baseline in Table 2 is described as not performing memory search at all ("the OpenAI implementation does not perform memory search, as it processes manually extracted memories from their playground"), and the paper concedes "it requires pre-extraction of relevant context, which is not reflected in the reported metrics" — **so the headline 26% comparison is against a baseline the authors themselves say is measured unfairly**; (c) the LangMem implementation shows p50 search latency of 17.99 s, which is so anomalous that the authors call it "impractical for interactive applications" — likely an integration artifact, not an inherent property.

**Source C — the mem0 README (updated since the paper), which reports much higher numbers and a different methodology** ([mem0 README, "New Memory Algorithm (April 2026)"](https://github.com/mem0ai/mem0/blob/main/README.md), retrieved 2026-09-19):

| Benchmark | Old | New | Tokens | Latency p50 |
|---|---|---|---|---|
| LoCoMo | 71.4 | **92.5** | 7.0K | 0.88 s |
| LongMemEval | 67.8 | **94.4** | 6.8K | 1.09 s |
| BEAM (1M) | — | 64.1 | 6.7K | 1.00 s |
| BEAM (10M) | — | 48.6 | 6.9K | 1.05 s |

  🚩 **This conflicts with the paper.** LoCoMo jumps from 71.4 to 92.5 and **token usage jumps from 1,764 (paper) to 7.0K (README)** — a 4× increase in injected tokens for a much higher score. The README's own disclosure: "**Scores reflect Mem0's managed platform, which includes proprietary optimizations not available in the open-source SDK; open-source users should expect directionally similar gains but not identical numbers.**" Methodology described: "**Single-pass retrieval (one call, no agentic loops) at a top_200 retrieval budget.**" Architectural changes listed: single-pass ADD-only extraction (no UPDATE/DELETE), agent-generated facts as first-class, entity linking, multi-signal retrieval (semantic + BM25 + entity matching, fused). **Flag clearly: the README's numbers post-date the paper, are vendor-self-reported, and are not independently reproduced. The 7.0K token figure and the 92.5 score are the numbers to cite for the *current* product.**

## 5.2 "Lost in the Middle" — Liu et al., TACL (arXiv:2307.03172)

Source: [arXiv:2307.03172](https://arxiv.org/abs/2307.03172) (abstract page) and [arXiv HTML v3](https://arxiv.org/html/2307.03172v3). Submitted 6 Jul 2023; **v3 last revised 20 Nov 2023**; "Accepted for publication in Transactions of the Association for Computational Linguistics (TACL), 2023."

**Exact quantitative findings:**
- **Models tested:** MPT-30B-Instruct, LongChat-13B (16K), GPT-3.5-Turbo, GPT-3.5-Turbo (16K), Claude-1.3, Claude-1.3 (100K); GPT-4 (8K) on a subset. Contexts of **10, 20, and 30 documents**.
- **Closed-book vs oracle baselines (Table 1)** — essential for interpreting the degradation:

| Model | Closed-Book | Oracle |
|---|---|---|
| LongChat-13B (16K) | 35.0% | 83.4% |
| MPT-30B-Instruct | 31.5% | 81.9% |
| **GPT-3.5-Turbo** | **56.1%** | **88.3%** |
| GPT-3.5-Turbo (16K) | 56.0% | 88.6% |
| Claude-1.3 | 48.3% | 76.1% |
| Claude-1.3 (100K) | 48.2% | 76.4% |

- **Headline degradation:** "**GPT-3.5-Turbo's multi-document QA performance can drop by more than 20%—in the worst case, performance in 20- and 30-document settings is lower than performance without any input documents (i.e., closed-book performance; 56.1%).**" The shape is a **U-curve**: "highest when relevant information occurs at the very beginning (primacy bias) or end of its input context (recency bias), and performance significantly degrades when models must access and use information in the middle of its input context."
- **Extended-context models are not better:** "we find that models often have identical performance to their extended-context counterparts, indicating that extended-context models are not necessarily better at using their input context."
- **🚩 The single most important number for a RAG/memory designer — retrieval saturation:** "When retrieving from Wikipedia to answer queries from NaturalQuestions-Open, we find that **model performance saturates long before retriever recall saturates**, indicating that current models fail to effectively use additional retrieved documents—**using 50 documents instead of 20 retrieved documents only marginally improves performance (∼1.5% for GPT-3.5-Turbo and ∼1% for claude-1.3).**"
  > **Actionable:** going from top-20 to top-50 retrieved items buys ~1–1.5 points. That is a very weak return for 2.5× the context. **A relevance-threshold cutoff plus a hard top-k in the 10–20 range is the evidence-supported default; the marginal value of items 21–50 is near zero.**
- **Mitigation tested:** "Query-aware contextualization (placing the query before and after the documents or key-value pairs) enables near-perfect performance on the **synthetic key-value task**, but **minimally changes trends in multi-document QA**." So query-bracketing is **not** a general fix — flag this, it is often cited as one.
- ⚠️ **Critical caveat I must state: this is a 2023 paper about 2023 models (GPT-3.5-Turbo, Claude-1.3, MPT-30B).** Much of its U-curve is attributable to positional-encoding and training-distribution artefacts that have been substantially mitigated by later long-context training. **It is still the canonical citation, but any claim that "mid-context information is lost in Claude Sonnet 5 / Opus 5" is an extrapolation the paper does not support.** Anchoring memory injections at the *start or end* of the prompt remains cheap insurance regardless.

## 5.3 Anthropic's context-engineering guidance

Source: [anthropic.com/engineering/effective-context-engineering-for-ai-agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents), **published Sep 29, 2025**.

- **The term Anthropic uses is "context rot" — not "lost in the middle":** "Studies on needle-in-a-haystack style benchmarking have uncovered the concept of **context rot**: **as the number of tokens in the context window increases, the model's ability to accurately recall information from that context decreases.** While some models exhibit more gentle degradation than others, **this characteristic emerges across all models.**"
- **The governing principle, quoted verbatim:** "good context engineering means finding the **smallest possible set of high-signal tokens** that maximize the likelihood of some desired outcome." Restated in the conclusion: "the guiding principle remains the same: find the smallest set of high-signal tokens that maximize the likelihood of your desired outcome."
- **The mechanism Anthropic gives:** "LLMs have an '**attention budget**' that they draw on when parsing large volumes of context. **Every new token introduced depletes this budget by some amount.**" Architectural reason: attention gives "**n² pairwise relationships for n tokens**", so "as its context length increases, a model's ability to capture these pairwise relationships gets stretched thin."
- **Shape of the degradation:** "These factors create a **performance gradient rather than a hard cliff**: models remain highly capable at longer contexts but may show reduced precision for information retrieval and long-range reasoning."
- **⚠️ Anthropic publishes NO specific token number.** There is no "keep injected context under N tokens" guidance. The only quantitative framing is relative ("smallest possible set"). **Do not attribute a numeric budget to Anthropic — they do not give one.**
- **Three named techniques for long-horizon tasks:** **compaction** ("taking a conversation nearing the context window limit, summarizing its contents, and reinitiating a new context window with the summary... typically serves as the first lever"), **structured note-taking**, and **multi-agent architectures**.
- **The hybrid retrieval model Anthropic endorses, which is exactly a memory-server design:** "the most effective agents might employ a hybrid strategy, **retrieving some data up front for speed, and pursuing further autonomous exploration at its discretion**. ... **Claude Code is an agent that employs this hybrid model: CLAUDE.md files are naively dropped into context up front, while primitives like glob and grep allow it to navigate its environment and retrieve files just-in-time, effectively bypassing the issues of stale indexing and complex syntax trees.**"
  > **Actionable for Vestige:** this is the strongest published argument for the `expandable` IDs + `memory(action="get_batch")` pattern already in the tool contract. The correct budget shape is **small up-front injection + cheap, agent-driven drill-down**, not a large up-front dump. Anthropic does not quantify the split.
- Also relevant: "**Note that the terminology around 'context poisoning', 'context distraction' and 'context confusion' is not Anthropic's.** Anthropic's post names only 'context rot' and 'context pollution'/'context pollution constraints'." **Flag: the four-way "poisoning/distraction/confusion/clash" taxonomy circulating online is from Drew Breunig's blog, not Anthropic.** I did not fetch Breunig's post in this session — flagged as unattributed here.

## 5.4 Prompt caching — concrete numbers, and the ordering recommendation

Source: [docs.anthropic.com/en/docs/build-with-claude/prompt-caching](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching), retrieved 2026-09-19. **I could not extract a publication/last-updated date from the page — flagged.**

- **Minimum cacheable prompt length, per model (exact, from the "Cache limitations" section):**
  - **512 tokens** — Claude Fable 5.1, Claude Mythos 5.1, Claude Opus 5, Claude Fable 5, Claude Mythos 5
  - **2,048 tokens** — Claude Mythos Preview, Claude Opus 4.7
  - **4,096 tokens** — Claude Opus 4.6, Claude Opus 4.5
  - **1,024 tokens** — Claude Opus 4.8, Claude Sonnet 5, Claude Sonnet 4.6, Claude Sonnet 4.5, Claude Opus 4.1 (retired except Bedrock/GCP), Claude Opus 4 (retired except GCP), Claude Sonnet 4 (retired except…)
  > **This directly contradicts the premise in the research brief that it is "e.g. 1024 or 2048". The correct answer as of 2026-09 is: it varies by model across 512 / 1024 / 2048 / 4096.** For the newest frontier models it is now **512**.
- **TTL:** "**By default, the cache has a 5-minute lifetime.** The cache is refreshed for no additional cost each time the cached content is used." A 1-hour TTL is available at extra cost via `"cache_control": {"type": "ephemeral", "ttl": "1h"}`.
- **⚠️ Lifetime accounting subtlety that trips people up:** "The lifetime is measured from the **start of the request that writes or reads the cache entry**, not from the end of its response. Time spent generating a response counts against the lifetime: **if a response takes 4 minutes to stream, a follow-up request that reuses the same cached prefix must start within about 1 minute of that response completing.**"
- **Cost multipliers (exact):** "**5-minute cache write tokens are 1.25 times** the base input tokens price; **1-hour cache write tokens are 2 times** the base input tokens price; **Cache read tokens are 0.1 times** the base input tokens price." Footnote exception: "Cache hits and refreshes on Claude Fable 5.1 and Claude Mythos 5.1 are priced at **0.025x** the base input price." These multipliers "stack with other pricing modifiers such as the Batch API discount and data residency."
- **Breakpoints: 4 maximum.** "When used together, the automatic cache breakpoint **uses one of the 4 available breakpoint slots**."
- **✅ THE ORDERING ANSWER — yes, stable ordering matters, and Anthropic states it as a hard requirement.** Verbatim:
  > "**Prompt caching caches the full prefix** — Prompt caching references the entire prompt - **tools, system, and messages (in that order)** up to and including the block designated with `cache_control`."
  > "**Cache writes happen only at your breakpoint.** Marking a block with `cache_control` writes exactly one cache entry: a hash of the prefix ending at that block. The system does not write entries for any earlier position. **Because the hash is cumulative, covering everything up to and including the breakpoint, changing any block at or before the breakpoint produces a different hash on the next request.**"
  > From the troubleshooting checklist: "Confirm your breakpoint is on a block that stays identical across requests. **Cache writes happen only at the breakpoint, and if that block changes (timestamps, per-request context, the incoming message), the prefix hash never matches.** The lookback does not find stable content behind the breakpoint; it only finds entries that earlier requests wrote at their own breakpoints."
  > "Verify that the keys in your `tool_use` content blocks have **stable ordering** as some languages (for example, Swift, Go) randomize key order during JSON conversion, breaking caches."
- **Lookback window: 20 blocks.** "The lookback window is 20 blocks. The system checks at most 20 positions per breakpoint... If a growing conversation pushes your breakpoint 20 or more blocks past the last write, the lookback window misses it. **Add a second breakpoint closer to that position from the start so a write accumulates there before you need it.**"
- **Concurrency caveat:** "For concurrent requests, note that **a cache entry only becomes available after the first response begins.** If you need cache hits for parallel requests, wait for the first response before [sending the rest]."

### 🎯 Direct architectural consequences for Vestige

1. **🚩 CURRENT DESIGN BREAKS CACHING.** If `session_context` injects a *freshly retrieved, query-dependent* memory block into the system prompt (or early in the message), the **prefix hash changes on every turn** and the cache never hits. This is the single most impactful interaction between a memory system and prompt caching, and Anthropic's doc is unambiguous about it.
2. **The cache-compatible shape:** put **stable content first** — tool definitions, then the agent's static system prompt, then a stable "durable memory" block (user preferences, project conventions, decisions) marked with a breakpoint — and put **volatile retrieved context last**, after the breakpoint, in the final user message. Then the volatile part is re-sent as ordinary input tokens each turn (cheap, no cache write) while everything before it reads at 0.1×.
3. **Two-tier memory maps naturally onto two breakpoints:** (a) slow-changing profile/preferences block = long-lived, 1-hour TTL; (b) the volatile search results = after the last breakpoint, never cached. Anthropic's "use 2 breakpoints when a growing conversation pushes past the lookback window" guidance applies directly if the agent has long sessions.
4. **Minimum-cacheable-floor as a design constraint:** on Sonnet 5 / Opus 4.8 the stable prefix must be **≥ 1,024 tokens**; on Opus 4.6/4.5 it must be **≥ 4,096**. Anthropic: "If your prompt falls just short of the minimum for your model and platform, **expanding the cached content to reach the threshold is often worthwhile.**" **A memory system that injects a 400-token preference block on Opus 4.5 gets zero caching benefit — the design should either pad to the threshold or accept no caching.**
5. **`tools` ordering matters too.** "tools, system, and messages (in that order)" — so **tool definition order and content are part of the prefix hash.** Vestige exposes 28 MCP tools. If the tool list is emitted in nondeterministic order (HashMap iteration order in Rust is a real hazard here), **every request misses the cache.** Verify `build_tools_list` in `catalog.rs` returns a deterministic, stable order.
6. **`session_context`'s `token_budget` parameter default of 2000 sits just above the 1,024 floor for Sonnet-class and well below the 4,096 floor for Opus 4.5/4.6.** Budget presets should be model-aware if caching is a goal.

## 5.5 Zep / Graphiti — the cleanest token-reduction numbers available

Source: Rasmussen, Paliychuk, Beauvais, Ryan, Chalef, *"Zep: A Temporal Knowledge Graph Architecture for Agent Memory"*, **[arXiv:2501.13956](https://arxiv.org/abs/2501.13956), submitted 20 Jan 2025**.

- **DMR benchmark:** Zep **94.8% vs MemGPT 93.4%** (gpt-4-turbo). With gpt-4o-mini, Zep 98.2%. Baselines: full-conversation 94.6% (gpt-4), session-summaries 78.6%. ⚠️ **The paper itself disclaims this result:** "these results must be contextualized: **each conversation contains only 60 messages, easily fitting within current LLM context windows**," and "The evaluation relies exclusively on single-turn, fact-retrieval questions that fail to assess complex memory understanding."
- **LongMemEval — Table 2, the key table (exact):**

| Memory | Model | Score | Latency | Latency IQR | **Avg Context Tokens** |
|---|---|---|---|---|---|
| Full-context | gpt-4o-mini | 55.4% | 31.3 s | 8.76 s | **115k** |
| **Zep** | gpt-4o-mini | **63.8%** | **3.20 s** | 1.31 s | **1.6k** |
| Full-context | gpt-4o | 60.2% | 28.9 s | 6.01 s | **115k** |
| **Zep** | gpt-4o | **71.2%** | **2.58 s** | 0.684 s | **1.6k** |

  - **Token reduction (my arithmetic): 1,600 / 115,000 = 98.6% fewer context tokens.** Accuracy improvement: +15.2% (gpt-4o-mini) and **+18.5%** (gpt-4o). Latency: 31.3 s → 3.20 s ≈ **89.8% reduction** (the paper says "approximately 90%").
  - 🚩 **Unlike Mem0, Zep beats full-context on accuracy while using 98.6% fewer tokens.** But **both benchmarks were run by Zep's own authors on their own system**, and Mem0's paper reports Zep scoring *below* both Mem0 variants (65.99 vs 66.88/68.44) on LOCOMO. **These two vendor papers use different benchmarks, different LLMs, different baselines, and each reports the other losing. Neither is independent. Flag the conflict.**
  - **The 1.6k-token figure is the best available anchor for a memory-injection budget**: it is a production system's measured average injected context on a hard multi-session benchmark, using a graph-based memory with top-10 node/edge retrieval. ✓ **1.6k tokens ≈ a 1,024–2,048 budget.** This corroborates Vestige's existing `token_budget` presets (500 / 2000 / 3000–5000) as sane, with 2000 as the well-supported default.

## 5.6 Letta / MemGPT — the memory-hierarchy design (no token numbers published)

Source: Packer, Wooders, Lin, Fang, Patil, Stoica, Gonzalez, *"MemGPT: Towards LLMs as Operating Systems"*, **[arXiv:2310.08560](https://arxiv.org/abs/2310.08560), submitted 12 Oct 2023, last revised 12 Feb 2024 (v2)**.

- **Design, verbatim:** "we propose **virtual context management**, a technique drawing inspiration from **hierarchical memory systems in traditional operating systems that provide the appearance of large memory resources through data movement between fast and slow memory**. ... MemGPT (Memory-GPT), a system that intelligently manages **different memory tiers** in order to effectively provide extended context within the LLM's limited context window, and **utilizes interrupts to manage control flow between itself and the user**."
- Evaluated on document analysis (documents "that far exceed the underlying LLM's context window") and multi-session chat.
- ⚠️ **The abstract publishes NO token-budget numbers, no memory-pressure thresholds, and no recursive-summarization trigger points.** I checked the abstract page; the paper body may contain them but **I did not extract them — flagged as unverified.** Do not cite a "MemGPT threshold" without reading the body.
- **The architectural takeaway that IS verifiable:** MemGPT's "interrupts" model means the *agent* decides when to page memory in and out. That is the same "just-in-time retrieval" shape Anthropic endorses (§5.3) — an on-demand tool call, not an up-front dump.

## 5.7 Other published token-budget heuristics

**I did not find a single authoritative source publishing a numeric "keep injected memory under N tokens" rule.** The best available quantitative anchors are, in order of quality:
1. **Zep's measured 1.6k average context tokens** (Table 2, [arXiv:2501.13956](https://arxiv.org/abs/2501.13956), 2025-01-20) — production system, hard benchmark.
2. **Mem0's measured 1,764 memory tokens** (Table 2, [arXiv:2504.19413](https://arxiv.org/abs/2504.19413), 2025-04-28) — and its **updated 7.0K** for the April-2026 algorithm ([README](https://github.com/mem0ai/mem0/blob/main/README.md)).
3. **Lost-in-the-Middle's retrieval-saturation finding**: 20 → 50 documents buys ~1–1.5% ([arXiv:2307.03172](https://arxiv.org/abs/2307.03172), v3 2023-11-20) — evidence for a **cap in the 10–20 item range**.
4. **Mem0's chunk-size sweep**: **256-token retrieval units are the accuracy peak**; 1024–4096-token chunks are materially worse (Table 2, [arXiv:2504.19413](https://arxiv.org/abs/2504.19413)) — evidence for **short retrieved units**.
5. **Anthropic's "smallest possible set of high-signal tokens"** ([2025-09-29](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)) — qualitative.

**Relevance-threshold cutoff vs top-k, and MMR/diversity:** ⚠️ **I did not find published quantitative guidance on either.** The original MMR paper is Carbonell & Goldstein, *"The Use of MMR, Diversity-Based Reranking for Reordering Documents and Producing Summaries"*, SIGIR 1998 — **I did not fetch or verify it in this session; flagged.** Lost-in-the-Middle's saturation finding is the closest thing to an evidence-based argument for a *smaller* k, and it argues for both a threshold and a hard cap rather than one or the other. **No source I found supports a specific threshold value (e.g. cosine 0.7).**

---

# Summary of flags: unverified, deprecated, conflicting

## 🚩 LICENSING (act on this)
1. **`jinaai/jina-reranker-v2-base-multilingual` = CC-BY-NC-4.0, NON-COMMERCIAL.** Verified two ways (HF API + model card). Blocks commercial release.
2. **SQLCipher Community mandates user-visible attribution** (stricter than plain BSD-3-Clause), per Zetetic's own licence page.
3. **SEE licence forbids source distribution and is per-"team"**; **SEE pricing could not be verified** (`/see/purchase.wiki` → HTTP 404).
4. **`sqlite3mc`/wxSQLite3 README could not be fetched (HTTP 404)** — status unverified.
5. **fastembed's Apache-2.0 covers code, not weights** — do not treat "fastembed supports model X" as a licence statement about X.

## 🚩 CONFLICTING SOURCES
6. **Mem0 paper vs Mem0 README:** LoCoMo 66.88 (paper) vs 92.5 (README); tokens 1,764 vs 7.0K; the README explicitly says its numbers come from a proprietary managed platform not in the OSS SDK.
7. **Mem0 vs Zep on each other:** Mem0's paper puts Zep at 65.99 (below both Mem0 variants); Zep's paper reports beating everything on DMR/LongMemEval. Different benchmarks, no independent replication.
8. **Mem0's p95 latency reduction is stated as both "91%" and "92%" within the same paper.**
9. **Mem0's "26% over OpenAI" baseline is one the authors themselves say wasn't measured fairly.**
10. **Anthropic's minimum cacheable prompt length is 512/1024/2048/4096 depending on model** — the brief's premise of "1024 or 2048" is now incomplete; newest frontier models are at 512.
11. **Prompt-caching doc has no extractable publication/last-updated date.**

## 🚩 UNVERIFIED / NOT FOUND (do not present these as fact)
12. **No primary-source benchmark exists for "how many concurrent WAL readers" with QPS numbers.** Fly.io explains the mechanism without numbers; SQLite publishes no figure. D1's "1 ms → 1,000 QPS" is a Cloudflare-platform rule of thumb, not a SQLite benchmark.
13. **The "~80K commits/s single-writer ceiling" number is an unrefereed SQLite forum post**, with an explicitly *projected* (not measured) end-to-end result for its proposal.
14. **FTS5 index size overhead (~1.9× external-content, ~3.5× content-storing) is a single community forum measurement** (2024-05-20). SQLite publishes no official figure. Similarly, the `detail` option's space saving is unquantified in the docs.
15. **No official Apple or SQLite documentation exists for Time Machine + WAL-mode SQLite.** The only evidence is a 2018 sqlite-users thread (with a reply from D. Richard Holl... [D. Richard Hipp]) describing a suspected few-milliseconds-apart capture. **Treat as a real hazard with weak documentation.**
16. **`fdesetup status`, `tmutil addexclusion`, `NSURLIsExcludedFromBackupKey` were not verified against Apple documentation in this session.**
17. **Litestream publishes no numeric RPO/RTO guarantee.** The L0/L1/L2/L3 intervals (30 s / 5 min / 1 h / 24 h) are documented; an RPO SLA is not.
18. **Litestream × multi-process writers is undocumented.** Litestream deliberately holds a long-running read transaction, which SQLite documents as blocking checkpoint progress. No source addresses the combination.
19. **LiteFS current status and Turso embedded-replica RPO/RTO were fetched but not analysed** — flagged as gaps.
20. **No published local-ONNX benchmark for nomic-embed-text-v1.5 or Jina Reranker v2** on Apple silicon: no memory footprint, no cold-start latency, no CPU-vs-CoreML comparison. The parameter-count-derived size estimates in §4.4 are **arithmetic, not measurements**. ONNX Runtime's own performance page contains no numbers.
21. **CoreML is not reachable through fastembed** for the ONNX path — verified by reading fastembed's feature list; enabling it requires patching or using `ort` directly.
22. **Nomic does not publish MTEB at 384 dimensions**; Vestige truncates to 384. The ≈61.5 estimate is my linear interpolation.
23. **Morris et al.'s 92% exact-recovery figure is for 32-token inputs** and the abstract does not name the two embedding models tested. Do not generalise it to long memories or to nomic-embed-text-v1.5 specifically.
24. **No EDPB/DPA statement classifying embeddings as personal data was found.**
25. **EDPB Guidelines 3/2018 (territorial scope), the EU AI Act text, and NIST SP 800-57 / OWASP desktop-secrets guidance were not fetched.** Flagged as gaps.
26. **The MMR paper (Carbonell & Goldstein, SIGIR 1998) was not fetched.** MMR/diversity and relevance-threshold guidance: **nothing quantitative found.**
27. **MemGPT's token-budget thresholds are not in the abstract**; not extracted from the body.
28. **The "context poisoning / distraction / confusion / clash" taxonomy is NOT Anthropic's** — Anthropic names only "context rot" and "context pollution."
29. **The `ort` `api-24` vs `api-30` feature-to-ONNX-Runtime-version mapping was not verified.**
30. **`bge-reranker-base` / `bge-reranker-v2-m3` licences were not verified** — verify before using them as the Jina replacement.

## 🚩 DEPRECATED / SUPERSEDED
31. **`sqlite-vss` is explicitly not in active development** (README warning) — replaced by `sqlite-vec`.
32. **`sqlite-vec` is pre-v1** with a documented "expect breaking changes" warning; stable line is 0.1.9 (2026-03-31), alpha line 0.1.10-alpha.4 (2026-05-18).
33. **`instant-distance` (crates.io) last published 2023-06-26** — effectively stale for a new build.
34. **SQLite < 3.53.0 / < 3.51.3 carries the WAL-reset corruption bug** — and 3.51.3/3.53.0+ are only months old (2026-03-13 / 2026-04-09).
35. **Litestream 0.5.0 cannot restore 0.3.x backups** (format change); 0.5.1 and 0.5.2 fixed the migration bugs.
36. **Litestream's 0.3.x era is deprecated** by the site's own header ("View v0.3.14 (Previous)").

## ✅ Highest-value actionable findings for Vestige (ranked)
1. **Resolve the Jina reranker NC licence** before any commercial ship.
2. **Enable the Nomic task prefixes by default** (`search_query:` / `search_document:`) — the model card says they are mandatory; currently off by default, and enabling requires a full `regenerate_embeddings`.
3. **Pin SQLite ≥ 3.53.4** to close the WAL-reset bug, given the multi-client deployment shape is exactly the affected configuration.
4. **Reconsider the multi-process SQLite topology** — a single-writer daemon over a Unix domain socket removes the BUSY-on-connect, BUSY-during-recovery, checkpoint-starvation, and WAL-reset exposure, and aligns with the MCP spec's local-server guidance. If kept multi-process, add `busy_timeout`, use `BEGIN IMMEDIATE` for all writes, set a non-default `journal_size_limit`, and detect/reject network filesystems.
5. **Make backups restore-verified** (GDPR Art. 32(1)(c)–(d)) — the `needsBackup` trigger is agent-dependent, so a user who never triggers it never gets one. Use `VACUUM INTO` (single self-contained file, no `-wal`/`-shm`, purges deleted content) into a directory that is Time Machine–excluded for the live DB.
6. **Restructure injected context to be cache-friendly**: stable memory block before the cache breakpoint, volatile retrieval after it; verify deterministic tool-list ordering.
7. **Hard-cap retrieval at 10–20 items with a relevance threshold**, and keep retrieved units short (256-token range per Mem0's sweep) — with `expandable` IDs for drill-down, per Anthropic's hybrid-retrieval guidance.
8. **Strengthen the HNSW staleness check beyond row count** — a monotonic change counter or content hash; row-count validation cannot detect a same-size delete+insert.
9. **Decide the ANN deletion semantics deliberately**: tombstone-with-retained-vector is the GDPR-risky case; vector-purge-plus-tombstone plus periodic rebuild is the defensible one.
10. **Pick a Keychain accessibility class that works for the 6-hour background loop** (`AfterFirstUnlock` or `...ThisDeviceOnly`), not the default `WhenUnlocked` — otherwise the consolidation loop silently fails while the screen is locked.

---

# Primary sources

All URLs retrieved 2026-09-19 unless otherwise stated.

## SQLite (official)
| URL | Date / version |
|---|---|
| https://sqlite.org/wal.html | last updated 2026-08-25 19:42:39Z |
| https://sqlite.org/pragma.html | retrieved 2026-09-19 (sections: synchronous, busy_timeout, journal_size_limit, wal_autocheckpoint, locking_mode, cache_size) |
| https://sqlite.org/rescode.html | retrieved 2026-09-19 (§5 extended codes; (5), (261), (517), (773)) |
| https://sqlite.org/fts5.html | retrieved 2026-09-19 (§4.4.2, §4.4.3, §4.4.4, §4.6, §6.1, §6.2, §6.7, §6.8, §6.9, §6.10, §6.14) |
| https://sqlite.org/backup.html | retrieved 2026-09-19 |
| https://sqlite.org/lang_vacuum.html | retrieved 2026-09-19 |
| https://sqlite.org/howtocorrupt.html | retrieved 2026-09-19 (§1.2, §1.3, §2.1) |
| https://sqlite.org/changes.html | retrieved 2026-09-19; latest release 3.53.4, 2026-07-24; WAL-reset fix noted in 3.53.0, 2026-04-09 |
| https://sqlite.org/c3ref/busy_timeout.html | retrieved 2026-09-19 |
| https://sqlite.org/lockingv3.html | retrieved 2026-09-19 |
| https://sqlite.org/vec1/ | page generated 2026-08-28 |
| https://www.sqlite.org/see/doc/trunk/www/readme.wiki | retrieved 2026-09-19 |
| https://www.sqlite.org/see | retrieved 2026-09-19 (no pricing extractable) |
| https://sqlite.org/see/purchase.wiki | **HTTP 404** at retrieval |
| https://www.sqlite.org/forum/forumpost/c9d69d74c6644dd1?t=c | thread 2026-03-30 → 2026-04-15 |
| https://www.sqlite.org/forum/forumpost/97ef578678fc8c78 | retrieved 2026-09-19 (community; unverified numbers) |
| https://sqlite.org/forum/forumpost/53e724d12dbd2097?t=c | 2024-05-20 |
| https://sqlite.org/forum/forumpost/a2049876cc?t=h | retrieved 2026-09-19 |
| https://sqlite.org/rsync/doc/trunk/www/rsync.md | **HTTP 404** at retrieval; `sqlite3_rsync` documented instead at howtocorrupt.html §1.2 (available since 3.47.0, 2024-10-21) |

## Backup / replication
| URL | Date / version |
|---|---|
| https://litestream.io/ | retrieved 2026-09-19 (v0.5.x "Actively maintained") |
| https://litestream.io/how-it-works/ | retrieved 2026-09-19 |
| https://litestream.io/reference/config/ | retrieved 2026-09-19 |
| https://mtlynch.io/notes/hold-off-on-litestream-0.5.0/ | October 14, 2025; updated October 17, 2025 |
| https://theconsensus.dev/p/2026/08/23/another-look-at-sqlite-wal-reset.html | 2026-08-23 (cited by sqlite.org/wal.html) |
| https://fly.io/blog/sqlite-internals-wal/ | Ben Johnson; no date exposed; retrieved 2026-09-19 |
| https://developers.cloudflare.com/d1/platform/limits/ | last updated Apr 21, 2026 |
| https://rqlite.io/docs/guides/performance/ | retrieved 2026-09-19 |
| https://rqlite.io/docs/faq/ | retrieved 2026-09-19 |
| https://fly.io/docs/litefs/ | fetched, not analysed |
| https://docs.turso.tech/features/embedded-replicas/introduction | fetched, not analysed |

## Vector search
| URL | Date / version |
|---|---|
| https://github.com/asg017/sqlite-vec + /releases | v0.1.9 stable 2026-03-31; v0.1.10-alpha.4 2026-05-18 |
| https://alexgarcia.xyz/sqlite-vec/ | shows v0.1.10-alpha.4; MIT/Apache-2 |
| https://github.com/asg017/sqlite-vss | deprecation notice; retrieved 2026-09-19 |
| https://github.com/unum-cloud/usearch | retrieved 2026-09-19; Apache-2.0 |
| https://arxiv.org/abs/2512.06200 | submitted 5 Dec 2025; NeurIPS 2025 Workshop on ML for Systems |

## Encryption
| URL | Date / version |
|---|---|
| https://www.zetetic.net/sqlcipher/ | retrieved 2026-09-19 |
| https://www.zetetic.net/sqlcipher/license/ | retrieved 2026-09-19 |
| https://raw.githubusercontent.com/sqlcipher/sqlcipher/master/LICENSE.txt | Copyright (c) 2025 ZETETIC LLC |
| https://raw.githubusercontent.com/sqlcipher/sqlcipher/master/CHANGELOG.md | 4.18.0 = August 2026 (SQLite 3.53.4 baseline); 4.19.0 in progress |
| https://raw.githubusercontent.com/rusqlite/rusqlite/master/Cargo.toml | v0.40.1 in-tree |
| https://raw.githubusercontent.com/rusqlite/rusqlite/master/README.md | retrieved 2026-09-19 |
| https://raw.githubusercontent.com/rusqlite/rusqlite/master/libsqlite3-sys/sqlite3/sqlite3.h | SQLITE_VERSION "3.53.4" |
| https://crates.io/api/v1/crates/rusqlite | 0.40.2, 2026-08-08 |

## macOS
| URL | Date / version |
|---|---|
| https://support.apple.com/guide/mac-help/protect-data-on-your-mac-with-filevault-mh11785/mac | retrieved 2026-09-19 |
| https://support.apple.com/en-us/104984 | retrieved 2026-09-19 |
| https://developer.apple.com/documentation/security/restricting-keychain-item-accessibility.md | © 2026 Apple Inc. |
| https://developer.apple.com/documentation/security/ksecattraccessiblewhenunlocked.md | © 2026 Apple Inc. |
| https://marc.info/?l=sqlite-users&m=154462708903146&w=2 | 2018-12-12 (mailing-list; weakest source in this report) |

## Privacy / law
| URL | Date / version |
|---|---|
| https://gdpr-info.eu/art-2-gdpr/ | Regulation (EU) 2016/679 |
| https://gdpr-info.eu/art-3-gdpr/ | idem |
| https://gdpr-info.eu/art-5-gdpr/ | idem |
| https://gdpr-info.eu/art-17-gdpr/ | idem |
| https://gdpr-info.eu/art-25-gdpr/ | idem |
| https://gdpr-info.eu/art-32-gdpr/ | idem |
| https://gdpr-info.eu/recitals/no-18/ | idem |
| https://eur-lex.europa.eu/eli/reg/2016/679/oj | official OJ text |
| https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX:62013CJ0212 | judgment 11 December 2014 |
| https://arxiv.org/abs/2310.06816 | submitted 10 Oct 2023; EMNLP 2023 |
| https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices.md | protocol version 2026-07-28 |

## Models / inference
| URL | Date / version |
|---|---|
| https://huggingface.co/nomic-ai/nomic-embed-text-v1.5/raw/main/README.md | lastModified 2026-04-07; Apache-2.0 |
| https://huggingface.co/nomic-ai/nomic-embed-text-v2-moe/raw/main/README.md | lastModified 2025-04-01; Apache-2.0 |
| https://huggingface.co/jinaai/jina-reranker-v2-base-multilingual/raw/main/README.md | lastModified 2025-10-21; **CC-BY-NC-4.0** |
| https://raw.githubusercontent.com/Anush008/fastembed-rs/main/Cargo.toml | v7.0.1 |
| https://raw.githubusercontent.com/Anush008/fastembed-rs/main/README.md | retrieved 2026-09-19 |
| https://crates.io/api/v1/crates/fastembed | 7.0.1, 2026-09-16, Apache-2.0 |
| https://raw.githubusercontent.com/pykeio/ort/main/Cargo.toml | 2.0.0-rc.13, MIT OR Apache-2.0 |
| https://raw.githubusercontent.com/microsoft/onnxruntime/main/LICENSE | MIT, Microsoft Corporation |
| https://onnxruntime.ai/docs/performance/ | TOC only; no numbers |

## Token budget / memory systems
| URL | Date / version |
|---|---|
| https://arxiv.org/abs/2504.19413 + https://arxiv.org/html/2504.19413v1 | submitted 28 Apr 2025 (v1, no revisions) |
| https://github.com/mem0ai/mem0/blob/main/README.md | "New Memory Algorithm (April 2026)" |
| https://arxiv.org/abs/2307.03172 + https://arxiv.org/html/2307.03172v3 | v1 2023-07-06; v3 2023-11-20; TACL 2023 |
| https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents | Published Sep 29, 2025 |
| https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching | no date extractable; retrieved 2026-09-19 |
| https://arxiv.org/abs/2501.13956 + https://arxiv.org/html/2501.13956v1 | submitted 20 Jan 2025 |
| https://arxiv.org/abs/2310.08560 | v1 2023-10-12; v2 2024-02-12 |
| https://github.com/modelcontextprotocol/servers/blob/main/src/memory/README.md | retrieved 2026-09-19 |
| https://github.com/doobidoo/mcp-memory-service/blob/main/README.md | v11.8.4 dated August 25, 2026 |
| https://github.com/basicmachines-co/basic-memory/blob/main/README.md | retrieved 2026-09-19 |
| https://github.com/getzep/graphiti/blob/main/README.md | retrieved 2026-09-19 |
