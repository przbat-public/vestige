//! Health command — warnings and recommendations.

use colored::Colorize;
use vestige_core::Storage;

pub(super) fn run_health() -> anyhow::Result<()> {
    let storage = Storage::new(None)?;
    let stats = storage.get_stats()?;

    println!("{}", "=== Vestige Health Check ===".cyan().bold());
    println!();

    // Determine health status
    let (status, status_color) = if stats.total_nodes == 0 {
        ("EMPTY", "white")
    } else if stats.average_retention < 0.3 {
        ("CRITICAL", "red")
    } else if stats.average_retention < 0.5 {
        ("DEGRADED", "yellow")
    } else {
        ("HEALTHY", "green")
    };

    let colored_status = match status_color {
        "green" => status.green().bold(),
        "yellow" => status.yellow().bold(),
        "red" => status.red().bold(),
        _ => status.white().bold(),
    };

    println!("{}: {}", "Status".white().bold(), colored_status);
    println!("{}: {}", "Total Memories".white(), stats.total_nodes);
    println!(
        "{}: {}",
        "Due for Review".white(),
        stats.nodes_due_for_review
    );
    println!(
        "{}: {:.1}%",
        "Average Retention".white(),
        stats.average_retention * 100.0
    );

    // Embedding coverage
    let embedding_coverage = if stats.total_nodes > 0 {
        (stats.nodes_with_embeddings as f64 / stats.total_nodes as f64) * 100.0
    } else {
        0.0
    };
    println!(
        "{}: {:.1}%",
        "Embedding Coverage".white(),
        embedding_coverage
    );
    println!(
        "{}: {}",
        "Embedding Service".white(),
        if storage.is_embedding_ready() {
            "Ready".green()
        } else {
            "Not Ready".red()
        }
    );

    // Warnings
    let mut warnings = Vec::new();

    if stats.average_retention < 0.5 && stats.total_nodes > 0 {
        warnings
            .push("Low average retention - consider running consolidation or reviewing memories");
    }

    if stats.nodes_due_for_review > 10 {
        warnings.push("Many memories are due for review");
    }

    if stats.total_nodes > 0 && stats.nodes_with_embeddings == 0 {
        warnings.push("No embeddings generated - semantic search unavailable");
    }

    if embedding_coverage < 50.0 && stats.total_nodes > 10 {
        warnings.push("Low embedding coverage - run consolidation to improve semantic search");
    }

    if !warnings.is_empty() {
        println!();
        println!("{}", "Warnings:".yellow().bold());
        for warning in &warnings {
            println!("  {} {}", "!".yellow().bold(), warning.yellow());
        }
    }

    // Recommendations
    let mut recommendations = Vec::new();

    if status == "CRITICAL" {
        recommendations
            .push("CRITICAL: Many memories have very low retention. Review important memories.");
    }

    if stats.nodes_due_for_review > 5 {
        recommendations.push("Review due memories to strengthen retention.");
    }

    if stats.nodes_with_embeddings < stats.total_nodes {
        recommendations
            .push("Run 'vestige consolidate' to generate embeddings for better semantic search.");
    }

    if stats.total_nodes > 100 && stats.average_retention < 0.7 {
        recommendations.push("Consider running periodic consolidation to maintain memory health.");
    }

    if recommendations.is_empty() && status == "HEALTHY" {
        recommendations.push("Memory system is healthy!");
    }

    println!();
    println!("{}", "Recommendations:".cyan().bold());
    for rec in &recommendations {
        let icon = if rec.starts_with("CRITICAL") {
            "!".red().bold()
        } else {
            ">".cyan()
        };
        let text = if rec.starts_with("CRITICAL") {
            rec.red().to_string()
        } else {
            rec.to_string()
        };
        println!("  {} {}", icon, text);
    }

    Ok(())
}
