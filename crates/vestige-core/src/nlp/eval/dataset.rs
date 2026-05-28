//! Dataset primitives — `Example`, `Label`, `Dataset`.
//!
//! Kept deliberately small and value-typed so datasets can live as
//! `const`-buildable Rust data. Loading from JSON / CSV would be flexible
//! but introduces a parse step on every test run and a moving filesystem
//! dependency in CI — neither earn their cost for our scale (low hundreds
//! of examples per task).

use crate::nlp::language::Language;

/// Ground-truth label for a binary detection task.
///
/// Variants are deliberately verbose (`Positive` / `Negative`) instead of
/// `true` / `false` — readability matters when a test author looks at a
/// 50-line dataset definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Label {
    /// The detector should classify this example as the positive class
    /// (contradiction / opinion / future-relevant).
    Positive,
    /// The detector should classify this example as the negative class.
    Negative,
}

impl Label {
    /// Convert to a `bool` for metric computation.
    pub fn is_positive(&self) -> bool {
        matches!(self, Label::Positive)
    }
}

/// A single labeled example.
///
/// Two-field structure for contradiction detection (a `(new, old)` pair);
/// single-field for opinion / future-relevance (just `text`). To avoid
/// two struct types we use [`Example::pair`] with `text_a` and `text_b`,
/// with the convention that single-text tasks set `text_b = ""`.
#[derive(Debug, Clone)]
pub struct Example {
    /// Stable identifier for traceability — when a test fails, this is
    /// the way you find the offending example in the dataset file.
    pub id: &'static str,
    /// The primary text (for contradiction: the *new* memory).
    pub text_a: &'static str,
    /// The secondary text (for contradiction: the *old* memory).
    /// Empty for single-text tasks.
    pub text_b: &'static str,
    /// Language of the example, used for per-language breakdowns.
    pub language: Language,
    /// Ground-truth label.
    pub label: Label,
    /// Optional rationale — why the label is what it is. Surfaces in eval
    /// failure messages.
    pub rationale: &'static str,
}

/// A named collection of examples.
#[derive(Debug, Clone)]
pub struct Dataset {
    /// Dataset name (e.g. `"contradiction_v1"`).
    pub name: &'static str,
    /// Concise description of what the dataset covers, for the report.
    pub description: &'static str,
    /// The examples themselves.
    pub examples: &'static [Example],
}

impl Dataset {
    /// Number of examples.
    pub fn len(&self) -> usize {
        self.examples.len()
    }

    /// Whether the dataset is empty.
    pub fn is_empty(&self) -> bool {
        self.examples.is_empty()
    }

    /// Count of examples per [`Language`].
    pub fn count_by_language(&self) -> std::collections::HashMap<Language, usize> {
        let mut counts = std::collections::HashMap::new();
        for ex in self.examples {
            *counts.entry(ex.language).or_insert(0) += 1;
        }
        counts
    }

    /// Count of examples per [`Label`].
    pub fn count_by_label(&self) -> std::collections::HashMap<Label, usize> {
        let mut counts = std::collections::HashMap::new();
        for ex in self.examples {
            *counts.entry(ex.label).or_insert(0) += 1;
        }
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static SAMPLE: &[Example] = &[
        Example {
            id: "ex1",
            text_a: "a",
            text_b: "b",
            language: Language::English,
            label: Label::Positive,
            rationale: "test",
        },
        Example {
            id: "ex2",
            text_a: "c",
            text_b: "d",
            language: Language::Polish,
            label: Label::Negative,
            rationale: "test",
        },
        Example {
            id: "ex3",
            text_a: "e",
            text_b: "",
            language: Language::English,
            label: Label::Positive,
            rationale: "test",
        },
    ];

    #[test]
    fn dataset_len_and_empty() {
        let ds = Dataset {
            name: "test",
            description: "test",
            examples: SAMPLE,
        };
        assert_eq!(ds.len(), 3);
        assert!(!ds.is_empty());
    }

    #[test]
    fn count_by_language() {
        let ds = Dataset {
            name: "test",
            description: "test",
            examples: SAMPLE,
        };
        let counts = ds.count_by_language();
        assert_eq!(counts[&Language::English], 2);
        assert_eq!(counts[&Language::Polish], 1);
    }

    #[test]
    fn count_by_label() {
        let ds = Dataset {
            name: "test",
            description: "test",
            examples: SAMPLE,
        };
        let counts = ds.count_by_label();
        assert_eq!(counts[&Label::Positive], 2);
        assert_eq!(counts[&Label::Negative], 1);
    }

    #[test]
    fn label_is_positive() {
        assert!(Label::Positive.is_positive());
        assert!(!Label::Negative.is_positive());
    }
}
