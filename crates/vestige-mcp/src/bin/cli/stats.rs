//! Stats command — show memory statistics, tagging distribution, cognitive states.

use colored::Colorize;
use vestige_core::Storage;

pub(super) fn run_stats(show_tagging: bool, show_states: bool) -> anyhow::Result<()> {
    let storage = Storage::new(None)?;
    let stats = storage.get_stats()?;

    println!("{}", "=== Vestige Memory Statistics ===".cyan().bold());
    println!();

    // Basic stats
    println!("{}: {}", "Total Memories".white().bold(), stats.total_nodes);
    println!(
        "{}: {}",
        "Due for Review".white().bold(),
        stats.nodes_due_for_review
    );
    println!(
        "{}: {:.1}%",
        "Average Retention".white().bold(),
        stats.average_retention * 100.0
    );
    println!(
        "{}: {:.2}",
        "Average Storage Strength".white().bold(),
        stats.average_storage_strength
    );
    println!(
        "{}: {:.2}",
        "Average Retrieval Strength".white().bold(),
        stats.average_retrieval_strength
    );
    println!(
        "{}: {}",
        "With Embeddings".white().bold(),
        stats.nodes_with_embeddings
    );

    if let Some(model) = &stats.embedding_model {
        println!("{}: {}", "Embedding Model".white().bold(), model);
    }

    if let Some(oldest) = stats.oldest_memory {
        println!(
            "{}: {}",
            "Oldest Memory".white().bold(),
            oldest.format("%Y-%m-%d %H:%M:%S")
        );
    }
    if let Some(newest) = stats.newest_memory {
        println!(
            "{}: {}",
            "Newest Memory".white().bold(),
            newest.format("%Y-%m-%d %H:%M:%S")
        );
    }

    // Embedding coverage
    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "{}: {:.1}%",
        "Embedding Coverage".white().bold(),
        embedding_coverage
    );

    // Tagging distribution (retention levels)
    if show_tagging {
        println!();
        println!("{}", "=== Retention Distribution ===".yellow().bold());

        let memories = storage.get_all_nodes(500, 0)?;
        let total = memories.len();

        if total > 0 {
            let high = memories
                .iter()
                .filter(|m| m.retention_strength >= 0.7)
                .count();
            let medium = memories
                .iter()
                .filter(|m| m.retention_strength >= 0.4 && m.retention_strength < 0.7)
                .count();
            let low = memories
                .iter()
                .filter(|m| m.retention_strength < 0.4)
                .count();

            print_distribution_bar("High (>=70%)", high, total, "green");
            print_distribution_bar("Medium (40-70%)", medium, total, "yellow");
            print_distribution_bar("Low (<40%)", low, total, "red");
        } else {
            println!("{}", "No memories found.".dimmed());
        }
    }

    // State distribution
    if show_states {
        println!();
        println!(
            "{}",
            "=== Cognitive State Distribution ===".magenta().bold()
        );

        let memories = storage.get_all_nodes(500, 0)?;
        let total = memories.len();

        if total > 0 {
            let (active, dormant, silent, unavailable) = compute_state_distribution(&memories);

            print_distribution_bar("Active", active, total, "green");
            print_distribution_bar("Dormant", dormant, total, "yellow");
            print_distribution_bar("Silent", silent, total, "red");
            print_distribution_bar("Unavailable", unavailable, total, "magenta");

            println!();
            println!("{}", "State Thresholds:".dimmed());
            println!("  {} >= 0.70 accessibility", "Active".green());
            println!("  {} >= 0.40 accessibility", "Dormant".yellow());
            println!("  {} >= 0.10 accessibility", "Silent".red());
            println!("  {} < 0.10 accessibility", "Unavailable".magenta());
        } else {
            println!("{}", "No memories found.".dimmed());
        }
    }

    Ok(())
}

fn compute_state_distribution(
    memories: &[vestige_core::KnowledgeNode],
) -> (usize, usize, usize, usize) {
    use vestige_mcp::tools::memory_unified::{compute_accessibility, state_from_accessibility};

    let (mut active, mut dormant, mut silent, mut unavailable) = (0, 0, 0, 0);
    for memory in memories {
        let acc = compute_accessibility(
            memory.retention_strength,
            memory.retrieval_strength,
            memory.storage_strength,
        );
        match state_from_accessibility(acc) {
            vestige_core::MemoryState::Active => active += 1,
            vestige_core::MemoryState::Dormant => dormant += 1,
            vestige_core::MemoryState::Silent => silent += 1,
            vestige_core::MemoryState::Unavailable => unavailable += 1,
        }
    }
    (active, dormant, silent, unavailable)
}

/// Print a distribution bar
fn print_distribution_bar(label: &str, count: usize, total: usize, color: &str) {
    let percentage = if total > 0 {
        (count as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let bar_width: usize = 30;
    let filled = ((percentage / 100.0) * bar_width as f64) as usize;
    let empty = bar_width.saturating_sub(filled);

    let bar = format!("{}{}", "#".repeat(filled), "-".repeat(empty));
    let colored_bar = match color {
        "green" => bar.green(),
        "yellow" => bar.yellow(),
        "red" => bar.red(),
        "magenta" => bar.magenta(),
        _ => bar.white(),
    };

    println!(
        "  {:15} [{:30}] {:>4} ({:>5.1}%)",
        label, colored_bar, count, percentage
    );
}
