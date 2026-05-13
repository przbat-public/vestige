//! Ingest command — write a memory through the prediction-error gating pipeline.


use colored::Colorize;
use vestige_core::{IngestInput, Storage};

use super::util::truncate;

pub(super) fn run_ingest(
    content: String,
    tags: Option<String>,
    node_type: String,
    source: Option<String>,
) -> anyhow::Result<()> {
    if content.trim().is_empty() {
        anyhow::bail!("Content cannot be empty");
    }

    let tag_list: Vec<String> = tags
        .as_deref()
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let input = IngestInput {
        content: content.clone(),
        node_type,
        source,
        sentiment_score: 0.0,
        sentiment_magnitude: 0.0,
        tags: tag_list,
        valid_from: None,
        valid_until: None,
        provenance: None,
        ..Default::default()
    };

    let storage = Storage::new(None)?;

    // Try smart_ingest (PE Gating) if available, otherwise regular ingest
    #[cfg(all(feature = "embeddings", feature = "vector-search"))]
    {
        let result = storage.smart_ingest(input)?;
        println!("{}", "=== Vestige Ingest ===".cyan().bold());
        println!();
        println!("{}: {}", "Decision".white().bold(), result.decision.green());
        println!("{}: {}", "Node ID".white().bold(), result.node.id);
        if let Some(sim) = result.similarity {
            println!("{}: {:.3}", "Similarity".white().bold(), sim);
        }
        if let Some(pe) = result.prediction_error {
            println!("{}: {:.3}", "Prediction Error".white().bold(), pe);
        }
        println!("{}: {}", "Reason".white().bold(), result.reason);
        println!();
        println!(
            "{}",
            format!("Memory {} ({})", result.decision, truncate(&content, 60))
                .green()
                .bold()
        );
    }

    #[cfg(not(all(feature = "embeddings", feature = "vector-search")))]
    {
        let node = storage.ingest(input)?;
        println!("{}", "=== Vestige Ingest ===".cyan().bold());
        println!();
        println!("{}: create", "Decision".white().bold());
        println!("{}: {}", "Node ID".white().bold(), node.id);
        println!();
        println!(
            "{}",
            format!("Memory created ({})", truncate(&content, 60))
                .green()
                .bold()
        );
    }

    Ok(())
}

