# MCP Security, Agent Memory Poisoning & Prompt Injection — Verified Literature Survey

**Compiled:** 2026-09-19
**Method:** arXiv export API (`export.arxiv.org/api/query`, primary source) for systematic discovery + individual `arxiv.org/abs/*` page fetches for verification. Every arXiv ID below was confirmed to resolve.
**Key structural finding:** 2026 arXiv IDs (2601–2609) are heavily populated. Roughly **70 papers** match `all:"memory poisoning"` alone; **206** match `all:"Model Context Protocol" AND all:"security"`; **185** match `all:"indirect prompt injection"`. Reports claiming no 2026 work exists are wrong.

---

## A. THE SEVEN NAMED PAPERS — ALL VERIFIED

### A1. MCPTox
- **arXiv:** 2508.14925 — https://arxiv.org/abs/2508.14925
- **Exact title:** *MCPTox: A Benchmark for Tool Poisoning Attack on Real-World MCP Servers*
- **Submitted:** 19 Aug 2025 (v1 only). Accepted at AAAI (AAAI OJS record 40895).
- **Authors:** Zhiqiang Wang, Yichao Gao, Yanting Wang, Suyuan Liu, Haifeng Sun, Haoran Cheng, Guanquan Shi, Haohua Du, Xiangyang Li
- **Summary:** First systematic benchmark for tool poisoning — malicious instructions embedded in a tool's *metadata* rather than its output, so no execution is required. Built on 45 live real-world MCP servers and 353 authentic tools, generating 1,312 malicious test cases via three attack templates spanning 10 risk categories. Evaluates 20 prominent LLM agent configurations.
- **KEY QUANTITATIVE FINDINGS:** o1-mini ASR **72.8%**. More capable models are often *more* susceptible (exploits superior instruction-following). Highest refusal rate of any agent (Claude-3.7-Sonnet) **<3%** — existing safety alignment is ineffective. 45 servers / 353 tools / 1,312 cases / 10 risk categories / 20 agent settings.
- **Verification:** VERIFIED via arxiv.org/abs/2508.14925 fetch.

### A2. MCPSecBench
- **arXiv:** 2508.13220 — https://arxiv.org/abs/2508.13220
- **Exact title:** *MCPSecBench: A Systematic Security Benchmark and Playground for Testing Model Context Protocols*
- **Submitted:** v1 17 Aug 2025; v2 9 Oct 2025; **v3 12 Feb 2026** (still being revised in 2026)
- **Authors:** Yixuan Yang, Cuifeng Gao, Daoyuan Wu, Yufan Chen, Yingjiu Li, Shuai Wang (Lingnan University, Hong Kong)
- **Summary:** First formalization of a secure MCP plus required specifications; extends prior taxonomies with protocol-level and host-side threats. Introduces MCPSecBench: an integrated playground of prompt datasets, MCP servers/clients, attack scripts, GUI test harness and protection mechanisms, evaluated across three major MCP platforms. Modular and extensible.
- **KEY QUANTITATIVE FINDINGS:** **17 distinct attack types** across **4 primary attack surfaces**. All attack surfaces yield successful compromises; core vulnerabilities universally affect Claude, OpenAI and Cursor. Current protection mechanisms proved **largely ineffective — average success rate <30%**.
- **Verification:** VERIFIED via arxiv.org/abs/2508.13220 fetch.

### A3. MCP-Guard
- **arXiv:** 2508.10991 — https://arxiv.org/abs/2508.10991
- **Exact title:** *MCP-Guard: A Multi-Stage Defense-in-Depth Framework for Securing Model Context Protocol in Agentic AI*
- **Submitted:** v1 14 Aug 2025; v2 22 Aug 2025; **v3 5 Jan 2026; v4 8 Jan 2026**
- **Authors:** Wenpeng Xing, Zhonghao Qi, Yupeng Qin, Yilin Li, Caini Chang, Jiahui Yu, Changting Lin, Zhenzhen Xie, Meng Han
- **Summary:** A layered defense architecture for LLM-tool interactions. Three-stage detection pipeline: (1) lightweight static scanning for overt threats, (2) a deep neural detector for semantic attacks, (3) a fine-tuned E5-based model for adversarial prompt identification — finalized by an LLM arbitrator. Ships MCP-ATTACKBENCH for training and evaluation.
- **KEY QUANTITATIVE FINDINGS:** Fine-tuned E5 model achieves **96.01% accuracy** identifying adversarial prompts. **MCP-ATTACKBENCH = 70,448 samples** (augmented by GPT-4).
- **Verification:** VERIFIED via arxiv.org/abs/2508.10991 fetch.

### A4. "Breaking the Protocol"
- **arXiv:** 2601.17549 — https://arxiv.org/abs/2601.17549
- **Exact title:** *Breaking the Protocol: Security Analysis of the Model Context Protocol Specification and Prompt Injection Vulnerabilities in Tool-Integrated LLM Agents*
- **Submitted:** **24 Jan 2026** (v1 only)
- **Authors:** Narek Maloyan, Dmitry Namiot
- **Summary:** Claims the first rigorous security analysis of MCP's architectural design, identifying three protocol-level vulnerabilities: (1) absence of capability attestation (servers can claim arbitrary permissions), (2) bidirectional sampling without origin authentication (server-side prompt injection), (3) implicit trust propagation in multi-server configurations. Implements MCPBench and proposes MCPSec, a backward-compatible extension.
- **KEY QUANTITATIVE FINDINGS:** 847 attack scenarios across 5 MCP server implementations. MCP's architectural choices **amplify ASR by 23–41%** vs equivalent non-MCP integrations. MCPSec reduces ASR **52.8% → 12.4%** with **8.3 ms median latency overhead** per message.
- **⚠️ CAVEAT:** The PDF/TeX is only **13 KB** — an extremely short preprint with no peer review, and it is the sole source for these numbers. Treat the 23–41% and 52.8%→12.4% figures as un-replicated.
- **Verification:** VERIFIED via arxiv.org/abs/2601.17549 fetch (existence + abstract confirmed; claims NOT independently replicated).

### A5. MCP-SandboxScan
- **arXiv:** 2601.01241 — https://arxiv.org/abs/2601.01241
- **Exact title:** *MCP-SandboxScan: WASM-based Secure Execution and Runtime Analysis for MCP Tools*
- **Submitted:** v1 **3 Jan 2026**; v2 **22 Jun 2026**
- **Authors:** Zhuoran Tan, Run Hao, Jeremy Singer, Yutian Tang, Christos Anagnostopoulos
- **Summary:** Presents SandScope, an MCP-aware audit framework combining runtime witness detection with semantic tool profiling. Executes portable tools under WASI or drives unmodified MCP servers over stdio, extracts LLM-visible sinks from tool results and prompt/message fields, and reports auditable source-to-sink witnesses from environment/file/tool-input sources. Targets confused-deputy risks in the MCP tool supply chain.
- **KEY QUANTITATIVE FINDINGS:** 100-repository MCP corpus. Shallow dynamic scans completed for **35 repositories**; semantic profiling recovered metadata for **1,127 tools across 71 repositories**, including **886 tools with security-sensitive declared capabilities**. Schema-guided re-execution pass re-executed **33** repos and observed source-to-sink witnesses in **12**.
- **Verification:** VERIFIED via arxiv.org/abs/2601.01241 fetch.

### A6. EchoLeak / CVE-2025-32711 — **ID CONFIRMED EXACTLY AS GIVEN**
- **arXiv:** 2509.10540 — https://arxiv.org/abs/2509.10540
- **Exact title:** *EchoLeak: The First Real-World Zero-Click Prompt Injection Exploit in a Production LLM System*
- **Submitted:** **6 Sep 2025** (v1 only). **Published at AAAI Fall Symposium Series 2025** (8 pages content + 1 page refs, 2 figures).
- **Authors:** Pavan Reddy, Aditya Sanjay Gujral
- **Summary:** In-depth case study of EchoLeak (CVE-2025-32711), a zero-click prompt injection in Microsoft 365 Copilot enabling remote, unauthenticated data exfiltration via a single crafted email. Chains four bypasses: evading Microsoft's XPIA classifier, circumventing link redaction with reference-style Markdown, exploiting auto-fetched images, and abusing a Teams proxy permitted by the CSP. Derives engineering mitigations.
- **KEY QUANTITATIVE FINDINGS:** **Qualitative case study — no ASR percentage is reported in the abstract.** The quantitative claim is the number of chained bypasses (4) and the zero-click, unauthenticated, single-email exfiltration property. Anyone citing a specific ASR for EchoLeak is inventing it.
- **Verification:** VERIFIED via arxiv.org/abs/2509.10540 fetch (exact title, ID and date all match the request).

### A7. Tool poisoning / rug pull attacks — **PRIMARY 2026 REFERENCE**
- **arXiv:** 2512.06556 — https://arxiv.org/abs/2512.06556
- **Exact title:** *Semantic Attacks on Tool-Augmented LLMs: Securing the Model Context Protocol Against Descriptor-Level Manipulation*
- **Submitted:** v1 **6 Dec 2025**; v2 **21 May 2026**
- **Authors:** Saeid Jamshidi, Arghavan Moradi Dakhel, Kawser Wazed Nafi, Foutse Khomh
- **Summary:** Formalizes **three descriptor-driven attack classes: Tool Poisoning, Shadowing, and Rug Pull** — treating tool descriptors as trusted metadata despite direct injection into the LLM reasoning context. Proposes a layered defense: descriptor integrity verification, pre-context semantic vetting with an auxiliary LLM, and lightweight runtime guardrails, with no retraining. Evaluates GPT-5.3, DeepSeek-V3 and LLaMA-3.5 across eight prompting strategies.
- **KEY QUANTITATIVE FINDINGS:** Baseline **unsafe tool invocations up to 36%** of trials. Full-stack mitigation reduces unsafe invocations to **15%** while raising block rate to **74%**.
- **Verification:** VERIFIED via arxiv.org/abs/2512.06556 fetch.

### A7b. Other essential rug-pull / tool-poisoning papers (verified)

| Paper | arXiv | Date | Key numbers |
|---|---|---|---|
| **MindGuard: Intrinsic Decision Inspection for Securing LLM Agents Against Metadata Poisoning** | [2508.20412](https://arxiv.org/abs/2508.20412) | 28 Aug 2025; v3 15 Jan 2026 | Decision Dependence Graph; **94–99% average precision** detecting poisoned invocations, **95–100% attribution accuracy**, <1 s, **zero extra token cost** |
| **MCPXKIT: The Unified Toolkit for Analyzing Model Context Protocol Security** | [2508.12538](https://arxiv.org/abs/2508.12538) | 18 Aug 2025; v2 25 May 2026 | **31 distinct attack methods** in 4 classifications; accepted **IEEE TDSC** (DOI 10.1109/TDSC.2026.3695553) |
| **Unicode TAG-Block Concealment of Tool-Metadata Payloads in the MCP** | [2607.05744](https://arxiv.org/abs/2607.05744) | **7 Jul 2026** | **8/8** techniques deliver payload to model context; **4/8** evade string-matching sanitizer; **1/8** (TAG-block) invisible in human approval view; **MCP forces re-approval 0/8** even under TOCTOU rug-pull; **32/32** cross-library outcome cells agree; sanitizer flags **0/25** benign descriptions |
| **Parasites in the Toolchain: A Large-Scale Analysis of Attacks on the MCP Ecosystem** | [2509.06572](https://arxiv.org/abs/2509.06572) | 8 Sep 2025; v5 1 May 2026 | Parasitic Toolchain Attacks / MCP-UPD; **12,230 tools across 1,360 servers**. **Accepted IEEE S&P 2026** |
| **Model Context Protocol (MCP) at First Glance** | [2506.13538](https://arxiv.org/abs/2506.13538) | 16 Jun 2025; v5 13 Apr 2026 | 1,899 open-source MCP servers; **7.2%** general vulnerabilities; **5.5% exhibit MCP-specific tool poisoning**; 8 distinct vulnerability classes (only 3 overlap traditional software) |
| **Rethinking MCP Security: A Large-Scale Study of Runtime MCP Servers and Security Scanner Reliability** | [2607.11086](https://arxiv.org/abs/2607.11086) | **13 Jul 2026** | MCPZoo: **64,611 unique MCP servers** (113,927 total), **>37,288** supporting dynamic analysis. Scanners report **96.89%** of servers risky, but **<50% of sampled alerts are true positives** — scanner signals unreliable |
| **Same Name, Different Server: A Security Census of Silent Drift in the MCP Ecosystem** | [2609.14119](https://arxiv.org/abs/2609.14119) | **12 Sep 2026** | Full registry harvest: **21,643 servers, 72,606 version records** (Aug 2026 snapshot); source fetched for **14,353 servers**; 8-class threat catalogue validated against **414 hand-labeled findings** |
| **Measuring and Exploiting Implicit Trust in LLM Tool-Calling Pipelines** | [2609.18217](https://arxiv.org/abs/2609.18217) | **16 Sep 2026** | Cross-channel fragmentation: models with **0% compliance** on single-channel injection exfiltrate at **up to 100%** under two-channel fragmentation (GPT-4o, Llama 70B, Composer 2, Haiku 4.5). 12 frontier models, 15,000+ trials; **all 7 third-party MCP security tools failed** to detect fragmented payloads |
| **MCP-ITP: An Automated Framework for Implicit Tool Poisoning in MCP** | [2601.07395](https://arxiv.org/abs/2601.07395) | 12 Jan 2026 | Automated implicit tool poisoning |
| **Systematization of Knowledge: Security and Safety in the Model Context Protocol Ecosystem** | [2512.08290](https://arxiv.org/abs/2512.08290) | 9 Dec 2025 | SoK |
| **MCPXKIT-adjacent: SMCP: Secure Model Context Protocol** | [2602.01129](https://arxiv.org/abs/2602.01129) | 1 Feb 2026 | Secure MCP design |
| **Prompt Injection Attacks on Agentic Coding Assistants** | [2601.17548](https://arxiv.org/abs/2601.17548) | 24 Jan 2026 | Skills, tools and protocol ecosystems |
| **FlowGuard: From Signals to Evidence for MCP Security Detection** | [2607.14754](https://arxiv.org/abs/2607.14754) | 16 Jul 2026 | 1,880 executable MCP cases; F1 **0.879** (command injection) / **0.942** (file system access); latency reduced up to **2.23×**; 523 findings across 326 real servers |
| **No-Box Vulnerability Analysis: Description-only Detection of IPI Vulnerabilities in MCP Servers** | [2609.10854](https://arxiv.org/abs/2609.10854) | 9 Sep 2026 (v2 16 Sep) | 20 MCP servers / 177 tools; 95 confirmed vulnerable; MCPSEC predicted **94 (98.9% recall)** verified vulnerabilities vs LLM baseline **80 (84.2%)** |
| **AutoMalTool: Automatic Red Teaming LLM-based Agents with Model Context Protocol Tools** | [2509.21011](https://arxiv.org/abs/2509.21011) | 25 Sep 2025 | Automated malicious MCP tool generation |
| **Quantifying Conversation Drift in MCP via Latent Polytope (SecMCP)** | [2508.06418](https://arxiv.org/abs/2508.06418) | 8 Aug 2025 | AUROC **>0.915** on Llama3/Vicuna/Mistral |
| **TrustShiftProbe: Characterizing, Benchmarking, and Defending Staged Trust Attacks on MCP Servers** | [2608.23763](https://arxiv.org/abs/2608.23763) | 24 Aug 2026 | Staged trust attacks on MCP |
| **AEGIS: Preventing Cross-Domain Resource Abuse in MCP** | [2608.20481](https://arxiv.org/abs/2608.20481) | 20 Aug 2026 | Cross-domain resource abuse |
| **Exposed by Design: A Dynamic Security Assessment of Internet-Facing MCP Servers at Scale** | [2608.00150](https://arxiv.org/abs/2608.00150) | 31 Jul 2026 | Internet-facing MCP servers |
| **Characterizing Network Centralization and Observability in the Remote MCP Ecosystem** | [2609.19100](https://arxiv.org/abs/2609.19100) | 16 Sep 2026 | 179 remote endpoints; HHI **0.736** (highly concentrated); **95%** of commercial PaaS-hosted servers enforce OAuth 2.1 + PKCE |
| **When Agents Look Like Beacons: NIDS Evasion by MCP Traffic** | [2609.19091](https://arxiv.org/abs/2609.19091) | 16 Sep 2026 | MCP traffic yields **0.0 behavioral beacon score**, near-zero IDS alerts; evades Suricata + RITA |
| **We Urgently Need Privilege Management in MCP** | [2507.06250](https://arxiv.org/abs/2507.06250) | 5 Jul 2025 | **2,562 real-world MCP applications** across 23 categories; network APIs affect 1,438 servers, system 1,237 |
| **ChainWatch: A Kill Chain-Aligned Sequential Detection Framework** | [2607.19432](https://arxiv.org/abs/2607.19432) | 20 Jul 2026 | HMM-based multi-step MCP attack detection |
| **MCP-AI: Protocol-Driven Intelligence Framework (healthcare)** | [2512.05365](https://arxiv.org/abs/2512.05365) | 5 Dec 2025 | MCP memory objects in clinical reasoning |

---

## B. NEWER 2026 PAPERS (arXiv IDs 2601–2609)

The 2026 corpus on this topic is large and fast-moving. Below are the genuinely-2026 items, grouped by ID month. **All verified to resolve.**

### 2601 (January 2026)
- **2601.01241** — MCP-SandboxScan (3 Jan) — see A5
- **2601.07395** — MCP-ITP: An Automated Framework for Implicit Tool Poisoning in MCP (12 Jan)
- **2601.13112** — CODE: A Contradiction-Based Deliberation Extension Framework for Overthinking Attacks on RAG (19 Jan)
- **2601.17548** — Prompt Injection Attacks on Agentic Coding Assistants: A Systematic Analysis of Vulnerabilities in Skills, Tools, and Protocol Ecosystems (24 Jan)
- **2601.17549** — Breaking the Protocol (24 Jan) — see A4
- **2601.07072** — Overcoming the Retrieval Barrier: Indirect Prompt Injection in the Wild for LLM Systems (11 Jan) — trigger fragment guarantees retrieval; **near-100% retrieval across 11 benchmarks and 8 embedding models**; as little as **$0.21 per target user query**; one poisoned email coerced GPT-4o into exfiltrating SSH keys with **>80% success** in a multi-agent workflow

### 2602 (February 2026)
- **2602.01129** — SMCP: Secure Model Context Protocol (1 Feb)
- **2602.09319** — Benchmarking Knowledge-Extraction Attack and Defense on RAG (v3 8 Jun 2026)
- **2602.13480** — MELT: A Behavioral Trace Dataset for High-Risk Memecoin Launch Detection (13 Feb)
- **2602.21529** — TMRugPull: A Temporally Sound Multimodal Dataset for Early RugPull Detection (25 Feb)

### 2603 (March 2026)
- **2603.02240** — SuperLocalMemory: Privacy-Preserving Multi-Agent Memory with Bayesian Trust Defense Against Memory Poisoning (17 Feb 2026)
- **2603.09134** — AgenticCyOps: Securing Multi-Agentic AI Integration in Enterprise Cyber Operations (10 Mar) — **reduces exploitable trust boundaries by ≥72%** vs flat MAS; intercepts 3 of 4 representative attack chains within the first two steps
- **2603.11324** — LROO Rug Pull Detector (11 Mar)
- **2603.13830** — Early Rug Pull Warning for BSC Meme Tokens (14 Mar)

### 2604 (April 2026)
- **2604.02623** — Poison Once, Exploit Forever (3 Apr) — see Section C
- **2604.03588** — Rashomon Memory (4 Apr)
- **2604.05719** — Hackers or Hallucinators? LLM-Based Automated Penetration Testing (7 Apr)
- **2604.09747** — ADAM: A Systematic Data Extraction Attack on Agent Memory via Adaptive Querying (10 Apr) — **up to 100% ASR** extracting memory contents
- **2604.23711** — Spore: Efficient and Training-Free Privacy Extraction Attack on LLMs (26 Apr)
- **2604.27707** — Contextual Agentic Memory is a Memo, Not True Memory (30 Apr; v2 5 Aug 2026) — position paper arguing lookup ≠ memory, with a provable generalization ceiling and structural vulnerability to persistent memory poisoning

### 2605 (May 2026)
- **2605.01970** — Trojan Hippo: Weaponizing Agent Memory for Data Exfiltration (3 May; v3 15 May) — **85–100% ASR** against current frontier OpenAI/Google models; planted memories activate even after **100 benign sessions**; defenses reduce ASR to **0–5%** but at widely varying utility cost
- **2605.03228** — MAGE: Safeguarding LLM Agents against Long-Horizon Threats via Shadow Memory (4 May)
- **2605.09033** — ShadowMerge (9 May; v3 15 May) — see Section C
- **2605.09330** — The Trap of Trajectory: Spurious Correlations in Agentic Memory (10 May)
- **2605.09863** — Nautilus Compass: Black-box Persona Drift Detection (11 May) — ROC AUC **0.83**; cross-vendor memory comparison incl. Mem0, Letta, Cognee, Zep, MemOS
- **2605.14421** — MemLineage: Lineage-Guided Enforcement for LLM Agent Memory (14 May) — **only configuration driving all three memory-poisoning columns to zero ASR**; sub-millisecond overhead; AgentDojo strict ASR reduced to zero
- **2605.15338** — Hidden in Memory: Sleeper Memory Poisoning in LLM Agents (14 May) — **99.8% (GPT-5.5) / 95% (Kimi-K2.6)** poisoned memory insertion; **60–89%** attacker-intended agentic actions among successful retrievals
- **2605.16233** — FORGE: Self-Evolving Agent Memory With No Weight Updates via Population Broadcast (15 May)
- **2605.17830** — Remembering More, Risking More: Longitudinal Safety Risks in Memory-Equipped LLM Agents (18 May)
- **2605.18930** — OEP: Poisoning Self-Evolving LLM Agents via Locally Correct but Non-Transferable Experiences (18 May)
- **2605.22842** — The Misattribution Gap: When Memory Poisoning Looks Like Model Failure (12 May) — **64 documented failures**; four safety classifiers produced **zero detections across 510 checkpoints**; agents cited the injected document as normative authority in **59 of 65** valid cases; Counterfactual Composition Testing identifies the causal entry with **87.5% accuracy and zero false positives**
- **2605.26154** — MemMorph: Tool Hijacking in LLM Agents via Memory Poisoning (24 May) — **up to 85.9% ASR with only three injected records**, beating the strongest baseline by up to **25%**; 3 benchmarks, 10 agent backbones, 3 memory implementations
- **2605.29960** — Hijacking Agent Memory: Stealthy Trojan Attacks Through Conversational Interaction (28 May) — MemPoison: **ASR up to 0.95**
- **2605.30604** — An Organization-Scoped LLM Agent Runtime Architecture for Regulated Cybersecurity Operations (28 May)

### 2606 (June 2026)
- **2606.01138** — memorywire: A Vendor-Neutral Wire Format for Agent Memory Operations (31 May 2026; v4 12 Aug) — adapters for **sqlite-vec, mem0, Letta, Cognee, pgvector**; RRF holds recall@5 = 1.000 under 1-of-N rank-0 injection sweep where **max fusion collapses to 0.500 with 80% leak**
- **2606.04329** — From Untrusted Input to Trusted Memory: A Systematic Study of Memory Poisoning Attacks in LLM Agents (3 Jun; v2 18 Jun) — **four memory write channels, nine structural vulnerabilities, six attack classes**; introduces MPBench
- **2606.05743** — Membrane: A Self-Evolving Contrastive Safety Memory for LLM Agent Defense (4 Jun; v2 5 Sep) — highest F1 on all six jailbreak attacks; benign refusal on AgentHarm **7–14%** vs **28–85%** for prior guards; **87–88% F1** under cross-attack transfer
- **2606.06337** — TokenMizer (4 Jun)
- **2606.10742** — MemVenom: Triggered Poisoning of Multimodal Memories in Web Agents (9 Jun) — **up to 99.15% ASR on GPT-5-family web agents**
- **2606.12703** — SMSR: Certified Defence Against Runtime Memory Poisoning in Persistent LLM Agent Systems (10 Jun) — end-to-end query-only attack reduced from **65.3% to 5.3%** (n=150); clean-query utility **90% / 85%**
- **2606.12797** — The Containment Gap: How Deployed Agentic AI Frameworks Fail Public-Facing Safety Requirements (11 Jun)
- **2606.15899** — SkillVetBench (11 Jun)
- **2606.18356** — SafeClawBench (18 Jun)
- **2606.22030** — When Does Belief-Based Agent Memory Help? (v2)
- **2606.24322** — Securing LLM-Agent Long-Term Memory Against Poisoning: Non-Malleable, Origin-Bound Authority with Machine-Checked Guarantees (23 Jun) — proves a **machine-checked separation theorem**: no content- or lineage-based defense is sound under laundering; write-time origin binding is necessary
- **2606.24402** — Poisoned Playbooks: Demystifying Knowledge Poisoning Effects on AI Security Agents (23 Jun)
- **2606.26793** — MIRROR: Novelty-Constrained Memory-Guided MCTS Red-Teaming for Agentic RAG (25 Jun) — **76% ASR image poisoning** vs 52% baseline; **97% ASR orchestrator attacks**; ART-SafeBench 41,815 records
- **2606.28270** — Agent-Native Immune System (v1)
- **2606.28666** — Why Trust Your Agent? TRiSM-Guided Agentic Workflows in Healthcare (27 Jun) — RAG poisoning ASR **31% → 10%**; data-field injection **42% → 25%**; network injection eliminated; accuracy **72.5% → 86.5%**
- **2606.29073** — From Tool Connection to Execution Control: Benchmarking Security Invariants in MCP-Style Agent Runtimes (27 Jun) — 10 benchmark cases: naive baseline permits all, mitigation baseline permits 6/10, HCP blocks **all 10**
- **2606.29279** — Manufactured Confidence: How Memory Consolidation Turns Hearsay into Confident Facts (28 Jun) — names **mem0 and LangMem** explicitly; no attacker needed; passive "unverified" tags are ignored; a "do not trust this" instruction escalates even correct memory
- **2606.30566** — Forensic Trajectory Signatures for Agent Memory Poisoning Detection (v2) — AUC **0.9563** (simple rule) → **0.9904** (Random Forest, BCa 95% CI [0.987, 0.993]); **AUC = 1.000 on 6/9 splits**. ⚠️ **v2 preregistered follow-up (N=4,360, 13 models) reveals the signature yields 100% false positives conditional on recall_before_send=1** — the detector does not survive replication

### 2607 (July 2026)
- **2607.00422** — KidnapRAG: A Black-Box Attack for Hijacking Reasoning in Agentic RAG (1 Jul; v2 28 Aug) — accepted **EMNLP 2026 Main**
- **2607.05189** — **When Claws Remember but Do Not Tell: Stealthy Memory Injection in Persistent Personal Agents** (6 Jul) — the **MemGhost** paper. WhisperBench 108 cases; **87.5% end-to-end success on OpenClaw with GPT-5.4**, **71.4% on Claude Code SDK with Sonnet 4.6**; transfers to NanoClaw / Hermes Agent and to **filesystem and vector-based Mem0** backends; survives input-level, model-level and system-level defenses
- **2607.05743** — The Balkanization of Execution-Security Research for AI Coding Agents (7 Jul)
- **2607.05744** — Unicode TAG-Block Concealment (7 Jul) — see A7b
- **2607.06595** — **When Agents Remember Too Much: Memory Poisoning Attacks on LLM Agents** (6 Jul) — the **GhostWriter** paper. See Section C
- **2607.07461** — Mitigating Taint-Style Vulnerabilities in MCP Servers via Security-Aware Tool Descriptions (8 Jul) — SPELLSMITH
- **2607.11086** — Rethinking MCP Security / MCPZoo (13 Jul) — see A7b
- **2607.12406** — Isolation as a First-Class Principle for LLM-Agent System Safety (v2)
- **2607.14754** — FlowGuard (16 Jul)
- **2607.14798** — Ground-Side Mission Plan Compilation (16 Jul)
- **2607.15657** — Do Agents Dream of False Memories? Black-box Visual Attacks on Long-term Memory in Multimodal AI Agents (v2) — **Lucid**; image-only threat model
- **2607.17535** — Salience Induction against Multi-Hop RAG Agents (20 Jul) — **83.3% ASR** at 30% edit budget; strongest baseline defense leaves **75.7% post-defense ASR**; Salience Normalization cuts to **15.3%** (standard) / **23.6%** (adaptive)
- **2607.19292** — The safety failures we are not instrumenting (v1)
- **2607.19430** — ChannelGuard: Safe Models Do Not Compose into Safe Multi-Agent Systems (v2 10 Aug) — **Tool-output gate blocks Tool Poisoning 30/30** at the application layer; undefended pipeline's apparent safety came almost entirely from the **cloud provider's server-side filter (54 of 60 blocks on Azure GPT-5)**; prompt injection ASR **0.333 → 0.167**
- **2607.19432** — ChainWatch (20 Jul)
- **2607.24006** — Agentic Cloud Decoys (27 Jul)
- **2607.24625** — APPA: Recoverable Information-Flow Control for Real-World LLM Agents (v2) — **zero observed attacks across 1,320 guarded episodes**; utility 64.2–91% across 6,600 episodes
- **2607.25297** — Hybrid Analysis for Secure MCP Tool Use in LLM Agents (28 Jul)
- **2607.27080** — **MemSecBench: Tracking Agent Memory Poisoning from Persistence to Consequence and Repair** (Jul 2026) — **310 cases from 48 realistic contexts**; controlled Write–Execute–Forget protocol; seven lifecycle checkpoints

### 2608 (August 2026)
- **2608.00150** — Exposed by Design (31 Jul 2026)
- **2608.00718** — Adversarial Attacks in Multi-Agent LLM Pipelines (v1)
- **2608.00997** — Registry Descriptions Go Stale Unevenly: An 89-Day Measurement of MCP Drift (v2 2 Aug)
- **2608.01609** — From Viral to Void: Rug Pull Identification (3 Aug)
- **2608.01637** — Salami Attack: Stealthy Collusive Memory Poisoning against OpenClaw (v1)
- **2608.02018** — Invisible Ink Threats (v2 6 Aug)
- **2608.02657** — Your Agentic LLMs Secretly Encode Indirect Prompt-Injection Exposure in Hidden States (v2 24 Aug) — linear probes **0.90+ AUROC** on unseen attacks; probe-gated defense cuts AgentDojo ASR **34.6% → 0%** on Qwen3.5-27B
- **2608.02843** — MutMem: Cryptographically Authorized Mutation in Persistent Agent Memory (v1)
- **2608.03844** — **MAFIA: Query-Only Memory Attacks via Probing and Factual Injection against Audited LLM Agents** (4 Aug) — **up to 90.7% ASR** while suppressing audit detection from a peak of **83.3% to at most 7.4%**
- **2608.04366** — Combating Knowledge Corruption in Agent Systems: Byzantine-Tolerant Secure Collaborative RAG (5 Aug) — **ACM Web Conference 2026**, pp. 2661–2672
- **2608.04741** — LoginTrap (5 Aug) — **86% average end-to-end ASR** across LLM backbones
- **2608.05430** — Robust Context-Aware Detection of Malicious Instructions in Text (5 Aug)
- **2608.05715** — Hijacking Robots with a Piece of Paper: Physical Prompt Injection in VLM-Controlled Robots (6 Aug) — 5,670 trials; ASR **27.0% / 29.4% / 5.0%**; **99.9% conscious acknowledgment**; text masking 100% effective
- **2608.06477** — StepJack: Benchmarking Computer-Use Agent Safety Against Multi-Step IPI (6 Aug) — 480 examples; ASR **41.7% → 72.9%** (GPT-5.4-mini) with decomposition depth
- **2608.07622** — Controlled Memory Interference in Continual LLM Agents (7 Aug)
- **2608.08100** — Defending Retrieval-Augmented Intrusion Detection Against Knowledge Poisoning and Prompt Injection (8 Aug) — recovery **R=1.0 at 1% poisoning → R=0.57 at 30%**; multi-doc retrieval limits label-flip to **0.6–2.4%** vs **35–55%** single-doc
- **2608.08795** — Toward Metacognitive One-Shot Indirect Prompt Injection (9 Aug) — SAVOR; leads strongest prior attack by **2.5–11.8 points**; **+23.1 points** ASB, **+28.6 points** OpenClaw-IPI
- **2608.08939** — Not an A11y: Android Accessibility and IPI (9 Aug) — MobileRun ASR **0.822** with Gemma4:31B
- **2608.10760** — A Gateway Architecture for Enterprise MCP Authentication (11 Aug)
- **2608.13574** — Agentao: A Policy-Governed Runtime Harness (v2 28 Aug)
- **2608.17153** — Towards Safer RAG: Only Agents Capable of System 2 Thinking may Access Untrusted Documents (v2 16 Sep) — DeepSeek-V4-Flash Cordon Rate **0.211 → 0.107**, Leakage Rate **0.235 → 0.140**, but overall attack success **rises 0.233 → 0.298**
- **2608.17665** — GraphWake: Group Polarization via Memory-Mediated Polarization Cascade (18 Aug)
- **2608.18260** — Redakto (18 Aug)
- **2608.18351** — Task-Conditioned Least-Privilege Learning (18 Aug)
- **2608.18740** — A Multi-Agent Platform for Automated Enterprise Analytics (19 Aug)
- **2608.20481** — AEGIS: Preventing Cross-Domain Resource Abuse in MCP (20 Aug)
- **2608.20631** — Weighted Memory Tree (21 Aug) — accuracy **+9.97 pp**, prompt tokens **−32.8%**
- **2608.20756** — Vis-Poison: Poisoning Visual Knowledge in Multimodal RAG (21 Aug; v2 28 Aug) — **40.16%–65.40% end-to-end ASR** against 30k-entry multimodal KBs, black-box; >60% average against MLLMs that answer correctly from parametric knowledge. **Findings of EMNLP 2026**
- **2608.21095** — Trustworthy RAG: An Evaluation Agent for Detecting Misinformation and Knowledge Poisoning (21 Aug) — **91% accuracy, 100% precision, 100% recall on instruction injection** on TruthfulQA with Llama 3.3 70B; ROC-AUC 0.73–0.81 across three LLMs; in-place entity swaps remain hard to detect
- **2608.21230** — **Utility Under Attack: Agent Memory Poisoning and the Limits of Content Screening and Provenance Ranking** (21 Aug) — see Section C
- **2608.22061** — MEMORY Wins All: Indirect Bias Injection Attacks via Social Media Feeds (22 Aug) — IBIA; **91.2% average adversary-aligned response rate** across four downstream tasks, incl. **86.6% on GPT-5.5**; memory boundary defense reduces to **80.6%**
- **2608.23471** — **InjecMEM: Memory Injection Attack on LLM Agent Memory Systems** (24 Aug) — **accepted COLM 2026**. Single-interaction injection, no read/edit access to the memory store
- **2608.23763** — TrustShiftProbe (24 Aug)
- **2608.23858** — Beyond the Mandate: Security Analysis of Agent Payments Protocol (AP2) (24 Aug)
- **2608.23992** — Hybrid Semantic Tool Discovery for Enterprise MCP Gateway (25 Aug)
- **2608.24022** — What Guides the Agent? Attnlocate (25 Aug) — mean IoU **0.743**, AUROC **0.956**, TPR **0.934** at FPR 0.067
- **2608.24957** — ToolMinimize: Auditing and Rewriting LLM Agent Tool Calls (25 Aug) — **81–88%** of tool calls include unnecessary privacy-sensitive data; privacy cost reduced **81.2–92.0%**; **79.0%** on 25 unannotated MCP schemas; median latency 1.77 ms
- **2608.28794** — Breaking Darknet CAPTCHAs with general purpose LLM (28 Aug)
- **2608.30177** — Understanding Stage-Wise Utility-Risk Trade-offs in LLM Agent Memory (31 Aug) — MemGauge, 11 LLMs

### 2609 (September 2026) — the newest tier
- **2609.00267** — Delegation Without Trust (31 Aug 2026)
- **2609.00523** — Transferable End-to-End Optimization for Indirect Long-Term Memory Poisoning in LLM Agents (1 Sep) — **PipePoison**; improves attack utilization rate by **19.1 percentage points**; **+16 points** over strongest baseline on unseen victim configurations; remains effective under **eight** representative defenses
- **2609.01693** — Public-Sharing Labels and Verbatim Field Egress in an MCP-to-A2A Agent Configuration (1 Sep)
- **2609.01836** — Agent Memory Is a Surface for Endogenous Authorization Laundering (1 Sep) — writers create false authority for up to **50.2%** of unauthorized requests; once false authority exists, executors act on it in **98.6%** of trials
- **2609.02265** — CAPTURE: Disentangling Preference Drift from Memory Poisoning (2 Sep) — limits fixed-policy poisoning success to **11.5%** while accepting **83.5%** of genuine preference updates; adaptive attacker raises success to **24.7%**
- **2609.02690** — ACLE-MCP: Attested Capability Leases for Execution-Time Trust in Remote LLM Tool Use (2 Sep)
- **2609.08258** — Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems (8 Sep) — **five systems measured; no system enforces revocation by default**; revoked facts outrank their replacements and lead agents to unsafe actions, across nine policy scenarios and nine models
- **2609.08747** — MemSentry: A Framework for Detecting Persistent Memory Poisoning in Agentic AI (8 Sep) — SBERT+LR best at **91.7% accuracy / 0.908 macro-F1**; all four methods detect **100% of external quarantine-class threats**; 1,000 GPT-4-generated scenarios
- **2609.10707** — Architecting the Secure AI-SOC: A Neurosymbolic Framework (9 Sep)
- **2609.10854** — No-Box Vulnerability Analysis (9 Sep; v2 16 Sep)
- **2609.10871** — A2ABreak: Systematic Security Analysis of the A2A Protocol (9 Sep)
- **2609.10892** — DriftNet: A Dual-Head Trajectory Transformer (9 Sep) — trajectory F1 **0.983**; injection-point recovery **98.7%**; hijacked-span IoU **0.979**; zero flags on 218 resisted attacks; 2.9% flags on hard negatives
- **2609.11952** — ChemMat-AgentSafetyBench (v1)
- **2609.13334** — The Agentic Company OS (11 Sep)
- **2609.13889** — **When Malicious Instructions Persist: Persistent Memory Poisoning Attack on Harness-Based Agents** (12 Sep) — **PMPA**; the newest memory-poisoning attack paper found. See Section C
- **2609.14119** — Same Name, Different Server: A Security Census of Silent Drift in the MCP Ecosystem (12 Sep)
- **2609.14721** — A Two-Dimensional Study of the Model Context Protocol: Publication and Adoption (13 Sep) — **802 MCP publications + 33,319 GitHub repos**; growth peaked together in **March 2026**; a second wave accounts for **49.5%** of repos; **57.8%** of publications and **93.7%** of repos use MCP as enabling tech rather than studying/securing it
- **2609.14744** — AcquireBound (13 Sep)
- **2609.14780** — The Stochastic Deputy: Structural Tenant Isolation for Tool-Using LLM Agents (13 Sep) — 373-trial ablation; a validated tenant parameter served **every** out-of-scope attempt (**26 of 26**; 26 of 41 plausible-pretext trials)
- **2609.14987** — ActGuard: Pre-execution Action Auditing against Indirect Prompt Injection (14 Sep)
- **2609.15906** — Authorization Architectures for Tool-Using AI Agents (14 Sep)
- **2609.16098** — Universal Defenses for Tool-Integrated LLM Agents Against Adversarial Attacks (14 Sep) — **0% ASR in many settings** across 7 LLMs (Gemma2-9B, Qwen2-7B, LLaMA3-8B, LLaMA3.1-8B, GPT-3.5, GPT-4, GPT-5)
- **2609.17320** — Emergence World: Adversarial Stress-Testing of Long-Horizon Multi-Agent Systems (15 Sep) — **8 worlds, 10 agents, 16 days, >850,000 LLM calls, ~50 billion tokens**. **No world achieved full resilience.** Agents acted on injected content **up to 46 hours later**. Model-level alignment is **not compositional**
- **2609.17648** — Trust propagation and structural containment in Multi-agent LLM pipelines (15 Sep) — memory poisoning reaches execution in **every undefended trial**; with authorization, **100% JBR but 0% Unsafe Action Rate**; Observer layer cuts hijack false-positive rate **49% → 7%**
- **2609.18217** — Measuring and Exploiting Implicit Trust in LLM Tool-Calling Pipelines (16 Sep)
- **2609.18411** — The Verifiable Action Card (16 Sep) — attack success without VAC **68–100%**; VAC reduces to **0% on every model**, **78% legitimate-task completion**, **0% false-block rate**
- **2609.19091** — When Agents Look Like Beacons: NIDS Evasion by MCP Traffic (16 Sep)
- **2609.19100** — Characterizing Network Centralization and Observability in the Remote MCP Ecosystem (16 Sep)
- **2609.19425** — Closed-World Resolution Against Tool Hallucination in LLM Agents (16 Sep) — 322 genuine hallucinations across ten hosted models; **154 MCP-surface hallucinations**; frontier models that were clean on single-registry surfaces hallucinate on merged MCP namespaces

---

## C. MEMORY-SPECIFIC THREATS (HIGHEST PRIORITY)

### C.0 Foundational prior work
- **AgentPoison: Red-teaming LLM Agents via Poisoning Memory or Knowledge Bases** — **arXiv:2407.12784** — 17 Jul 2024 — https://arxiv.org/abs/2407.12784 — the first backdoor attack targeting generic and RAG-based LLM agents by poisoning long-term memory or the RAG knowledge base. Constrained optimization maps triggered instances to a unique embedding space. No model training or fine-tuning required.
- **Permissive Information-Flow Analysis for Large Language Models** — **arXiv:2410.03055** — 4 Oct 2024; v3 14 Jan 2026 — https://arxiv.org/abs/2410.03055 — permissive label propagation improves over baseline in **>85%** of cases.

### C.1 Direct memory-poisoning attacks

#### GhostWriter / Agentic Memory Sentry — the canonical "When Agents Remember Too Much" paper
- **arXiv:** 2607.06595 — https://arxiv.org/abs/2607.06595
- **Exact title:** *When Agents Remember Too Much: Memory Poisoning Attacks on Large Language Model Agents*
- **Submitted:** **6 Jul 2026** (v1)
- **Authors:** George Torres, Sharad Shrestha, Satyajayant Misra
- **Summary:** Identifies GhostWriter, a two-phase attack (injection → activation) exploiting the memory subsystem of tool-using personal agents. Personal assistant agents converge the conversational and action-planning memory domains and handle sensitive information while reading untrusted sources. Proposes Agentic Memory Sentry (AM-Sentry) with a memory-saving policy and a memory-retrieval screen.
- **KEY QUANTITATIVE FINDINGS:** **~98% injection rate** (near-universal) and **~60% average activation rate** against state-of-the-art agents. AM-Sentry "dramatically reduces" GhostWriter success while preserving utility (no post-defense ASR stated in the abstract).
- **Verification:** VERIFIED via arxiv.org/abs/2607.06595 fetch.

#### ShadowMerge — graph-based memory (Mem0)
- **arXiv:** 2605.09033 — https://arxiv.org/abs/2605.09033
- **Exact title:** *ShadowMerge: A Novel Poisoning Attack on Graph-Based Agent Memory via Relation-Channel Conflicts*
- **Submitted:** v1 **9 May 2026**; v2 14 May 2026; v3 **15 May 2026**
- **Authors:** Yang Luo, Zifeng Kang, Tiantian Ji, Xinran Liu, Yong Liu, Shuyu Li, Lingyun Peng
- **Summary:** Attacks graph-based agent memory via relation-channel conflicts — a poisoned relation shares the same query-activated anchor and canonicalized relation channel as benign evidence but carries a conflicting value. The AIR pipeline converts the conflict into an ordinary interaction that the graph-memory system will extract, merge and retrieve. Evaluated on **Mem0** plus PubMedQA, WebShop and ToolEmu.
- **KEY QUANTITATIVE FINDINGS:** **93.8% average ASR**, improving the best baseline by **50.3 absolute points**; negligible impact on unrelated benign tasks. Representative input-side defenses are **insufficient**. Findings responsibly disclosed to affected graph-memory vendors.
- **Verification:** VERIFIED via arxiv.org/abs/2605.09033 fetch.

#### PMPA — newest memory-poisoning attack (September 2026)
- **arXiv:** 2609.13889 — https://arxiv.org/abs/2609.13889
- **Exact title:** *When Malicious Instructions Persist: Persistent Memory Poisoning Attack on Harness-Based Agents*
- **Submitted:** **12 Sep 2026** (v1)
- **Authors:** Shuhuai Huang, Jingfeng Zhang, Hong Jia
- **Summary:** PMPA embeds malicious instructions into benign external sources and induces the victim agent to write them into persistent memory **without direct access to the agent framework**. Once stored, poisoned memory is retrieved in later sessions, triggering further malicious actions and privacy leakage. Evaluated on **OpenClaw** and **Claude Code** across backbone LLMs, input modalities and trigger scenarios. Code at github.com/hsh754/PMPA.
- **KEY QUANTITATIVE FINDINGS:** Average Injection Success Rate / Cross-session ASR = **73.7% / 55.5% on OpenClaw** and **66.9% / 81.7% on Claude Code**, while preserving benign task performance. A targeted prompt-level defense reduces memory injection in many settings but provides **limited protection once persistent memory has been poisoned**.
- **Verification:** VERIFIED via arXiv API record (ID, title, authors, dates, full abstract).

#### MemGhost — stealthy memory injection in persistent personal agents
- **arXiv:** 2607.05189 — https://arxiv.org/abs/2607.05189
- **Exact title:** *When Claws Remember but Do Not Tell: Stealthy Memory Injection in Persistent Personal Agents*
- **Submitted:** **6 Jul 2026** (v1), 25 pages
- **Authors:** Yechao Zhang, Shiqian Zhao, Jiawen Zhang, Jie Zhang, Gelei Deng, Xiaogeng Liu, Chaowei Xiao, Tianwei Zhang
- **Summary:** Defines **stealth memory injection**: a remote black-box adversary sends a single email payload that must (a) induce the agent to write poisoned memory, (b) stay hidden in the agent's user-facing response, and (c) affect future behavior. Introduces **WhisperBench** (108 cases, five risk categories, fact and preference poisoning, real IMAP/SMTP workflow) and **MemGhost**, a one-shot payload generator trained with SFT + RL against environment and objective proxies.
- **KEY QUANTITATIVE FINDINGS:** On 56 held-out cases: **87.5% end-to-end success on OpenClaw with GPT-5.4**, **71.4% on Claude Code SDK with Sonnet 4.6**. Transfers across personal-agent architectures (**NanoClaw**, **Hermes Agent**) and memory backends (**filesystem and vector-based Mem0**). Remains effective against input-level, model-level and system-level defenses.
- **Verification:** VERIFIED via arxiv.org/abs/2607.05189 fetch.
- **⚠️ NAMING NOTE:** "MemGhost" is the *attack framework name*, not the paper title. A web search result titled "MemGhost Attack Turns Agent Memory Into a Permanent Backdoor" is a **blog post** ([forkast.news](https://forkast.news/memghost-attack-turns-agent-memory-into-a-permanent-backdoor-and-nothing-stops-it/)), not a paper. The primary source is 2607.05189.

#### eTAMP — environment-injected memory poisoning (web agents)
- **arXiv:** 2604.02623 — https://arxiv.org/abs/2604.02623
- **Exact title:** *Poison Once, Exploit Forever: Environment-Injected Memory Poisoning Attacks on Web Agents*
- **Submitted:** v1 **3 Apr 2026**; v2 **7 Apr 2026**
- **Authors:** Wei Zou, Mingwen Dong, Miguel Romero Calvo, Shuaichen Chang, Jiang Guo, Dongkyu Lee, Xing Niu, Xiaofei Ma, Yanjun Qi, Jiarong Jiang
- **Summary:** More realistic threat model than prior memory work — contamination through **environmental observation alone**, with **no direct memory access**. A single contaminated observation (e.g. a manipulated product page) silently poisons memory and activates during future tasks on *different* websites, bypassing permission-based defenses. Introduces **Frustration Exploitation**.
- **KEY QUANTITATIVE FINDINGS:** Up to **32.5% on GPT-5-mini**, **23.4% on GPT-5.2**, **19.5% on GPT-OSS-120B**. ASR increases **up to 8×** when agents struggle with dropped clicks or garbled text. **More capable models are not more secure.**
- **Verification:** VERIFIED via arxiv.org/abs/2604.02623 fetch.

#### Additional memory-poisoning attacks (verified, with numbers)

| Paper | arXiv | Date | Key quantitative findings |
|---|---|---|---|
| **Hidden in Memory: Sleeper Memory Poisoning in LLM Agents** | [2605.15338](https://arxiv.org/abs/2605.15338) | 14 May 2026 (v2 18 May) | Poisoned memories added up to **99.8% on GPT-5.5**, **95% on Kimi-K2.6**; among successful retrievals, attacker-intended agentic actions in **60–89%** of evaluations |
| **MemVenom: Triggered Poisoning of Multimodal Memories in Web Agents** | [2606.10742](https://arxiv.org/abs/2606.10742) | 9 Jun 2026 | **Up to 99.15% ASR on GPT-5-family web agents**; graph-structured external memory poisoned with coordinated text-image evidence; black-box, transferable across architectures and scales |
| **MemMorph: Tool Hijacking in LLM Agents via Memory Poisoning** | [2605.26154](https://arxiv.org/abs/2605.26154) | 24 May 2026 | **Up to 85.9% ASR with only 3 injected records**; outperforms strongest baseline by up to **25%**; 3 benchmarks, 10 backbones, 3 memory implementations; potent under 3 defenses |
| **MAFIA: Query-Only Memory Attacks via Probing and Factual Injection against Audited LLM Agents** | [2608.03844](https://arxiv.org/abs/2608.03844) | 4 Aug 2026 | **Up to 90.7% ASR** while suppressing audit detection from a peak of **83.3% to ≤7.4%** |
| **Hijacking Agent Memory: Stealthy Trojan Attacks (MemPoison)** | [2605.29960](https://arxiv.org/abs/2605.29960) | 28 May 2026 | **ASR up to 0.95**; bypasses selective extraction and rewriting stages; exploits embedding-space anisotropy |
| **InjecMEM: Memory Injection Attack on LLM Agent Memory Systems** | [2608.23471](https://arxiv.org/abs/2608.23471) | 24 Aug 2026 | **Accepted COLM 2026**; single interaction, no read/edit access; retriever-agnostic anchor + adversarially optimized command; survives memory drift without affecting non-target queries |
| **Trojan Hippo: Weaponizing Agent Memory for Data Exfiltration** | [2605.01970](https://arxiv.org/abs/2605.01970) | 3 May 2026 (v3 15 May) | **85–100% ASR** vs current frontier OpenAI/Google models; planted memories activate **even after 100 benign sessions**; 4 memory backends (explicit tool memory, agentic memory, RAG, sliding window); defenses cut ASR to **0–5%** but at widely varying utility cost |
| **OEP: Poisoning Self-Evolving LLM Agents via Locally Correct but Non-Transferable Experiences** | [2605.18930](https://arxiv.org/abs/2605.18930) | 18 May 2026 | Low-privilege black-box attack via **locally correct, semantically plausible** clean experiences — evades safety filters that catch explicit malicious content |
| **MEMORY Wins All: Indirect Bias Injection Attacks via Social Media Feeds** | [2608.22061](https://arxiv.org/abs/2608.22061) | 22 Aug 2026 | **91.2% average adversary-aligned response rate** across four downstream tasks, incl. **86.6% on GPT-5.5**; watermark curation identifies 95.9% of injected comments; boundary defense reduces AAR to **80.6%** |
| **Do Agents Dream of False Memories? Black-box Visual Attacks on Long-term Memory** | [2607.15657](https://arxiv.org/abs/2607.15657) | Jul 2026 (v2) | **Lucid**; strictly image-bounded threat model — no access to target MLLM, retrieval encoder, or text channel; two failure modes (memory poisoning, memory injection) |
| **Spore: Efficient and Training-Free Privacy Extraction Attack on LLMs** | [2604.23711](https://arxiv.org/abs/2604.23711) | 26 Apr 2026 | Targets **LLM agent memory** at inference time; single-query candidate-set extraction; bypasses detection and strong safety alignment |
| **ADAM: A Systematic Data Extraction Attack on Agent Memory via Adaptive Querying** | [2604.09747](https://arxiv.org/abs/2604.09747) | 10 Apr 2026 | **Up to 100% ASR** extracting sensitive memory via entropy-guided queries |
| **MemoryGraft: Persistent Compromise of LLM Agents via Poisoned Experience Retrieval** | [2512.16962](https://arxiv.org/abs/2512.16962) | 18 Dec 2025 | Implants **malicious successful experiences** into long-term memory; exploits the agent's **semantic imitation heuristic**; validated on MetaGPT DataInterpreter + GPT-4o; persistent behavioral drift across sessions |
| **Salami Attack: Stealthy Collusive Memory Poisoning against OpenClaw** | [2608.01637](https://arxiv.org/abs/2608.01637) | Aug 2026 | Collusive, low-and-slow memory poisoning |
| **Bilevel Optimization for Covert Memory Tampering (XAMT)** | [2512.15790](https://arxiv.org/abs/2512.15790) | 15 Dec 2025 | Sub-percent poison rates: **≤1% (MARL), ≤0.1% (RAG)**; SMAC + SafeRAG benchmarks |
| **Zombie Agents: Persistent Control of Self-Evolving LLM Agents via Self-Reinforcing Injections** | [2602.15654](https://arxiv.org/abs/2602.15654) | 17 Feb 2026 (v2) | Self-reinforcing injection loop giving persistent control of self-evolving agents |
| **GraphWake: Group Polarization via Memory-Mediated Polarization Cascade** | [2608.17665](https://arxiv.org/abs/2608.17665) | 18 Aug 2026 | Community-level polarization driven through agent memory as the persistence channel |
| **GraphWake-adjacent: Rashomon Memory** | [2604.03588](https://arxiv.org/abs/2604.03588) | 4 Apr 2026 | Argumentation-driven multi-perspective memory (defensive architecture) |

### C.2 Defenses for long-term agent memory (verified, with effectiveness)

| Defense | arXiv | Date | Effectiveness |
|---|---|---|---|
| **MemSentry** | [2609.08747](https://arxiv.org/abs/2609.08747) | 8 Sep 2026 | SBERT+LR **91.7% accuracy, 0.908 macro-F1**; **100% detection of external quarantine-class threats**; 1,000 GPT-4 scenarios, 70/30 split |
| **MemLineage** | [2605.14421](https://arxiv.org/abs/2605.14421) | 14 May 2026 | **Only harness configuration driving all three memory-poisoning columns to zero ASR**; sub-ms per-operation overhead; AgentDojo strict ASR → **0** |
| **CAPTURE** | [2609.02265](https://arxiv.org/abs/2609.02265) | 2 Sep 2026 | Fixed-policy poisoning success limited to **11.5%**; accepts **83.5%** of genuine preference updates; adaptive attacker raises success to **24.7%** |
| **SMSR (Certified Defence Against Runtime Memory Poisoning)** | [2606.12703](https://arxiv.org/abs/2606.12703) | 10 Jun 2026 | End-to-end query-only attack success **65.3% → 5.3%** (n=150, non-overlapping CIs); clean-query utility **90% / 85%**; HMAC-SHA256 provenance + randomised ablation with verdict-based majority voting |
| **MAGE (Memory As Guardrail Enforcement)** | [2605.03228](https://arxiv.org/abs/2605.03228) | 4 May 2026 | Shadow-stack-inspired safety memory; earliest framework to detect long-horizon threats via agentic memory |
| **Membrane (Contrastive Safety Memory)** | [2606.05743](https://arxiv.org/abs/2606.05743) | 4 Jun 2026 (v2 5 Sep) | Highest F1 on all six modern jailbreak attacks; benign refusal **7–14%** vs **28–85%** for prior guards; **87–88% F1** under cross-attack transfer; stable under memory poisoning. **EMNLP 2026 Main** |
| **Non-Malleable Origin-Bound Authority** | [2606.24322](https://arxiv.org/abs/2606.24322) | 23 Jun 2026 | **Machine-checked separation theorem**: no content- or lineage-based defense is sound under laundering; write-time origin binding is necessary |
| **MemGauge (stage-wise evaluation)** | [2608.30177](https://arxiv.org/abs/2608.30177) | 31 Aug 2026 | 11 LLMs; three distinct utility-risk profiles across write / manage / retrieve stages |
| **MemSecBench** | [2607.27080](https://arxiv.org/abs/2607.27080) | Jul 2026 | **310 cases / 48 contexts**; Write–Execute–Forget protocol; seven lifecycle checkpoints |
| **Weighted Memory Tree** | [2608.20631](https://arxiv.org/abs/2608.20631) | 21 Aug 2026 | Accuracy **+9.97 pp**, prompt tokens **−32.8%**; limits persistence and propagation of unreliable information |
| **SuperLocalMemory** | [2603.02240](https://arxiv.org/abs/2603.02240) | 17 Feb 2026 | Defends **OWASP ASI06** memory poisoning; 10.6 ms median search latency; **72% trust degradation for sleeper attacks**; trust separation gap **0.90**; MCP integration |
| **memorywire** | [2606.01138](https://arxiv.org/abs/2606.01138) | 31 May 2026 (v4 12 Aug) | Vendor-neutral wire format with **mem0, Letta, Cognee** adapters + HITL governance channel; provenance field is the strongest lever for recovering a poisoned store |
| **Forensic Trajectory Signatures** | [2606.30566](https://arxiv.org/abs/2606.30566) | Jun 2026 (v2) | AUC **0.9904** — **but v2 replication shows 100% false positives conditional on recall_before_send=1**. Do not cite v1 numbers |
| **MemAudit: Post-hoc Auditing of Poisoned Agent Memory via Causal Attribution and Structural Anomaly Detection** | [2605.23723](https://arxiv.org/abs/2605.23723) | 22 May 2026 | Causal attribution + structural anomaly detection for post-hoc memory forensics |

### C.3 Memory-governance failure modes (no attacker required)

- **arXiv:2609.08258** — *Revoked but Still Authoritative: An Empirical Study of Revocation Enforcement in Agent-Memory Systems* (8 Sep 2026) — **Five agent-memory systems measured; NO system enforces revocation by default.** The revoked fact is returned wherever the revocation label is visible to the retrieval layer, **outranks its replacement**, and leads agents to the unsafe action — across **nine policy scenarios and nine models**, scored under six defense conditions. A guard that withholds revoked/conflicting records was built as a mitigation. Code: github.com/VulcanLab/Memory-Rebirth-Attack
- **arXiv:2609.01836** — *Agent Memory Is a Surface for Endogenous Authorization Laundering* (1 Sep 2026) — Introduces **EAL-Bench**. Under incremental memory updates, writers create false authority for up to **50.2%** of unauthorized requests; once false authority is present, executors act on it in **98.6%** of trials. Evaluated across five LLMs as writers, two as executors, in procurement/cybersecurity/finance. Both safeguards reduce laundering but reject more legitimate actions — a real safety-utility tradeoff.
- **arXiv:2606.29279** — *Manufactured Confidence: How Memory Consolidation Turns Hearsay into Confident Facts* (28 Jun 2026) — Names **mem0 and LangMem** explicitly as memory products that rewrite conversation into stored "facts". A hedged remark becomes a confident dated assertion. **No attacker is needed.** The agent responds to *phrasing confidence*, not source: attributed, unattributed and even forged "system of record" claims all grant alike. A passive "unverified" tag is ignored; an active "do not trust this" instruction escalates even correct memory.
- **arXiv:2608.21230** — *Utility Under Attack: Agent Memory Poisoning and the Limits of Content Screening and Provenance Ranking* (21 Aug 2026) — **The single most important negative result in this survey.** Poisoning just **1.2% of a LongMemEval corpus reduces accuracy from 0.850 to 0.300**. A four-stage write-time screening pipeline reaching **0.832 recall on indirect prompt injection** (while flagging only 1.5% of trigger-word-laden benign text) **rejects 0 of 360 poisoned memories**. Provenance-weighted retrieval: shipped weight is **statistically indistinguishable from no defense (p=0.80)**; a stronger weight recovers utility only by excluding untrusted content (accuracy **0.3167 → 0.7000** in a mixed-provenance corpus); when the answer-bearing evidence itself is untrusted, evidence recall falls to **zero** and accuracy to **0.0417**. Conclusion: **no usable setting for the additive provenance term** under the measured similarity regime — argues for bounded occupancy constraints at retrieval instead.
- **arXiv:2605.22842** — *The Misattribution Gap: When Memory Poisoning Looks Like Model Failure in Agentic AI Systems* (12 May 2026) — Formalizes **Semantic Norm Drift** as a third path to agent misconduct, distinct from emergent misalignment and collusion. Policy-formatted documents enter a shared vector store via normal uploads and reappear as trusted system context after a **Trust Laundering Chain**. Across **64 documented failures**, attribution systems consistently blamed the model; **four safety classifiers, including one trained on memory poisoning, produced zero detections across 510 checkpoints**; agents explicitly cited the injected document as normative authority in **59 of 65** valid cases. Attack needs **no trigger, no model access, no repeated interaction**, achieves full effect within five sessions, and persists indefinitely. Counterfactual Composition Testing identifies the causal entry with **87.5% accuracy and zero false positives**.
- **arXiv:2605.17830** — *Remembering More, Risking More: Longitudinal Safety Risks in Memory-Equipped LLM Agents* (18 May 2026) — Introduces **temporal memory contamination**. Memory-enabled agents consistently exceed the NullMemory baseline, and memory-induced violation rates show a robust **upward trend with exposure length**, across eight memory architectures and Claw-like agents including **OpenClaw**. Effect driven by accumulated *content*, not encounter order. Memory-induced risk is detectable from retrieval state *before* generation.
- **arXiv:2606.04329** — *From Untrusted Input to Trusted Memory: A Systematic Study of Memory Poisoning Attacks in LLM Agents* (3 Jun 2026, v2 18 Jun) — Identifies **four memory write channels** and **nine structural vulnerabilities** in model capabilities, system prompt design, and agent architecture; develops a taxonomy of **six classes** of memory-poisoning attack; introduces **MPBench**. Key finding: **agents designed to write and retrieve memory more aggressively are more exploitable**, and **existing prompt-injection defenses fail to cover memory poisoning**.
- **arXiv:2604.27707** — *Contextual Agentic Memory is a Memo, Not True Memory* (30 Apr 2026, v2 5 Aug) — Position paper: vector stores, RAG, scratchpads and context-window management implement **lookup, not memory** — a category error with a **provable generalization ceiling** on compositionally novel tasks that no increase in context size or retrieval quality can overcome, plus **structural vulnerability to persistent memory poisoning**. Draws on Complementary Learning Systems theory.
- **arXiv:2605.09330** / **2605.22842** / **2607.19292** — related governance and instrumentation gaps.

### C.4 Memory servers and frameworks — coverage assessment

**Named products with direct paper coverage:**
- **Mem0** — attacked directly in **ShadowMerge** ([2605.09033](https://arxiv.org/abs/2605.09033), 93.8% ASR); used as a transfer target in **MemGhost** ([2607.05189](https://arxiv.org/abs/2607.05189)); adapter in **memorywire** ([2606.01138](https://arxiv.org/abs/2606.01138)); named as a memory product in **Manufactured Confidence** ([2606.29279](https://arxiv.org/abs/2606.29279)); benchmarked in **Nautilus Compass** ([2605.09863](https://arxiv.org/abs/2605.09863)).
- **Letta / MemGPT** — adapter in **memorywire** ([2606.01138](https://arxiv.org/abs/2606.01138)); benchmarked among public agent memory layers in **Nautilus Compass** ([2605.09863](https://arxiv.org/abs/2605.09863)).
- **Zep / Graphiti** — included in **memorywire**'s framework list ([2606.01138](https://arxiv.org/abs/2606.01138)) and in **Nautilus Compass**'s verified comparison set ([2605.09863](https://arxiv.org/abs/2605.09863)).
- **LangMem** — named explicitly in **Manufactured Confidence** ([2606.29279](https://arxiv.org/abs/2606.29279)) as a product whose consolidation rewrites hedged remarks into confident stored facts.
- **Cognee / MemoryOS / MemTensor** — enumerated in the framework lists of [2606.01138](https://arxiv.org/abs/2606.01138) and [2605.09863](https://arxiv.org/abs/2605.09863).

**⚠️ NOT FOUND as dedicated papers:** There is **no arXiv paper whose title is a security analysis or advisory for Mem0, Zep, Letta/MemGPT, or LangMem individually**. There are also **no CVE-numbered advisories** for these memory servers in the arXiv literature or in the sources I fetched. If you need vendor-specific advisories, that is a **GitHub Security Advisory / CVE-database task, not an arXiv task** — I did not verify those and make no claim about them. Most likely candidate ID: **2608.02843** — *MutMem: Cryptographically Authorized Mutation in Persistent Agent Memory* (Aug 2026) — verify separately.

### C.5 RAG and knowledge-graph poisoning (foundational + 2026)

- **arXiv:2407.12784** — *AgentPoison: Red-teaming LLM Agents via Poisoning Memory or Knowledge Bases* (17 Jul 2024) — the foundational memory/RAG backdoor attack.
- **arXiv:2505.18543** — *Benchmarking Poisoning Attacks against Retrieval-Augmented Generation* (24 May 2025) — **first comprehensive RAG poisoning benchmark**: 5 QA datasets + 10 expanded variants, **13 attack methods, 7 defenses**. Attacks perform well on standard QA but **drop significantly on expanded versions**; sequential/branching/conditional/loop RAG, multi-turn conversational RAG, multimodal RAG and RAG-based LLM agents all remain susceptible; **current defenses fail to provide robust protection**.
- **arXiv:2608.20756** — *Vis-Poison: Poisoning Visual Knowledge in Multimodal RAG* (21 Aug 2026) — **40.16%–65.40% end-to-end ASR** against 30k-entry multimodal KBs in **black-box** settings; **>60%** average against MLLMs that answer correctly from parametric knowledge. Findings of **EMNLP 2026**.
- **arXiv:2608.21095** — *Trustworthy RAG: An Evaluation Agent for Detecting Misinformation and Knowledge Poisoning* (21 Aug 2026) — Trust Index T = 0.4F + 0.35C + 0.25(1−P); **91% accuracy, 100% precision, 100% recall on instruction injection**; ROC-AUC **0.73–0.81**; **in-place edits such as entity swaps remain hard to detect**. Accepted **ICSEA 2026**.
- **arXiv:2607.00422** — *KidnapRAG: A Black-Box Attack for Hijacking Reasoning in Agentic RAG* (1 Jul 2026, v2 28 Aug) — Bait / Chain-Link / Mal-Ins role-specific documents; accepted **EMNLP 2026 Main**.
- **arXiv:2607.17535** — *Salience Induction against Multi-Hop RAG Agents* (20 Jul 2026) — attacks the **salience channel** with **truth-preserving edits**: **83.3% ASR** at 30% edit budget; strongest baseline defense leaves **75.7% post-defense ASR**; Salience Normalization reduces to **15.3%** (standard) / **23.6%** (adaptive).
- **arXiv:2608.04366** — *Combating Knowledge Corruption in Agent Systems: Byzantine-Tolerant Secure Collaborative RAG* — **ACM Web Conference 2026**, pp. 2661–2672, DOI 10.1145/3774904.3792200.
- **arXiv:2608.17153** — *Towards Safer RAG: Only Agents Capable of System 2 Thinking may Access Untrusted Documents* (v2 16 Sep 2026) — reasoning reduces Cordon Rate **0.211 → 0.107** and Leakage Rate **0.235 → 0.140**, but overall attack success **rises 0.233 → 0.298**: poison detection, attack success, and resistance to contextual influence are **distinct capabilities**.
- **arXiv:2606.28666** — TRiSM-guided healthcare workflows (27 Jun 2026) — RAG poisoning ASR **31% → 10%**; data-field injection **42% → 25%**.
- **arXiv:2606.26793** — *MIRROR* (25 Jun 2026) — **76% ASR image poisoning**, **97% ASR orchestrator attacks**; ART-SafeBench **41,815 records**.
- **arXiv:2602.09319** — *Benchmarking Knowledge-Extraction Attack and Defense on RAG* (v3 8 Jun 2026).
- **arXiv:2601.07072** — *Overcoming the Retrieval Barrier: Indirect Prompt Injection in the Wild* (11 Jan 2026) — **near-100% retrieval across 11 benchmarks and 8 embedding models**; **$0.21 per target user query**; one poisoned email coerced GPT-4o into SSH-key exfiltration at **>80%** success.

---

## NEWEST 2026 FINDINGS

**Newest submission dates found across the whole survey (as of 2026-09-19):**

| Rank | arXiv | Title (short) | Submitted |
|---|---|---|---|
| 1 | [2609.19425](https://arxiv.org/abs/2609.19425) | Closed-World Resolution Against Tool Hallucination in LLM Agents | **16 Sep 2026** |
| 1= | [2609.19100](https://arxiv.org/abs/2609.19100) | Characterizing Network Centralization in the Remote MCP Ecosystem | **16 Sep 2026** |
| 1= | [2609.19091](https://arxiv.org/abs/2609.19091) | When Agents Look Like Beacons: NIDS Evasion by MCP Traffic | **16 Sep 2026** |
| 1= | [2609.18411](https://arxiv.org/abs/2609.18411) | The Verifiable Action Card | **16 Sep 2026** |
| 1= | [2609.18217](https://arxiv.org/abs/2609.18217) | Measuring and Exploiting Implicit Trust in LLM Tool-Calling Pipelines | **16 Sep 2026** |
| 6 | [2609.17648](https://arxiv.org/abs/2609.17648) | Trust propagation and structural containment in Multi-agent LLM pipelines | 15 Sep 2026 |
| 6= | [2609.17320](https://arxiv.org/abs/2609.17320) | Emergence World: Adversarial Stress-Testing of Long-Horizon Multi-Agent Systems | 15 Sep 2026 |
| 8 | [2609.16098](https://arxiv.org/abs/2609.16098) | Universal Defenses for Tool-Integrated LLM Agents | 14 Sep 2026 |
| 8= | [2609.15906](https://arxiv.org/abs/2609.15906) | Authorization Architectures for Tool-Using AI Agents | 14 Sep 2026 |
| 8= | [2609.14987](https://arxiv.org/abs/2609.14987) | ActGuard: Pre-execution Action Auditing against IPI | 14 Sep 2026 |
| 11 | **[2609.13889](https://arxiv.org/abs/2609.13889)** | **When Malicious Instructions Persist: PMPA (persistent memory poisoning)** | **12 Sep 2026** ← newest *memory-poisoning attack* paper |
| 11= | [2609.14119](https://arxiv.org/abs/2609.14119) | Same Name, Different Server: A Security Census of Silent Drift in MCP | 12 Sep 2026 |
| 13 | [2609.10854](https://arxiv.org/abs/2609.10854) | No-Box Vulnerability Analysis (v2) | 9 Sep 2026 (v2) |
| 13= | [2609.10892](https://arxiv.org/abs/2609.10892) | DriftNet: Dual-Head Trajectory Transformer | 9 Sep 2026 |
| 15 | **[2609.08747](https://arxiv.org/abs/2609.08747)** | **MemSentry (persistent memory poisoning detection)** | **8 Sep 2026** ← newest *memory-defense* paper |
| 15= | **[2609.08258](https://arxiv.org/abs/2609.08258)** | **Revoked but Still Authoritative (memory revocation failure)** | **8 Sep 2026** |

**All papers confirmed as genuinely from 2026 (ID 2601–2609), by ID month:**

- **2609 (Sep 2026):** ~40 papers, incl. 2609.00523, 2609.01836, 2609.02265, 2609.02690, 2609.08258, 2609.08747, 2609.10707, 2609.10854, 2609.10871, 2609.10892, 2609.11952, 2609.13334, **2609.13889**, 2609.14119, 2609.14721, 2609.14744, 2609.14780, 2609.14987, 2609.15906, 2609.16098, 2609.17320, 2609.17648, 2609.18217, 2609.18411, 2609.19091, 2609.19100, 2609.19425
- **2608 (Aug 2026):** ~45 papers, incl. 2608.00150, 2608.01637, 2608.02843, 2608.03844, 2608.04366, 2608.06477, 2608.08795, 2608.10760, 2608.13574, 2608.17153, 2608.17665, 2608.20481, 2608.20631, 2608.20756, 2608.21095, **2608.21230**, 2608.22061, **2608.23471**, 2608.23763, 2608.23858, 2608.24022, 2608.24957, 2608.30177
- **2607 (Jul 2026):** ~25 papers, incl. 2607.00422, **2607.05189**, 2607.05743, 2607.05744, **2607.06595**, 2607.07461, 2607.11086, 2607.14754, 2607.15657, 2607.17535, 2607.19430, 2607.19432, 2607.24625, **2607.27080**
- **2606 (Jun 2026):** ~25 papers, incl. 2606.01138, 2606.04329, 2606.05743, 2606.10742, 2606.12703, 2606.15899, 2606.18356, 2606.22030, 2606.24322, 2606.24402, 2606.26793, 2606.28270, 2606.28666, 2606.29073, **2606.29279**, **2606.30566**
- **2605 (May 2026):** ~20 papers, incl. **2605.01970**, 2605.03228, **2605.09033**, 2605.09330, 2605.09863, 2605.14421, **2605.15338**, 2605.16233, 2605.17830, 2605.18930, **2605.22842**, **2605.26154**, 2605.29960, 2605.30604
- **2604 (Apr 2026):** ~8 papers, incl. **2604.02623**, 2604.03588, 2604.09747, 2604.23711, 2604.27707
- **2603 (Mar 2026):** 2603.02240, 2603.09134, 2603.11324, 2603.13830, 2603.24837
- **2602 (Feb 2026):** 2602.01129, 2602.09319, 2602.13480, 2602.21529
- **2601 (Jan 2026):** 2601.01241, 2601.07072, 2601.07395, 2601.13112, **2601.17548**, **2601.17549**

**Papers that are NOT 2026 but were revised in 2026** (important distinction for citation): 2508.13220 (v3 12 Feb 2026), 2508.10991 (v3 5 Jan / v4 8 Jan 2026), 2506.13538 (v5 13 Apr 2026), 2508.12538 (v2 25 May 2026), 2508.20412 (v3 15 Jan 2026), 2509.06572 (v5 1 May 2026), 2512.06556 (v2 21 May 2026), 2410.03055 (v3 14 Jan 2026), 2510.22963 (v4 19 Jun 2026), 2508.02312 (v2 14 Sep 2026).

---

## NOT VERIFIED / NOT FOUND

### Searched for but NO paper found
1. **"MCPSecBench" as a distinct arXiv ID from MCPTox** — resolved; they are genuinely separate papers (2508.13220 and 2508.14925). No ambiguity.
2. **Any dedicated arXiv paper on Mem0, Zep, Letta/MemGPT, or LangMem security as the *title subject*** — **NIE ZWERYFIKOWANO.** These products appear as *targets* or *adapters* inside broader papers (see C.4) but no standalone security paper per product was found. I did **not** search GitHub Security Advisories, NVD, or vendor blogs for these — out of scope and unverified.
3. **A paper titled "MemGhost"** — **NIE ZWERYFIKOWANO as a title.** MemGhost is the *attack framework* in arXiv:2607.05189 (*When Claws Remember but Do Not Tell*). The "MemGhost" headline circulating on the web is a blog post, not a paper.
4. **A paper titled "GhostWriter"** — **NIE ZWERYFIKOWANO as a title.** GhostWriter is the *attack* in arXiv:2607.06595 (*When Agents Remember Too Much*).
5. **EchoLeak quantitative ASR** — **NIE ZWERYFIKOWANO.** The abstract reports no attack success rate. It is a qualitative case study of one CVE.
6. **Any 2026 paper specifically titled "MCP tool poisoning defense 2026"** — no exact title match. The closest substantive 2026 defenses are ActGuard (2609.14987), FlowGuard (2607.14754), MindGuard (2508.20412, revised into 2026), ChannelGuard (2607.19430), TrustShiftProbe (2608.23763), AEGIS (2608.20481) and SPELLSMITH (2607.07461).

### ⚠️ Suspected metadata corruption — do NOT cite without re-verification
- **arXiv:2608.23601** — *StateTune: Transforming LLM-Assisted EDA Flow Tuning...* (19 Aug 2026). In the arXiv API Atom feed the `<title>` and `<summary>` fields for this entry appear **interleaved with the summary of an unrelated paper** about certified defence against runtime memory poisoning (the text that matches 2606.12703, SMSR). The full raw feed is preserved at the spill path noted below. **Treat 2608.23601's record as unreliable in the API; fetch https://arxiv.org/abs/2608.23601 directly if you need it.** The SMSR content itself is correctly attributed to 2606.12703 by its own separate entry.
- One API entry was truncated mid-author-list with a garbled author name fragment. **No fabricated IDs were introduced** — every ID in this document is either from a page I fetched or from a well-formed Atom `<id>` element.

### IDs listed above that I did NOT individually page-fetch (verify before citing)
These came from well-formed arXiv API Atom records (title, authors, date, and full abstract all present and coherent), which is strong evidence, but I did not open the `abs/` page for each:
`2601.07072, 2601.07395, 2601.13112, 2601.17548, 2602.01129, 2602.09319, 2602.13480, 2602.21529, 2603.02240, 2603.09134, 2603.11324, 2603.13830, 2603.24837, 2604.03588, 2604.05719, 2604.09747, 2604.23711, 2604.27707, 2605.01970, 2605.03228, 2605.09330, 2605.09863, 2605.14421, 2605.15338, 2605.16233, 2605.17830, 2605.18930, 2605.22842, 2605.26154, 2605.29960, 2605.30604, 2606.01138, 2606.04329, 2606.05743, 2606.06337, 2606.12703, 2606.12797, 2606.15899, 2606.18356, 2606.22030, 2606.24322, 2606.24402, 2606.26793, 2606.28270, 2606.28666, 2606.29073, 2606.29279, 2606.30566, 2607.00422, 2607.05743, 2607.05744, 2607.07461, 2607.11086, 2607.12406, 2607.14754, 2607.14798, 2607.15657, 2607.17535, 2607.19292, 2607.19430, 2607.19432, 2607.24006, 2607.24625, 2607.25297, 2607.27080, 2608.00150, 2608.00718, 2608.00997, 2608.01609, 2608.01637, 2608.02018, 2608.02657, 2608.02843, 2608.03844, 2608.04366, 2608.04741, 2608.05430, 2608.05715, 2608.06477, 2608.07622, 2608.08100, 2608.08795, 2608.08939, 2608.10760, 2608.13574, 2608.17153, 2608.17665, 2608.18260, 2608.18351, 2608.18740, 2608.20481, 2608.20631, 2608.20756, 2608.21095, 2608.22061, 2608.23763, 2608.23858, 2608.23992, 2608.24022, 2608.24957, 2608.28794, 2609.00267, 2609.00523, 2609.01693, 2609.01836, 2609.02265, 2609.02690, 2609.08258, 2609.10707, 2609.10871, 2609.10892, 2609.11952, 2609.13334, 2609.14721, 2609.14744, 2609.14780, 2609.14987, 2609.15906, 2609.16098, 2609.17320, 2609.17648, 2609.18411, 2609.19091, 2609.19100, 2609.19425, 2512.05365, 2512.08290, 2512.15790, 2512.16962, 2505.18543, 2506.13538, 2507.06250, 2508.02312, 2508.06418, 2508.12538, 2508.20412, 2509.06572, 2509.21011, 2510.22963, 2407.12784, 2410.03055`

**Pages I DID fetch and verify directly (18):** 2508.14925, 2508.13220, 2508.10991, 2601.17549, 2601.01241, 2509.10540, 2604.02623, 2512.06556, 2605.09033, 2608.30177, 2607.06595, 2607.05189, 2608.23471, 2606.10742, plus the arXiv API endpoints listed below.

### Primary-source URLs used
- arXiv API: `https://export.arxiv.org/api/query?search_query=all:"memory poisoning"&start=0&max_results=50&sortBy=submittedDate&sortOrder=descending` (70 total results)
- arXiv API: `.../query?search_query=all:"Model Context Protocol" AND all:"security"&...` (206 total results)
- arXiv API: `.../query?search_query=all:"tool poisoning" OR all:"rug pull"&...` (60 total results)
- arXiv API: `.../query?search_query=all:"agent memory" AND (all:"attack" OR all:"poisoning")&...` (55 total results)
- arXiv API: `.../query?search_query=(all:"Mem0" OR all:"Letta" OR all:"MemGPT" OR all:"Zep") AND (all:"poisoning" OR all:"attack" OR all:"security")&...` (5 total results)
- arXiv API: `.../query?search_query=all:"memory" AND all:"Model Context Protocol" AND (all:"attack" OR all:"poisoning" OR all:"security")&...` (15 total results)
- arXiv API: `.../query?search_query=(all:"RAG" OR all:"retrieval-augmented" OR all:"knowledge graph") AND all:"poisoning" AND all:"agent"&...` (40 total results)
- arXiv API: `.../query?search_query=all:"indirect prompt injection"&...` (185 total results)
- Note: `http://export.arxiv.org/...` cross-origin-redirects; use `https://export.arxiv.org/...` directly.

**Raw API response spill files** (this session): `/var/folders/pn/lq0q6bzd4cbg1_4jsm502n6m0000gn/T/dsh-spill-Vz0m0x/session-cb76e7cc822e/`
(`4b16dac78d91-`, `779ed3427dd9-`, `66dfa19b2d9e-`, `97dcd4c6bc3d-`, `ea12563a0d77-`, `a9b3430cd6f2-web_fetch.txt`)
