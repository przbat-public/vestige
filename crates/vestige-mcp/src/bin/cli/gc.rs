//! Garbage-collect command — delete stale memories below retention threshold.

use std::io::Write;

use chrono::Utc;
use colored::Colorize;
use vestige_core::Storage;

use super::util::{fetch_all_nodes, truncate};

pub(super) fn run_gc(
    min_retention: f64,
    max_age_days: Option<u64>,
    dry_run: bool,
    yes: bool,
) -> anyhow::Result<()> {
    println!("{}", "=== Vestige Garbage Collection ===".cyan().bold());
    println!();

    let storage = Storage::new(None)?;
    let all_nodes = fetch_all_nodes(&storage)?;
    let now = Utc::now();

    // Find candidates for deletion
    let candidates: Vec<&vestige_core::KnowledgeNode> = all_nodes
        .iter()
        .filter(|node| {
            // Must be below retention threshold
            if node.retention_strength >= min_retention {
                return false;
            }
            // If max_age_days specified, must also be older than that
            if let Some(max_days) = max_age_days {
                let age_days = (now - node.created_at).num_days();
                if age_days < 0 || (age_days as u64) < max_days {
                    return false;
                }
            }
            true
        })
        .collect();

    println!(
        "{}: {}",
        "Min retention threshold".white().bold(),
        min_retention
    );
    if let Some(max_days) = max_age_days {
        println!("{}: {} days", "Max age".white().bold(), max_days);
    }
    println!(
        "{}: {} / {} total",
        "Candidates for deletion".white().bold(),
        candidates.len(),
        all_nodes.len()
    );

    if candidates.is_empty() {
        println!();
        println!(
            "{}",
            "No memories match the garbage collection criteria.".green()
        );
        return Ok(());
    }

    // Show sample of what would be deleted
    println!();
    println!("{}", "Sample of memories to be removed:".yellow().bold());
    let sample_count = candidates.len().min(10);
    for node in candidates.iter().take(sample_count) {
        let age_days = (now - node.created_at).num_days();
        println!(
            "  {} [ret={:.3}, age={}d] {}",
            node.id[..8].dimmed(),
            node.retention_strength,
            age_days,
            truncate(&node.content, 60).dimmed()
        );
    }
    if candidates.len() > sample_count {
        println!(
            "  {} ... and {} more",
            "".dimmed(),
            candidates.len() - sample_count
        );
    }

    if dry_run {
        println!();
        println!(
            "{}",
            format!(
                "Dry run: {} memories would be deleted. Re-run without --dry-run to delete.",
                candidates.len()
            )
            .yellow()
            .bold()
        );
        return Ok(());
    }

    // Confirmation prompt (unless --yes)
    if !yes {
        println!();
        print!(
            "{} Delete {} memories? This cannot be undone. [y/N] ",
            "WARNING:".red().bold(),
            candidates.len()
        );
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            println!("{}", "Aborted.".yellow());
            return Ok(());
        }
    }

    // Perform deletion
    let mut deleted = 0;
    let mut errors = 0;
    let total_candidates = candidates.len();

    for node in &candidates {
        match storage.delete_node(&node.id) {
            Ok(true) => deleted += 1,
            Ok(false) => errors += 1, // node was already gone
            Err(e) => {
                eprintln!(
                    "  {} Failed to delete {}: {}",
                    "ERR".red(),
                    &node.id[..8],
                    e
                );
                errors += 1;
            }
        }
    }

    println!();
    println!(
        "{}",
        format!(
            "Garbage collection complete: {}/{} memories deleted{}",
            deleted,
            total_candidates,
            if errors > 0 {
                format!(" ({} errors)", errors)
            } else {
                String::new()
            }
        )
        .green()
        .bold()
    );

    Ok(())
}

