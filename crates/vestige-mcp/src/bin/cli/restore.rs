//! Restore command — restore memories from a backup file.

use std::path::PathBuf;

use colored::Colorize;
use vestige_core::{IngestInput, Storage};

use super::util::truncate;

pub(super) fn run_restore(backup_path: PathBuf) -> anyhow::Result<()> {
    println!("{}", "=== Vestige Restore ===".cyan().bold());
    println!();
    println!("Loading backup from: {}", backup_path.display());

    // A `.db` snapshot from the `backup` tool is a SQLite database, not JSON; route it to
    // the snapshot importer instead of failing on UTF-8 decoding.
    if is_sqlite_snapshot(&backup_path) {
        let storage = Storage::new(None)?;
        let report = storage.restore_from_snapshot(&backup_path)?;
        println!(
            "Imported {} of {} memories from the snapshot ({} vectors, {} awaiting re-embedding).",
            report.nodes_imported,
            report.nodes_in_snapshot,
            report.embeddings_imported,
            report.embeddings_reset
        );
        println!();
        println!(
            "{}",
            "Run `vestige-mcp` and the regenerate_embeddings tool to rebuild semantic search."
                .yellow()
        );
        return Ok(());
    }

    // Read and parse backup
    let backup_content = std::fs::read_to_string(&backup_path)?;

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

    let wrapper: Vec<BackupWrapper> = serde_json::from_str(&backup_content)?;
    let recall_result: RecallResult = serde_json::from_str(&wrapper[0].text)?;
    let memories = recall_result.results;

    println!("Found {} memories to restore", memories.len());
    println!();

    // Initialize storage
    println!("Initializing storage...");
    let storage = Storage::new(None)?;

    println!("Generating embeddings and ingesting memories...");
    println!();

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
                    "[{}/{}] {} {}",
                    i + 1,
                    total,
                    "OK".green(),
                    truncate(&memory.content, 60)
                );
            }
            Err(e) => {
                println!("[{}/{}] {} {}", i + 1, total, "FAIL".red(), e);
            }
        }
    }

    println!();
    println!(
        "Restore complete: {}/{} memories restored",
        success_count.to_string().green().bold(),
        total
    );

    // Show stats
    let stats = storage.get_stats()?;
    println!();
    println!("{}: {}", "Total Nodes".white(), stats.total_nodes);
    println!(
        "{}: {}",
        "With Embeddings".white(),
        stats.nodes_with_embeddings
    );

    Ok(())
}

/// Whether `path` begins with the SQLite file header (see `tools::restore`).
fn is_sqlite_snapshot(path: &std::path::Path) -> bool {
    use std::io::Read;

    let mut header = [0u8; 16];
    match std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut header)) {
        Ok(()) => &header == b"SQLite format 3\0",
        Err(_) => false,
    }
}
