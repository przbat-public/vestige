//! Vestige Restore — lightweight CLI to import memories from a JSON backup.
//!
//! This binary is intentionally minimal: it depends on `vestige-core` with the
//! `embeddings` and `vector-search` features turned **off** so the
//! single-purpose restore tool doesn't drag in fastembed (ONNX runtime) or
//! USearch. Embeddings can be regenerated after restore via the
//! `regenerate_embeddings` MCP tool.

use std::path::PathBuf;
use vestige_core::{IngestInput, Storage};

#[derive(serde::Deserialize)]
struct BackupWrapper {
    #[serde(rename = "type")]
    _type: String,
    text: String,
}

#[derive(serde::Deserialize)]
struct RecallResult {
    results: Vec<MemoryBackup>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MemoryBackup {
    content: String,
    node_type: Option<String>,
    tags: Option<Vec<String>>,
    source: Option<String>,
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: vestige-restore <backup.json>");
        eprintln!();
        eprintln!("After restore completes, run the `regenerate_embeddings` MCP");
        eprintln!("tool (or call `vestige-mcp consolidate`) to rebuild vector");
        eprintln!("search indices for the imported memories.");
        std::process::exit(1);
    }

    let backup_path = PathBuf::from(&args[1]);
    println!("Loading backup from: {}", backup_path.display());

    // Handle the SQLite snapshot the `backup` tool writes (VACUUM INTO), not just JSON.
    if is_sqlite_snapshot(&backup_path) {
        println!("Detected a SQLite snapshot — importing it.");
        let storage = Storage::new(None)?;
        let report = storage.restore_from_snapshot(&backup_path)?;
        println!(
            "Imported {} of {} memories ({} history revisions, {} code anchors, {} associations, {} lifecycle states, {} vectors copied, {} awaiting re-embedding).",
            report.nodes_imported,
            report.nodes_in_snapshot,
            report.revisions_imported,
            report.code_refs_imported,
            report.connections_imported,
            report.states_imported,
            report.embeddings_imported,
            report.embeddings_reset
        );
        println!();
        println!("Run the `regenerate_embeddings` MCP tool to rebuild vector search.");
        return Ok(());
    }

    let backup_content = std::fs::read_to_string(&backup_path)?;
    let wrapper: Vec<BackupWrapper> = serde_json::from_str(&backup_content)?;
    let recall_result: RecallResult = serde_json::from_str(&wrapper[0].text)?;
    let memories = recall_result.results;

    println!("Found {} memories to restore", memories.len());

    println!("Initializing storage...");
    let storage = Storage::new(None)?;

    println!("Ingesting memories (embeddings will be backfilled later)...\n");

    let total = memories.len();
    let mut success_count = 0;

    for (i, memory) in memories.into_iter().enumerate() {
        let input = IngestInput {
            content: memory.content.clone(),
            node_type: memory.node_type.unwrap_or_else(|| "fact".to_string()),
            source: memory.source,
            sentiment_score: 0.0,
            sentiment_magnitude: 0.0,
            tags: memory.tags.unwrap_or_default(),
            valid_from: None,
            valid_until: None,
            provenance: None,
            ..Default::default()
        };

        match storage.ingest(input) {
            Ok(_node) => {
                success_count += 1;
                println!(
                    "[{}/{}] OK: {}",
                    i + 1,
                    total,
                    truncate(&memory.content, 60)
                );
            }
            Err(e) => {
                println!("[{}/{}] FAIL: {}", i + 1, total, e);
            }
        }
    }

    println!(
        "\nRestore complete: {}/{} memories restored",
        success_count, total
    );
    println!("Next: call the `regenerate_embeddings` MCP tool to rebuild semantic indices.");

    Ok(())
}

/// The first `n` **bytes** of `s`, floored to a character boundary, plus an
/// ellipsis when anything was dropped.
///
/// `&s[..n]` panics when byte `n` lands inside a multi-byte character, and
/// restore prints the content of every memory it wrote — so a single Polish
/// memory longer than `n` bytes could abort a restore that had already written
/// rows, halfway through the file. The cut is floored rather than the limit
/// changed: the cap is for the console, not for the data.
fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..s.floor_char_boundary(n)])
    }
}

#[cfg(test)]
mod tests {
    use super::truncate;

    /// The restore path printed its way into a panic: `&content[..60]` with a
    /// multi-byte character at byte 60 took the process down (with
    /// `panic = "abort"` in the release profile there is no unwinding to catch).
    #[test]
    fn a_long_memory_is_shortened_without_splitting_a_character() {
        let polish =
            "Objaw: emulowana gra nie wchodziła do poziomu, tylko w kółko odtwarzała intro.";
        assert!(polish.len() > 60 && !polish.is_char_boundary(60));

        let shortened = truncate(polish, 60);
        assert!(shortened.ends_with("..."));
        let kept = shortened.trim_end_matches('.');
        assert!(
            polish.starts_with(kept),
            "only a prefix may survive: {kept:?}"
        );
        assert!(kept.len() <= 60);
    }

    #[test]
    fn a_short_memory_is_printed_whole() {
        assert_eq!(truncate("krótko", 60), "krótko");
        assert_eq!(truncate("", 60), "");
    }
}

/// Whether `path` begins with the SQLite file header.
fn is_sqlite_snapshot(path: &std::path::Path) -> bool {
    use std::io::Read;

    let mut header = [0u8; 16];
    match std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut header)) {
        Ok(()) => &header == b"SQLite format 3\0",
        Err(_) => false,
    }
}
