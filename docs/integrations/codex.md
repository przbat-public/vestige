# Codex CLI

> Give OpenAI Codex CLI persistent memory across sessions.

Codex CLI supports MCP servers via its configuration file. Add Vestige and Codex remembers your codebase patterns, decisions, and debugging history.

---

## Setup

### 1. Build Vestige

```bash
cargo build --release -p vestige-mcp
```

### 2. Add to Codex config

Edit `~/.codex/config.json`:

```json
{
  "mcpServers": {
    "vestige": {
      "command": "/path/to/vestige-mcp",
      "args": []
    }
  }
}
```

Replace `/path/to/vestige-mcp` with the actual binary path (e.g., `target/release/vestige-mcp`).

### 3. Add instructions

Create or edit `AGENTS.md` in your project root:

```markdown
## Memory

Use the `vestige` MCP server for persistent memory.
- At session start: call `session_context` to load relevant context.
- After solving bugs: call `smart_ingest` to save the root cause and fix.
- After architecture decisions: call `codebase(action='remember_decision')`.
- For factual questions: call `deep_reference` for trust-scored reasoning across memories.
```

---

## Verify

Run Codex in your project directory. It should list `vestige` among available MCP tools. Test with:

```
Ask Codex: "What do you remember about this project?"
```

It will call `session_context` or `search` and return stored memories.

---

## Tips

- **Use `deep_reference`** for questions like "Why did we choose X?" — it scores memories by trust and detects contradictions.
- **Use `search` with `retrieval_mode: "precise"`** for quick lookups that save tokens.
- **Use `search` with `retrieval_mode: "exhaustive"`** when you need comprehensive recall.
- **Batch retrieval**: Use `memory(action='get_batch', ids=[...])` to fetch multiple memories in one call (max 20).
