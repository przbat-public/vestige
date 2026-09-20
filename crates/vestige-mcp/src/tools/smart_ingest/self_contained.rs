//! Self-containedness gate — will this memory still make sense to a reader who
//! was not in the conversation that produced it?
//!
//! Why this exists: the failure it catches is not that search retrieves the
//! wrong thing, it is that what was stored cannot be understood at all six
//! months later. Measured evidence and the reasoning behind each rule live in
//! `docs/SELF-CONTAINED-MEMORY-DESIGN.md` §3 and §11; the short version:
//!
//! - Every memory system surveyed writes the rule "a memory must be
//!   understandable on its own" into its prompts and none of them checks it, so
//!   the check is the missing half.
//! - `coref.rs` resolves pronouns *inside* the ingested text. When the
//!   antecedent was in the conversation, no write-time rewrite can recover it,
//!   which is exactly the reported complaint — so the gate has to notice and
//!   say so rather than pretend the text is complete.
//! - A stored file path is fail-silent in both directions: after a version bump
//!   it keeps resolving to the old copy with nothing signalling it, and a pruned
//!   target fails as total silent loss. It breaks into wrongness or into
//!   nothing, never into an error.
//!
//! Deliberately conservative: the gate warns, it does not hide. Only content
//! that the repository already owns is rejected (`Reject`), because trading junk
//! for silent loss would be a worse failure than the one being fixed.

use serde_json::json;

/// Rule identifiers, stable because they are part of the tool response.
pub(super) const KIND_DISCOURSE_DEIXIS: &str = "discourse_deixis";
pub(super) const KIND_UNRESOLVED_PRONOUN: &str = "unresolved_pronoun";
pub(super) const KIND_RELATIVE_TIME: &str = "relative_time";
pub(super) const KIND_BARE_CODE_REFERENCE: &str = "bare_code_reference";
pub(super) const KIND_NO_SUBJECT: &str = "no_subject";

/// One reason a memory will not stand on its own, with the text that triggered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Finding {
    pub kind: &'static str,
    /// The exact substring that fired the rule, so the caller can see what to fix.
    pub span: String,
    pub hint: &'static str,
}

/// Outcome of the gate. `findings` means "written, but will need context";
/// `reject` means "this should not be stored at all".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Report {
    pub findings: Vec<Finding>,
    pub reject: Option<&'static str>,
}

impl Report {
    /// True when the memory is stored but flagged as needing its conversation.
    pub(super) fn requires_context(&self) -> bool {
        !self.findings.is_empty()
    }

    /// True when the memory must not be written at all.
    pub(super) fn is_rejected(&self) -> bool {
        self.reject.is_some()
    }

    /// Wire shape. Kept as an object rather than a sentence so a caller can act
    /// on individual findings instead of parsing prose.
    ///
    /// The reject field is named `rejectRecommended` on purpose: this commit
    /// computes and reports the recommendation, while enforcement (the gate
    /// refusing to write) is a separate change. A field called `rejected` that
    /// is set on a memory which was in fact written would be exactly the kind of
    /// false claim this whole review set out to remove.
    pub(super) fn to_json(&self) -> serde_json::Value {
        json!({
            "ok": !self.requires_context() && !self.is_rejected(),
            "requiresContext": self.requires_context(),
            "rejectRecommended": self.is_rejected(),
            "rejectReason": self.reject,
            "findings": self
                .findings
                .iter()
                .map(|f| json!({ "kind": f.kind, "span": f.span, "hint": f.hint }))
                .collect::<Vec<_>>(),
        })
    }
}

/// Phrases that point at something outside the memory itself. These are noun
/// phrases, not pronouns — which is why the existing coreference pass cannot see
/// them.
///
/// Split in two, because the two halves are not equally dangling. A *strong*
/// reference names the conversation as its antecedent ("as we discussed", "we
/// decided"), and nothing inside the memory can resolve it — it is flagged
/// unconditionally. A *weak* reference ("the fix", "the file") is perfectly
/// readable when the same memory names what it is about, so it is flagged only
/// when no subject is present. Treating them alike produced a rule that a single
/// capitalised word at the start of a line ("Files:") could switch off.
const DISCOURSE_DEIXIS_STRONG: &[(&str, &str)] = &[
    ("as discussed", "name what was discussed"),
    ("as mentioned", "name what was mentioned"),
    ("as agreed", "name what was agreed"),
    ("we decided", "name who decided and what"),
    ("last time", "give the date instead of 'last time'"),
    ("earlier today", "give the date instead of 'earlier today'"),
    ("the above", "restate what 'the above' refers to"),
    ("jak ustaliliśmy", "nazwij, co zostało ustalone"),
    ("jak omawialiśmy", "nazwij, co było omawiane"),
    ("omawialiśmy", "nazwij, co było omawiane"),
    ("ustaliliśmy", "nazwij, co zostało ustalone"),
    ("poprzednio", "nazwij, czego dotyczy „poprzednio”"),
    ("powyższe", "powtórz, czego dotyczy „powyższe”"),
];

/// Weak references: dangling only when the memory names no subject.
const DISCOURSE_DEIXIS_WEAK: &[(&str, &str)] = &[
    ("the fix", "name what was fixed"),
    ("the bug", "name the bug"),
    ("the issue", "name the issue"),
    ("the change", "name the change"),
    ("the file", "name the file"),
    ("the function", "name the function"),
    ("the test", "name the test"),
    ("the problem", "name the problem"),
    ("ta poprawka", "nazwij, co zostało poprawione"),
];

/// Third-person pronouns and bare demonstrative subjects. Only consulted when
/// the content names no subject; "it" alone is deliberately absent because it is
/// ordinary English prose far more often than it is a dangling reference.
const PRONOUNS: &[&str] = &[
    "he ", "she ", "they ", "him ", "her ", "them ", "his ", "hers ", "their ",
];
const BARE_DEMONSTRATIVES: &[&str] = &[
    "this is",
    "this was",
    "this means",
    "this fixes",
    "this breaks",
    "that is",
    "that was",
    "that means",
    "that fixes",
    "that breaks",
];

/// Relative time that rots the moment the memory is read later. Fires only when
/// the content carries no absolute date — an anchored memory is fine.
const RELATIVE_TIME: &[(&str, &str)] = &[
    ("today", "use an absolute date"),
    ("yesterday", "use an absolute date"),
    ("tomorrow", "use an absolute date"),
    ("recently", "use an absolute date"),
    ("soon", "use an absolute date"),
    ("last week", "use an absolute date"),
    ("next week", "use an absolute date"),
    ("last month", "use an absolute date"),
    ("next month", "use an absolute date"),
    ("dzisiaj", "użyj absolutnej daty"),
    ("wczoraj", "użyj absolutnej daty"),
    ("jutro", "użyj absolutnej daty"),
    ("w tym tygodniu", "użyj absolutnej daty"),
    ("w przyszłym tygodniu", "użyj absolutnej daty"),
];

/// Extensions that mark a token as a code reference rather than prose.
const CODE_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".md", ".json", ".toml", ".yml", ".yaml", ".sql",
    ".sh", ".go", ".java", ".rb", ".c", ".h", ".cpp",
];

/// Path prefixes that only make sense inside a checkout.
const CODE_PREFIXES: &[&str] = &[
    "src/",
    "crates/",
    "apps/",
    "tests/",
    "docs/",
    "packages/",
    "scripts/",
    "./src/",
];

/// First-person markers: content that speaks as the author is self-locating even
/// without a proper noun, so it must not be flagged as subject-less.
const FIRST_PERSON: &[&str] = &[" i ", " i'", "we ", "we'", "my ", "our ", "me ", "us "];

/// Rule-shaped content (a lesson to apply later) is the target output of this
/// system. It has no subject and no entity by nature, so the subject rule must
/// exempt it — a gate that flags "don't simulate the database in integration
/// tests" would be worse than no gate.
const RULE_MARKERS: &[&str] = &[
    "never ", "always ", "must ", "should ", "prefer ", "don't ", "do not ", "avoid ", "nie ",
    "zawsze ", "nigdy ", "należy ", "unikaj ",
];

/// Statements the repository already owns. These are rejected rather than
/// flagged: a version number or a file dump is derivable, and storing it buys a
/// copy that rots with every commit.
const DERIVABLE_MARKERS: &[&str] = &[
    "the architecture",
    "project structure",
    "directory layout",
    "folder structure",
    "coverage is",
    "architektura projektu",
    "struktura katalogów",
];

/// Code-shaped line prefixes — a memory that contains source code is a copy of
/// something the repository already stores better.
const CODE_LINE_PREFIXES: &[&str] = &[
    "fn ",
    "pub fn ",
    "impl ",
    "struct ",
    "enum ",
    "use ",
    "let ",
    "import ",
    "export ",
    "class ",
    "def ",
    "function ",
    "const ",
    "#!/",
];

/// Detect why `content` will not stand on its own.
///
/// `has_anchored_time` is the caller's knowledge that temporal anchoring already
/// attached an absolute time to this memory; it suppresses the relative-time
/// rule, because an anchored "next Friday" is a solved problem rather than a
/// dangling one.
pub(super) fn detect(content: &str, has_anchored_time: bool) -> Report {
    let mut report = Report::default();
    let lower = content.to_lowercase();
    let subject = has_named_subject(content);
    let rule_shaped = RULE_MARKERS.iter().any(|m| lower.contains(m));

    if let Some(reason) = derivable_reason(content) {
        report.reject = Some(reason);
    }

    for (phrase, hint) in DISCOURSE_DEIXIS_STRONG {
        if lower.contains(phrase) {
            report.findings.push(Finding {
                kind: KIND_DISCOURSE_DEIXIS,
                span: (*phrase).to_string(),
                hint,
            });
        }
    }

    for (phrase, hint) in DISCOURSE_DEIXIS_WEAK {
        // Only dangling when nothing in the same memory could be the antecedent.
        if lower.contains(phrase) && !subject {
            report.findings.push(Finding {
                kind: KIND_DISCOURSE_DEIXIS,
                span: (*phrase).to_string(),
                hint,
            });
        }
    }

    if !subject {
        for pronoun in PRONOUNS {
            if lower.contains(pronoun) {
                report.findings.push(Finding {
                    kind: KIND_UNRESOLVED_PRONOUN,
                    span: (*pronoun).trim().to_string(),
                    hint: "name the person the pronoun refers to",
                });
                break;
            }
        }
        for marker in BARE_DEMONSTRATIVES {
            if lower.contains(marker) {
                report.findings.push(Finding {
                    kind: KIND_UNRESOLVED_PRONOUN,
                    span: (*marker).to_string(),
                    hint: "name what the demonstrative points at",
                });
                break;
            }
        }
    }

    if !has_anchored_time && !has_absolute_date(content) {
        for (phrase, hint) in RELATIVE_TIME {
            if lower.contains(phrase) {
                report.findings.push(Finding {
                    kind: KIND_RELATIVE_TIME,
                    span: (*phrase).to_string(),
                    hint,
                });
                break;
            }
        }
    }

    if let Some(span) = bare_code_reference(content) {
        report.findings.push(Finding {
            kind: KIND_BARE_CODE_REFERENCE,
            span,
            hint: "anchor it as path@commit#symbol instead of a position",
        });
    }

    if !subject && !rule_shaped && !has_first_person(&lower) && !report.is_rejected() {
        report.findings.push(Finding {
            kind: KIND_NO_SUBJECT,
            span: first_words(content, 6),
            hint: "say what or whom this memory is about",
        });
    }

    report
}

/// True when the text names something a later reader can look up: a proper noun,
/// a URL, an email or a monetary amount. File paths deliberately do not count —
/// a path is an anchor, not a subject.
fn has_named_subject(content: &str) -> bool {
    #[cfg(feature = "preprocessing")]
    {
        use vestige_core::preprocessing::entities::{EntityType, extract_entities};
        if extract_entities(content, 8).iter().any(|e| {
            matches!(
                e.entity_type,
                EntityType::ProperNoun
                    | EntityType::Person
                    | EntityType::Organization
                    | EntityType::Url
                    | EntityType::Email
                    | EntityType::Monetary
            )
        }) {
            return true;
        }
    }
    #[cfg(not(feature = "preprocessing"))]
    {
        // Without the extractor, a capitalised word that is not sentence-initial
        // is the cheapest available proxy for a proper noun.
        let mut words = content.split_whitespace();
        let _first = words.next();
        if words.any(|w| {
            let cleaned = w.trim_matches(|c: char| !c.is_alphanumeric());
            cleaned.chars().count() > 2
                && cleaned.chars().next().is_some_and(|c| c.is_uppercase())
                && !cleaned.chars().all(|c| c.is_uppercase())
        }) {
            return true;
        }
    }
    false
}

/// An absolute date survives being read later; a relative one does not.
fn has_absolute_date(content: &str) -> bool {
    let bytes = content.as_bytes();
    let mut digits_in_a_row = 0usize;
    for &b in bytes {
        if b.is_ascii_digit() {
            digits_in_a_row += 1;
            // A four-digit run is a year in every format this system writes.
            if digits_in_a_row >= 4 {
                return true;
            }
        } else {
            digits_in_a_row = 0;
        }
    }
    false
}

fn has_first_person(lower: &str) -> bool {
    let padded = format!(" {lower} ");
    FIRST_PERSON.iter().any(|m| padded.contains(m))
}

/// A token that points into the checkout without saying which revision it meant.
fn bare_code_reference(content: &str) -> Option<String> {
    for raw in content.split_whitespace() {
        let token = raw.trim_matches(|c: char| {
            !c.is_alphanumeric() && !matches!(c, '/' | '.' | '_' | '-' | ':')
        });
        if token.is_empty() {
            continue;
        }
        // An anchored reference is the fix, not the problem: `path@sha#symbol`.
        if token.contains('@') && token.contains('#') {
            continue;
        }
        let lower = token.to_lowercase();
        let looks_like_path = CODE_PREFIXES.iter().any(|p| lower.starts_with(p))
            || CODE_EXTENSIONS.iter().any(|e| lower.ends_with(e))
            || lower.split_once(':').is_some_and(|(head, tail)| {
                tail.chars().all(|c| c.is_ascii_digit())
                    && !tail.is_empty()
                    && CODE_EXTENSIONS.iter().any(|e| head.ends_with(e))
            });
        if looks_like_path {
            return Some(token.to_string());
        }
    }
    None
}

/// Statements the repository already owns, which must not be copied into memory.
fn derivable_reason(content: &str) -> Option<&'static str> {
    let lower = content.to_lowercase();

    if content.contains("```") {
        return Some("a code block — the repository already stores the code");
    }
    if content.contains("├──") || content.contains("└──") {
        return Some("a directory tree — derivable from the repository");
    }

    let code_lines = content
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            CODE_LINE_PREFIXES.iter().any(|p| t.starts_with(p))
        })
        .count();
    if code_lines >= 2 {
        return Some("copied source code — the repository already stores it");
    }

    if DERIVABLE_MARKERS.iter().any(|m| lower.contains(m)) {
        return Some("a description of the codebase — derivable from the repository");
    }

    // Version strings and metrics rot with every commit.
    if lower.contains("coverage") && lower.contains('%') {
        return Some("a coverage figure — stale after the next commit");
    }
    if version_like(content) {
        return Some("a version number — stale after the next release");
    }

    None
}

/// `v1.2.3` or `Name 16` — a version claim that will be wrong soon.
fn version_like(content: &str) -> bool {
    let tokens: Vec<&str> = content.split_whitespace().collect();
    for (i, token) in tokens.iter().enumerate() {
        let cleaned = token.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
        let parts: Vec<&str> = cleaned.split('.').collect();
        if parts.len() >= 2
            && parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
            && (cleaned.starts_with('v') || i > 0)
        {
            return true;
        }
        // "Postgres 16" — a capitalised product followed by a bare number.
        if i > 0
            && cleaned.chars().all(|c| c.is_ascii_digit())
            && tokens[i - 1]
                .chars()
                .next()
                .is_some_and(|c| c.is_uppercase())
        {
            return true;
        }
    }
    false
}

fn first_words(content: &str, count: usize) -> String {
    content
        .split_whitespace()
        .take(count)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(report: &Report) -> Vec<&'static str> {
        report.findings.iter().map(|f| f.kind).collect()
    }

    /// The reported complaint, verbatim in shape: a fix that only makes sense if
    /// you were in the conversation, with the position instead of an anchor.
    #[test]
    fn a_bug_fix_from_a_conversation_is_flagged_but_still_written() {
        let report = detect(
            "BUG FIX: naprawiłem to, co omawialiśmy; Files: src/search.rs:112",
            false,
        );

        assert!(kinds(&report).contains(&KIND_DISCOURSE_DEIXIS));
        assert!(kinds(&report).contains(&KIND_BARE_CODE_REFERENCE));
        assert!(
            !report.is_rejected(),
            "a fixable memory must be written and flagged, never hidden"
        );
        assert!(report.requires_context());
    }

    /// A pronoun with no antecedent in the same memory is exactly the case a
    /// write-time rewriter cannot repair.
    #[test]
    fn a_pronoun_without_an_antecedent_is_reported() {
        let dangling = detect("He said the migration failed on the staging box", false);
        assert!(kinds(&dangling).contains(&KIND_UNRESOLVED_PRONOUN));

        let resolved = detect("Marek said the migration failed on the staging box", false);
        assert!(
            !kinds(&resolved).contains(&KIND_UNRESOLVED_PRONOUN),
            "a named antecedent must silence the pronoun rule"
        );
    }

    /// Relative time is fine once temporal anchoring has attached a real date.
    #[test]
    fn relative_time_is_reported_only_without_an_anchor() {
        let unanchored = detect("Deploy the migration tomorrow", false);
        assert!(kinds(&unanchored).contains(&KIND_RELATIVE_TIME));

        let anchored = detect("Deploy the migration tomorrow", true);
        assert!(!kinds(&anchored).contains(&KIND_RELATIVE_TIME));

        let dated = detect("Deploy the migration on 2026-09-20", false);
        assert!(!kinds(&dated).contains(&KIND_RELATIVE_TIME));
    }

    /// What the repository already owns must not be copied into memory.
    #[test]
    fn content_the_repository_owns_is_rejected() {
        let dump = detect("```rust\nfn main() { println!(\"hi\"); }\n```", false);
        assert!(dump.is_rejected());

        let version = detect("The stack is Postgres 16 with Redis 7", false);
        assert!(
            version.is_rejected(),
            "a version number is stale after the next release"
        );

        let tree = detect("Layout:\n├── src\n└── tests", false);
        assert!(tree.is_rejected());
    }

    /// The counter-case that keeps the gate honest: a lesson is the target
    /// output of this system, and flagging it would make the gate worse than
    /// having none.
    #[test]
    fn a_lesson_shaped_memory_is_not_flagged() {
        let report = detect(
            "Nie symuluj bazy w testach integracyjnych, bo testy przechodziły, a migracja padła",
            false,
        );

        assert!(
            !report.requires_context() && !report.is_rejected(),
            "a rule to apply later is exactly what should be stored: {:?}",
            report
        );
    }

    /// An anchored reference is the fix, so it must not itself be flagged.
    #[test]
    fn an_anchored_code_reference_is_not_flagged() {
        let report = detect(
            "Marek found the merge bug in crates/vestige-core/src/search/decompose.rs@1cfdf45#merge_results",
            true,
        );

        assert!(
            !kinds(&report).contains(&KIND_BARE_CODE_REFERENCE),
            "path@sha#symbol is the anchored form: {:?}",
            report
        );
    }

    /// A memory about a named thing, with no reference to anything outside it,
    /// must pass cleanly — otherwise the gate becomes noise and gets ignored.
    #[test]
    fn a_self_contained_memory_passes_cleanly() {
        let report = detect(
            "Vestige merges sub-query search results with a max-score union and breaks ties on the memory id, so the same query returns the same order on every run",
            true,
        );

        assert_eq!(report, Report::default(), "unexpected findings: {report:?}");
    }

    /// The rule that rejected this content is about version claims, and the
    /// point of the case is that a version is stale after the next release —
    /// so the gate is right and the memory should be rewritten.
    #[test]
    fn a_version_claim_inside_an_otherwise_good_memory_is_still_rejected() {
        let report = detect(
            "Vestige merges sub-query results with a max-score union since pipeline 1.0",
            true,
        );

        assert!(report.is_rejected(), "{report:?}");
    }

    /// The wire shape is part of the tool contract, so it is pinned.
    #[test]
    fn the_report_serialises_with_kinds_and_hints() {
        let report = detect("BUG FIX: naprawiłem to, co omawialiśmy", false);
        let value = report.to_json();

        assert_eq!(value["ok"], false);
        assert_eq!(value["requiresContext"], true);
        assert_eq!(value["rejectRecommended"], false);
        assert!(value["findings"].as_array().is_some_and(|a| !a.is_empty()));
        assert!(value["findings"][0]["kind"].is_string());
        assert!(value["findings"][0]["hint"].is_string());
    }
}
