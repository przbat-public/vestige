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
                println!("[{}/{}] OK: {}", i + 1, total, truncate(&memory.content, 60));
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
    println!(
        "Next: call the `regenerate_embeddings` MCP tool to rebuild semantic indices."
    );

    Ok(())
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}...", &s[..n])
    }
}
