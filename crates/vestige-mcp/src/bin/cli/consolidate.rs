//! Consolidate command — run a memory consolidation cycle.

use colored::Colorize;
use vestige_core::Storage;

pub(super) fn run_consolidate() -> anyhow::Result<()> {
    println!("{}", "=== Vestige Consolidation ===".cyan().bold());
    println!();
    println!("Running memory consolidation cycle...");
    println!();

    let storage = Storage::new(None)?;
    let result = storage.run_consolidation()?;

    println!(
        "{}: {}",
        "Nodes Processed".white().bold(),
        result.nodes_processed
    );
    println!(
        "{}: {}",
        "Nodes Promoted".white().bold(),
        result.nodes_promoted
    );
    println!("{}: {}", "Nodes Pruned".white().bold(), result.nodes_pruned);
    println!(
        "{}: {}",
        "Decay Applied".white().bold(),
        result.decay_applied
    );
    println!(
        "{}: {}",
        "Embeddings Generated".white().bold(),
        result.embeddings_generated
    );
    println!("{}: {}ms", "Duration".white().bold(), result.duration_ms);

    println!();
    println!(
        "{}",
        format!(
            "Consolidation complete: {} nodes processed, {} embeddings generated in {}ms",
            result.nodes_processed, result.embeddings_generated, result.duration_ms
        )
        .green()
    );

    Ok(())
}
