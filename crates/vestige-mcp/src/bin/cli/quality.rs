//! Quality command — the wave-5 measurement, for scripts and for people.
//!
//! Two readers, two shapes, one source. `--json` prints
//! [`MemoryQualityReport`] exactly as `Storage::memory_quality` serialises it,
//! because the A/B harness and CI compare runs by diffing those numbers: a CLI
//! that re-shaped them would put a second definition of "self-containment" in
//! the pipeline. The human rendering below adds nothing to the payload — it
//! reads the same counts out and says what each one does *not* mean, which is
//! the half §12.1 is about.
//!
//! The window starts at a day boundary in UTC, not at "now minus N days":
//! comparing two runs means naming the same instant twice, and a report whose
//! start moved with the clock is not reproducible.

use chrono::{DateTime, NaiveDate, Utc};
use colored::Colorize;
use vestige_core::Storage;
use vestige_core::storage::MemoryQualityReport;

pub(super) fn run_quality(since: Option<String>, json: bool) -> anyhow::Result<()> {
    let since = since.as_deref().map(start_of_day).transpose()?;

    let storage = Storage::new(None)?;
    let report = storage.memory_quality(since)?;

    if json {
        // Straight to stdout, unchanged: this is the machine-readable contract.
        // Failing here is an error and not a partial line, because a harness
        // that parsed half a report would compare two different questions.
        let encoded = serde_json::to_string(&report)?;
        println!("{encoded}");
        return Ok(());
    }

    print!("{}", render(&report));
    Ok(())
}

/// Parse `YYYY-MM-DD` as the start of that day in UTC.
///
/// A date that does not parse is an error rather than an absent window: the old
/// behaviour of silently ignoring it answers a different question than the one
/// asked, and the answer looks authoritative because it is a real report — over
/// every memory ever written, labelled with a `since` the caller believes in.
pub(super) fn start_of_day(date: &str) -> anyhow::Result<DateTime<Utc>> {
    let day = NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|e| {
        anyhow::anyhow!("Invalid date '{date}': {e}. Use YYYY-MM-DD (UTC), e.g. 2026-09-20.")
    })?;
    Ok(day
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_utc())
}

/// The four measures, each with its denominator and a reading.
fn render(report: &MemoryQualityReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{}\n\n",
        "=== Vestige Memory Quality (wave 5) ===".cyan().bold()
    ));
    out.push_str(&format!(
        "{} {}\n",
        "Window".white().bold(),
        match &report.since {
            Some(since) => format!("{since} .. {}", report.until),
            None => format!("everything ever written .. {}", report.until),
        }
    ));
    out.push_str(&format!(
        "{} {}\n\n",
        "Basis".white().bold(),
        report.window_basis
    ));

    // 1. Self-containment. The three categories are printed as three numbers —
    // merging `unchecked` into `clean` is the mistake the split exists to stop.
    let c = &report.containment;
    out.push_str(&format!("{}\n", "Self-containment".white().bold()));
    out.push_str(&format!(
        "  {} {} / {} checked   (clean {}, flagged {}, unchecked {})\n",
        "rate".dimmed(),
        rate(c.self_containment_rate()),
        c.clean + c.flagged,
        c.clean,
        c.flagged,
        c.unchecked
    ));
    out.push_str(&format!(
        "  {}\n",
        reading(c.self_containment_rate(), c.clean + c.flagged).dimmed()
    ));
    for rule in &report.flagged_by_kind {
        out.push_str(&format!(
            "  {} {} {}\n",
            "flagged by".dimmed(),
            rule.kind,
            rule.count
        ));
    }

    // 2. Anchors. `unchecked` sits outside the rate on purpose: it is the
    // environment failing to answer, not the code having rotted.
    let a = &report.anchors;
    out.push_str(&format!("\n{}\n", "Anchor resolvability".white().bold()));
    out.push_str(&format!(
        "  {} {} / {} checkable   (fresh {}, stale {}, orphaned {}, unchecked {})\n",
        "rate".dimmed(),
        rate(a.resolvability_rate()),
        a.fresh + a.stale + a.orphaned,
        a.fresh,
        a.stale,
        a.orphaned,
        a.unchecked
    ));
    out.push_str(&format!(
        "  {}\n",
        reading_anchors(
            a.resolvability_rate(),
            a.fresh + a.stale + a.orphaned,
            a.unchecked
        )
        .dimmed()
    ));

    // 3. Use. Two numbers, because a retrieval and a promotion are not the same
    // claim, and this one is a value signal — nothing decays because of it.
    let u = &report.use_counts;
    out.push_str(&format!("\n{}\n", "Use".white().bold()));
    out.push_str(&format!(
        "  {} {} / {} written   (ever accessed {})\n",
        "retrieved".dimmed(),
        rate(u.retrieval_rate()),
        u.total,
        u.ever_accessed
    ));
    out.push_str(&format!(
        "  {}\n",
        reading_use(u.retrieval_rate(), u.retrieved_at_least_once, u.total).dimmed()
    ));

    // 4. Gate outcomes, which are this process's and not the store's.
    let p = &report.process;
    out.push_str(&format!(
        "\n{}\n",
        "Gate outcomes (this process)".white().bold()
    ));
    out.push_str(&format!(
        "  {} {} refused, {} flagged\n",
        "writes".dimmed(),
        p.rejected,
        p.flagged
    ));
    for rule in &p.rejected_by_kind {
        out.push_str(&format!(
            "  {} {} {}\n",
            "refused by".dimmed(),
            rule.kind,
            rule.count
        ));
    }
    for rule in &p.flagged_by_kind {
        out.push_str(&format!(
            "  {} {} {}\n",
            "flagged by".dimmed(),
            rule.kind,
            rule.count
        ));
    }
    out.push_str(&format!(
        "  {}\n",
        "process-lifetime counters: a refused write stores nothing, so a restart starts them at \
         zero and the store cannot contradict them"
            .dimmed()
    ));

    for note in &report.notes {
        out.push_str(&format!("  {}\n", format!("note: {note}").dimmed()));
    }

    out
}

/// A rate with no denominator is no measurement, and printing `0.0%` for it is
/// the one reading of an empty window that is actively wrong.
fn rate(value: Option<f64>) -> String {
    match value {
        Some(rate) => format!("{:.1}%", rate * 100.0),
        None => "n/a".to_string(),
    }
}

fn reading(value: Option<f64>, denominator: i64) -> String {
    let Some(rate) = value else {
        return format!("no measurement: 0 of {denominator} in the window");
    };
    if rate >= 0.9 {
        "most of what was stored stands on its own".to_string()
    } else if rate >= 0.5 {
        "about half needs its conversation; rewrite the flagged ones".to_string()
    } else {
        "most of this window leans on context nobody kept".to_string()
    }
}

/// Use gets its own reading, because the self-containment ladder would libel a
/// young store: zero retrievals is a measurement of *nothing yet*, not a
/// verdict, and `useCounts` are explicitly not a decay signal (§12.1).
fn reading_use(value: Option<f64>, retrieved: i64, total: i64) -> String {
    let Some(rate) = value else {
        return format!("no measurement: 0 of {total} in the window");
    };
    if retrieved == 0 {
        return format!(
            "none of the {total} written here has been retrieved yet — too early to read, and \
             nothing decays because of it"
        );
    }
    format!(
        "{retrieved} of {total} ({:.1}%) were retrieved at least once",
        rate * 100.0
    )
}

fn reading_anchors(value: Option<f64>, checkable: i64, unchecked: i64) -> String {
    let Some(rate) = value else {
        return format!("no measurement: 0 of {checkable} checkable, {unchecked} unchecked");
    };
    let verdict = if rate >= 0.9 {
        "the referenced code still matches what was recorded"
    } else if rate >= 0.5 {
        "some references moved on; the stale ones are the rot to look at"
    } else {
        "most references no longer resolve: the code moved, the memories did not"
    };
    match unchecked {
        0 => verdict.to_string(),
        // Named separately so nobody reads an environment failure as code rot.
        n => format!("{verdict} ({n} anchor(s) nobody could check: no repository or no revision)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cli, Commands};
    use clap::Parser;
    use vestige_core::IngestInput;

    fn parse(argv: &[&str]) -> Result<Commands, clap::Error> {
        Cli::try_parse_from(argv).map(|cli| cli.command)
    }

    /// A store with one of each containment category, so the three are forced to
    /// stay distinct in whatever renders them.
    ///
    /// The markers are set on the input rather than by running the gate: the
    /// gate lives above the storage layer (that a real write lands here is
    /// covered in `smart_ingest::tests`). What is under test in this file is the
    /// rendering, and a fixture with exact counts is what makes the printed
    /// numbers checkable.
    fn store_with_one_of_each(tag: &str) -> (Storage, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "vestige-cli-quality-{tag}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock before epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("vestige.db");
        let storage = Storage::new(Some(path.clone())).expect("open the store");

        let memory = |content: &str, marker: Option<bool>, findings: Option<serde_json::Value>| {
            storage
                .ingest(IngestInput {
                    content: content.to_string(),
                    node_type: "fact".to_string(),
                    self_contained: marker,
                    self_contained_findings: findings,
                    ..Default::default()
                })
                .expect("ingest");
        };
        memory(
            "Marek keeps the migration lock in Postgres",
            Some(true),
            None,
        );
        memory(
            "Deploy the migration next week",
            Some(false),
            Some(serde_json::json!([
                { "kind": "relative_time", "span": "next week", "hint": "use an absolute date" }
            ])),
        );
        // No marker at all: the gate never ran, which is not "clean".
        memory("a memory from before the gate existed", None, None);

        (storage, path)
    }

    #[test]
    fn quality_parses_its_window_and_output_flags() {
        match parse(&["vestige", "quality"]).unwrap() {
            Commands::Quality { since, json } => {
                assert_eq!(since, None, "no window means everything ever written");
                assert!(!json, "the human rendering is the default");
            }
            _ => panic!("expected the Quality subcommand"),
        }

        match parse(&["vestige", "quality", "--since", "2026-09-20", "--json"]).unwrap() {
            Commands::Quality { since, json } => {
                assert_eq!(since.as_deref(), Some("2026-09-20"));
                assert!(json);
            }
            _ => panic!("expected the Quality subcommand"),
        }
    }

    #[test]
    fn a_since_date_is_the_start_of_that_day_in_utc() {
        let at = start_of_day("2026-09-20").unwrap();
        assert_eq!(at.to_rfc3339(), "2026-09-20T00:00:00+00:00");
        assert_eq!(at.timezone(), Utc);
    }

    /// An unparseable date has to stop the command: a report over the whole
    /// store, labelled with a window the caller believes in, is worse than no
    /// report at all.
    #[test]
    fn an_unparseable_date_is_an_error_not_a_missing_window() {
        for bad in ["2026-13-01", "20-09-2026", "yesterday", "2026/09/20", ""] {
            assert!(
                start_of_day(bad).is_err(),
                "{bad:?} must not be read as a window"
            );
        }
        // The error has to name the format, or the caller's next try is a guess.
        let err = start_of_day("20-09-2026").unwrap_err().to_string();
        assert!(err.contains("YYYY-MM-DD"), "unhelpful message: {err}");
    }

    /// Not a test — a store for looking at the command by hand.
    ///
    /// `VESTIGE_QUALITY_DEMO=/tmp/x cargo test -p vestige-mcp --bin vestige --
    /// --ignored --nocapture demo` writes the fixture below to
    /// `/tmp/x/com.vestige.core/vestige.db`, which is then scannable with
    /// `HOME=/tmp/x ./target/debug/vestige quality`. The store is built through
    /// the same calls the tests use; only the print is extra, because reading
    /// the rendering is the point.
    #[test]
    #[ignore = "writes a demo store for looking at the output by hand"]
    fn build_a_demo_store_for_the_command() {
        let root = std::env::var("VESTIGE_QUALITY_DEMO")
            .expect("set VESTIGE_QUALITY_DEMO to the HOME the command will run with");
        let data = std::path::Path::new(&root).join("com.vestige.core");
        std::fs::create_dir_all(&data).expect("create the store directory");
        let path = data.join("vestige.db");
        let _ = std::fs::remove_file(&path);

        let storage = Storage::new(Some(path.clone())).expect("open the store");
        let memory = |content: &str, marker: Option<bool>, findings: Option<serde_json::Value>| {
            storage
                .ingest(IngestInput {
                    content: content.to_string(),
                    node_type: "fact".to_string(),
                    self_contained: marker,
                    self_contained_findings: findings,
                    ..Default::default()
                })
                .expect("ingest");
        };
        memory(
            "Marek keeps the migration lock in Postgres",
            Some(true),
            None,
        );
        memory("Vestige records one revision per write", Some(true), None);
        memory(
            "Deploy the migration next week",
            Some(false),
            Some(serde_json::json!([
                { "kind": "relative_time", "span": "next week", "hint": "use an absolute date" },
                { "kind": "no_subject", "span": "Deploy the", "hint": "say what this is about" }
            ])),
        );
        memory(
            "BUG FIX: naprawiłem to, co omawialiśmy",
            Some(false),
            Some(serde_json::json!([
                { "kind": "discourse_deixis", "span": "omawialiśmy", "hint": "nazwij, co było omawiane" }
            ])),
        );
        // No marker: the gate never ran, which is not "clean".
        memory("a memory from before the gate existed", None, None);
        drop(storage);

        let storage = Storage::new(Some(path)).expect("reopen the store");
        let report = storage.memory_quality(None).expect("measure");
        println!("{}", render(&report));
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    }

    /// The window is the start of the named day, so a run from yesterday can be
    /// reproduced today; a window that is really the whole store looks like a
    /// measurement of it.
    #[test]
    fn the_since_window_excludes_what_was_recorded_before_that_day() {
        let (storage, _path) = store_with_one_of_each("window");
        let since = start_of_day("2000-01-01").unwrap();
        assert_eq!(
            storage
                .memory_quality(Some(since))
                .unwrap()
                .containment
                .total,
            3
        );

        let tomorrow = Utc::now().date_naive() + chrono::Days::new(1);
        let empty = storage
            .memory_quality(Some(start_of_day(&tomorrow.to_string()).unwrap()))
            .unwrap();
        assert_eq!(
            empty.containment.total, 0,
            "an empty window is not a window"
        );
    }

    /// What a person sees. The three categories are asserted as three printed
    /// numbers, because the failure this rendering has to prevent is one of them
    /// quietly absorbing another.
    #[test]
    fn the_human_rendering_names_each_denominator_and_reading() {
        let (storage, _path) = store_with_one_of_each("render");
        let report = storage.memory_quality(None).unwrap();
        let rendered = render(&report);

        // 1 clean of 2 checked is 50.0%, and the third memory is neither.
        assert!(rendered.contains("50.0%"), "{rendered}");
        assert!(rendered.contains("/ 2 checked"), "{rendered}");
        assert!(
            rendered.contains("clean 1, flagged 1, unchecked 1"),
            "the three categories must print apart: {rendered}"
        );
        assert!(
            rendered.contains("relative_time"),
            "the rule breakdown has to reach the reader: {rendered}"
        );
        // No anchors and no retrievals in this store: no rate, said in words.
        assert!(
            rendered.contains("n/a"),
            "an empty denominator is not 0.0%: {rendered}"
        );
        assert!(
            rendered.contains("no measurement"),
            "the reading has to say why the rate is missing: {rendered}"
        );
        // The process counters are labelled as the process's, not the store's.
        assert!(
            rendered.contains("process-lifetime") && rendered.contains("this process"),
            "{rendered}"
        );
        for note in &report.notes {
            assert!(
                rendered.contains(note),
                "the notes reach the reader too: {rendered}"
            );
        }
    }
}
