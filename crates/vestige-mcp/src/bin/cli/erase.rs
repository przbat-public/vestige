//! Erase command — GDPR Article 17 hard-deletion of a memory or a tag.
//!
//! The CLI is the third surface over the same core call as the MCP `erase`
//! tool and `POST /api/maintenance/erase`: it builds the same arguments and
//! calls [`vestige_mcp::tools::erase::run`], so the confirmation gate and the
//! exact-tag matching cannot drift between the three.

use colored::Colorize;
use vestige_core::Storage;

use vestige_mcp::tools::erase;

pub(super) fn run_erase(
    id: Option<String>,
    tag: Option<String>,
    dry_run: bool,
    confirm: bool,
) -> anyhow::Result<()> {
    println!("{}", "=== Vestige Erasure (GDPR Art. 17) ===".cyan().bold());
    println!();

    let storage = Storage::new(None)?;

    // clap guarantees exactly one of `--id`/`--tag` (see `Commands::Erase`).
    let action = if id.is_some() { "memory" } else { "tag" };
    let args = serde_json::json!({
        "action": action,
        "id": id,
        "tag": tag,
        "dry_run": dry_run,
        "confirmed": confirm,
    });

    let outcome = match erase::run(&storage, Some(args)) {
        Ok(outcome) => outcome,
        Err(e) => {
            // A refusal is not a crash: print the gate's own message (it names
            // the fix) and exit non-zero so scripts do not treat it as success.
            eprintln!("{} {}", "ERROR:".red().bold(), e);
            anyhow::bail!("erasure was not performed");
        }
    };

    println!("{}: {}", "Scope".white().bold(), outcome.action);
    println!("{}: {}", "Matched memories".white().bold(), outcome.matched);
    if !outcome.ids.is_empty() {
        println!();
        println!("{}", "Memory ids:".yellow().bold());
        for id in &outcome.ids {
            println!("  {}", id.as_str().dimmed());
        }
        if outcome.ids_truncated {
            println!(
                "  {}",
                format!(
                    "... and {} more (id list capped at {})",
                    outcome.matched as usize - outcome.ids.len(),
                    erase::MAX_REPORTED_IDS
                )
                .dimmed()
            );
        }
    }
    println!();

    if outcome.dry_run {
        println!(
            "{}",
            format!(
                "Dry run: {} memories would be erased. Re-run with --confirm to erase them.",
                outcome.matched
            )
            .yellow()
            .bold()
        );
        return Ok(());
    }

    println!(
        "{}",
        format!(
            "Erased {} memories and {} related artifacts (connections, embeddings, access log, \
             states, insights).",
            outcome.matched, outcome.artifacts_erased
        )
        .green()
        .bold()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{Cli, Commands};
    use clap::Parser;

    fn parse(argv: &[&str]) -> Result<Commands, clap::Error> {
        Cli::try_parse_from(argv).map(|cli| cli.command)
    }

    #[test]
    fn erase_requires_exactly_one_target() {
        // Neither: clap refuses before the storage layer is ever opened.
        assert!(parse(&["vestige", "erase"]).is_err());
        // Both: ambiguous scope must not silently win one of the two.
        assert!(parse(&["vestige", "erase", "--id", "abc", "--tag", "gdpr"]).is_err());
        assert!(parse(&["vestige", "erase", "--id", "abc"]).is_ok());
        assert!(parse(&["vestige", "erase", "--tag", "gdpr"]).is_ok());
    }

    #[test]
    fn erase_parses_the_destructive_flags() {
        match parse(&["vestige", "erase", "--id", "abc", "--dry-run"]).unwrap() {
            Commands::Erase {
                id,
                tag,
                dry_run,
                confirm,
            } => {
                assert_eq!(id.as_deref(), Some("abc"));
                assert_eq!(tag, None);
                assert!(dry_run);
                assert!(!confirm, "confirmation must be opt-in, never implied");
            }
            _ => panic!("expected the Erase subcommand"),
        }

        match parse(&["vestige", "erase", "--tag", "gdpr", "--confirm"]).unwrap() {
            Commands::Erase {
                id,
                tag,
                dry_run,
                confirm,
            } => {
                assert_eq!(id, None);
                assert_eq!(tag.as_deref(), Some("gdpr"));
                assert!(!dry_run, "the destructive pass is what --confirm asks for");
                assert!(confirm);
            }
            _ => panic!("expected the Erase subcommand"),
        }
    }

    #[test]
    fn erase_defaults_to_a_non_destructive_call() {
        // No flags at all must map to the safe path: `run_erase` then reaches
        // `erase::run` with dry_run=false and confirmed=false, which refuses.
        match parse(&["vestige", "erase", "--tag", "gdpr"]).unwrap() {
            Commands::Erase {
                dry_run, confirm, ..
            } => {
                assert!(!dry_run);
                assert!(!confirm);
                let args = serde_json::json!({
                    "action": "tag", "tag": "gdpr",
                    "dry_run": dry_run, "confirmed": confirm,
                });
                assert!(
                    !vestige_mcp::tools::common::is_confirmed(&args),
                    "an unflagged CLI call must not satisfy the gate"
                );
            }
            _ => panic!("expected the Erase subcommand"),
        }
    }
}
