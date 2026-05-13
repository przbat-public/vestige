# Storage Configuration

> Global, per-project, and multi-Claude setups

---

## Database Location

All memories are stored in a **single local SQLite file**:

| Platform | Database Location |
|----------|------------------|
| macOS | `~/Library/Application Support/com.vestige.core/vestige.db` |
| Linux | `~/.local/share/core/vestige.db` (`$XDG_DATA_HOME/core/` if set) |
| Windows | `%APPDATA%\vestige\core\data\vestige.db` |

> Paths come from the [`directories`](https://docs.rs/directories) crate (`ProjectDirs::from("com", "vestige", "core")` → `data_dir()`). The Linux folder is just `core/` because `directories` uses only the application name on Linux per XDG. Override the location with `--data-dir <PATH>` (see below) if you want a stable path that does not depend on the platform.

---

## Storage Modes

### Option 1: Global Memory (Default)

One shared memory for all projects. Good for:
- Personal preferences that apply everywhere
- Cross-project learning
- Simpler setup

```bash
# Default behavior - no configuration needed
claude mcp add vestige vestige-mcp -s user
```

### Option 2: Per-Project Memory

Separate memory per codebase. Good for:
- Client work (keep memories isolated)
- Different coding styles per project
- Team environments

**Claude Code Setup:**

Add to your project's `.claude/settings.local.json`:
```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp",
      "args": ["--data-dir", "./.vestige"]
    }
  }
}
```

This creates `.vestige/vestige.db` in your project root. Add `.vestige/` to `.gitignore`.

**Multiple Named Instances:**

For power users who want both global AND project memory:
```json
{
  "mcpServers": {
    "vestige-global": {
      "command": "vestige-mcp"
    },
    "vestige-project": {
      "command": "vestige-mcp",
      "args": ["--data-dir", "./.vestige"]
    }
  }
}
```

### Option 3: Multi-Claude Household

For setups with multiple Claude instances (e.g., Claude Desktop + Claude Code, or two personas):

**Shared Memory (Both Claudes share memories):**
```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp",
      "args": ["--data-dir", "~/shared-vestige"]
    }
  }
}
```

**Separate Identities (Each Claude has own memory):**

Claude Desktop config - for "Domovoi":
```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp",
      "args": ["--data-dir", "~/vestige-domovoi"]
    }
  }
}
```

Claude Code config - for "Storm":
```json
{
  "mcpServers": {
    "vestige": {
      "command": "vestige-mcp",
      "args": ["--data-dir", "~/vestige-storm"]
    }
  }
}
```

---

## Data Safety

**Important:** Vestige stores data locally with no cloud sync, redundancy, or automatic backup.

| Use Case | Risk Level | Recommendation |
|----------|------------|----------------|
| AI conversation memory | Low | Acceptable without backup—easily rebuilt |
| Coding patterns & decisions | Medium | Periodic backups recommended |
| Sensitive/critical data | High | **Not recommended**—use purpose-built systems |

**Vestige is not designed for:** medical records, financial transactions, legal documents, or any data requiring compliance guarantees.

---

## Backup Options

### Manual (one-time)

```bash
# macOS
cp ~/Library/Application\ Support/com.vestige.core/vestige.db ~/vestige-backup.db

# Linux
cp ~/.local/share/core/vestige.db ~/vestige-backup.db
```

### Automated (cron job)

```bash
# Add to crontab - backs up every hour
0 * * * * cp ~/Library/Application\ Support/com.vestige.core/vestige.db ~/.vestige-backups/vestige-$(date +\%Y\%m\%d-\%H\%M).db
```

### System Backups

Just use **Time Machine** (macOS) / **Windows Backup** / **rsync** — they'll catch the file automatically.

> For personal use with Claude? Don't overthink it. The memories aren't that precious.

---

## Direct SQL Access

The database is just SQLite. You can query it directly:

```bash
sqlite3 ~/Library/Application\ Support/com.vestige.core/vestige.db

# Example queries
SELECT content, retention_strength FROM knowledge_nodes ORDER BY retention_strength DESC LIMIT 10;
SELECT content FROM knowledge_nodes WHERE tags LIKE '%identity%';
SELECT COUNT(*) FROM knowledge_nodes WHERE retention_strength < 0.1;
```

**Caution**: Don't modify the database while Vestige is running.
