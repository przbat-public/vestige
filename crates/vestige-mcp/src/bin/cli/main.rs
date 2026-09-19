//! Vestige CLI
//!
//! Command-line interface for managing the cognitive memory system.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod backup;
mod consolidate;
mod erase;
mod export;
mod gc;
mod health;
mod ingest;
mod restore;
mod serve;
mod stats;
mod util;

/// Vestige - Cognitive Memory System CLI
#[derive(Parser)]
#[command(name = "vestige")]
#[command(author = "samvallad33")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "CLI for the Vestige cognitive memory system")]
#[command(
    long_about = "Vestige is a cognitive memory system built on memory research from Ebbinghaus (1885) to FSRS-6.\n\nIt implements FSRS-6, spreading activation, synaptic tagging, and more."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show memory statistics
    Stats {
        /// Show tagging/retention distribution
        #[arg(long)]
        tagging: bool,

        /// Show cognitive state distribution
        #[arg(long)]
        states: bool,
    },

    /// Run health check with warnings and recommendations
    Health,

    /// Run memory consolidation cycle
    Consolidate,

    /// Restore memories from backup file
    Restore {
        /// Path to backup JSON file
        file: PathBuf,
    },

    /// Create a full backup of the SQLite database
    Backup {
        /// Output file path for the backup
        output: PathBuf,
    },

    /// Export memories in JSON or JSONL format
    Export {
        /// Output file path
        output: PathBuf,
        /// Export format: json or jsonl
        #[arg(long, default_value = "json")]
        format: String,
        /// Filter by tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,
        /// Only export memories created after this date (YYYY-MM-DD)
        #[arg(long)]
        since: Option<String>,
    },

    /// Garbage collect stale memories below retention threshold
    Gc {
        /// Minimum retention strength to keep (delete below this)
        #[arg(long, default_value = "0.1")]
        min_retention: f64,
        /// Maximum age in days (delete memories older than this AND below retention threshold)
        #[arg(long)]
        max_age_days: Option<u64>,
        /// Dry run - show what would be deleted without actually deleting
        #[arg(long)]
        dry_run: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
    },

    /// GDPR Article 17 erasure: hard-delete a memory, or every memory with a tag
    Erase {
        /// Erase a single memory by id
        #[arg(long, conflicts_with = "tag", required_unless_present = "tag")]
        id: Option<String>,
        /// Erase every memory carrying this exact tag (not a prefix match)
        #[arg(long, conflicts_with = "id", required_unless_present = "id")]
        tag: Option<String>,
        /// Show what would be erased without deleting anything
        #[arg(long)]
        dry_run: bool,
        /// Confirm the irreversible deletion (required without --dry-run)
        #[arg(long)]
        confirm: bool,
    },

    /// Launch the memory web dashboard
    Dashboard {
        /// Port to bind the dashboard server to
        #[arg(long, default_value = "3927")]
        port: u16,
        /// Don't automatically open the browser
        #[arg(long)]
        no_open: bool,
    },

    /// Ingest a memory (routes through Prediction Error Gating)
    Ingest {
        /// Content to remember
        content: String,
        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,
        /// Node type (fact, concept, event, person, place, note, pattern, decision)
        #[arg(long, default_value = "fact")]
        node_type: String,
        /// Source reference
        #[arg(long)]
        source: Option<String>,
    },

    /// Start standalone HTTP MCP server (no stdio, for remote access)
    Serve {
        /// HTTP transport port
        #[arg(long, default_value = "3928")]
        port: u16,
        /// Also start the dashboard
        #[arg(long)]
        dashboard: bool,
        /// Dashboard port
        #[arg(long, default_value = "3927")]
        dashboard_port: u16,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Stats { tagging, states } => stats::run_stats(tagging, states),
        Commands::Health => health::run_health(),
        Commands::Consolidate => consolidate::run_consolidate(),
        Commands::Restore { file } => restore::run_restore(file),
        Commands::Backup { output } => backup::run_backup(output),
        Commands::Export {
            output,
            format,
            tags,
            since,
        } => export::run_export(output, format, tags, since),
        Commands::Gc {
            min_retention,
            max_age_days,
            dry_run,
            yes,
        } => gc::run_gc(min_retention, max_age_days, dry_run, yes),
        Commands::Erase {
            id,
            tag,
            dry_run,
            confirm,
        } => erase::run_erase(id, tag, dry_run, confirm),
        Commands::Dashboard { port, no_open } => serve::run_dashboard(port, !no_open),
        Commands::Ingest {
            content,
            tags,
            node_type,
            source,
        } => ingest::run_ingest(content, tags, node_type, source),
        Commands::Serve {
            port,
            dashboard,
            dashboard_port,
        } => serve::run_serve(port, dashboard, dashboard_port),
    }
}
