//! Export command — JSON / JSONL export with tag and date filters.

use std::io::{BufWriter, Write};
use std::path::PathBuf;

use chrono::NaiveDate;
use colored::Colorize;
use vestige_core::Storage;

use super::util::fetch_all_nodes;

/// Run export command - exports memories in JSON or JSONL format
pub(super) fn run_export(
    output: PathBuf,
    format: String,
    tags: Option<String>,
    since: Option<String>,
) -> anyhow::Result<()> {
    println!("{}", "=== Vestige Export ===".cyan().bold());
    println!();

    // Validate format
    if format != "json" && format != "jsonl" {
        anyhow::bail!("Invalid format '{}'. Must be 'json' or 'jsonl'.", format);
    }

    // Parse since date if provided
    let since_date = match &since {
        Some(date_str) => {
            let naive = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").map_err(|e| {
                anyhow::anyhow!("Invalid date '{}': {}. Use YYYY-MM-DD format.", date_str, e)
            })?;
            Some(
                naive
                    .and_hms_opt(0, 0, 0)
                    .expect("midnight is always valid")
                    .and_utc(),
            )
        }
        None => None,
    };

    // Parse tags filter
    let tag_filter: Vec<String> = tags
        .as_deref()
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let storage = Storage::new(None)?;
    let all_nodes = fetch_all_nodes(&storage)?;

    // Apply filters
    let filtered: Vec<&vestige_core::KnowledgeNode> = all_nodes
        .iter()
        .filter(|node| {
            // Date filter
            if let Some(ref since_dt) = since_date
                && node.created_at < *since_dt
            {
                return false;
            }
            // Tag filter: node must contain ALL specified tags
            if !tag_filter.is_empty() {
                for tag in &tag_filter {
                    if !node.tags.iter().any(|t| t == tag) {
                        return false;
                    }
                }
            }
            true
        })
        .collect();

    println!("{}: {}", "Format".white().bold(), format);
    if !tag_filter.is_empty() {
        println!("{}: {}", "Tag filter".white().bold(), tag_filter.join(", "));
    }
    if let Some(ref date_str) = since {
        println!("{}: {}", "Since".white().bold(), date_str);
    }
    println!(
        "{}: {} / {} total",
        "Matching".white().bold(),
        filtered.len(),
        all_nodes.len()
    );
    println!();

    // Create parent directories if needed
    if let Some(parent) = output.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(&output)?;
    let mut writer = BufWriter::new(file);

    match format.as_str() {
        "json" => {
            serde_json::to_writer_pretty(&mut writer, &filtered)?;
            writer.write_all(b"\n")?;
        }
        "jsonl" => {
            for node in &filtered {
                serde_json::to_writer(&mut writer, node)?;
                writer.write_all(b"\n")?;
            }
        }
        _ => unreachable!(),
    }

    writer.flush()?;

    let file_size = std::fs::metadata(&output)?.len();
    let size_display = if file_size >= 1024 * 1024 {
        format!("{:.2} MB", file_size as f64 / (1024.0 * 1024.0))
    } else if file_size >= 1024 {
        format!("{:.1} KB", file_size as f64 / 1024.0)
    } else {
        format!("{} bytes", file_size)
    };

    println!(
        "{}",
        format!(
            "Exported {} memories to {} ({}, {})",
            filtered.len(),
            output.display(),
            format,
            size_display
        )
        .green()
        .bold()
    );

    Ok(())
}
