//! Finding a symbol's body in a file's text, and hashing it.
//!
//! Why the body and not the file: a memory that says "`Storage::
//! embeddings_fingerprint` is the function that decides whether the HNSW sidecar
//! is reused" is still true after someone edits the twenty lines above it, and
//! still true after rustfmt reflows it. Hashing the whole file would mark that
//! memory stale on every unrelated commit, and a verdict that fires on
//! everything carries no information — a reader would learn to ignore it. So the
//! check resolves the **symbol first** and hashes **its text second**; that
//! ordering is the whole design, and it lives here (see [`find_symbol_body`])
//! and in `resolve::check` where the two calls are composed.
//!
//! This is a deterministic brace-matching scanner, not a parser: no grammar, no
//! model, no dependency. It understands comments, string literals and Rust raw
//! strings well enough that a brace inside them does not end a body early, and
//! it treats a lifetime (`&'a str`) as a lifetime rather than as an unterminated
//! char literal. The limits are real and worth stating: a declaration written in
//! a style it does not recognise is reported as *not found*, which surfaces as
//! `orphaned` — a false alarm a human can see and fix — rather than as a
//! confident answer about the wrong code.

use git2::{ObjectType, Oid};

/// The body of `symbol` in `text`, if it can be found unambiguously.
///
/// `symbol` is either a bare name (`embeddings_fingerprint`) or a
/// fully-qualified one (`Storage::embeddings_fingerprint`, `a::b::Type::method`,
/// in which case the last two segments are used). Doc comments and attributes
/// above a declaration are deliberately *not* part of the body: editing a
/// comment should not make a memory stale, and moving a symbol together with its
/// documentation should not either.
pub fn find_symbol_body<'a>(text: &'a str, symbol: &str) -> Option<&'a str> {
    let segments: Vec<&str> = symbol
        .split("::")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let (container, name) = match segments.len() {
        0 => return None,
        1 => (None, segments[0]),
        n => (Some(segments[n - 2]), segments[n - 1]),
    };
    if !is_identifier(name) {
        return None;
    }

    let sig = significant_chars(text);
    let candidates = declarations(text, &sig, name);

    let span = match container {
        Some(container) => {
            // Inside `impl Container` / `impl Trait for Container` first. When
            // several such blocks define the same name, the first is taken:
            // Rust rejects two inherent impls of one type defining one name, so
            // a second match is a trait impl (`impl Default for Container`) and
            // the inherent one is the declaration the caller meant. The
            // container-less fallback below is stricter, because there nothing
            // tells us which of two same-named items was meant.
            let blocks = impl_blocks(text, &sig, container);
            candidates
                .iter()
                .find(|(start, _, _)| {
                    blocks
                        .iter()
                        .any(|(body_start, body_end)| start >= body_start && start < body_end)
                })
                .or_else(|| (candidates.len() == 1).then(|| &candidates[0]))
                .copied()
        }
        // A bare name: prefer the item at file scope, then accept a single
        // match anywhere. Two same-named items at different depths is exactly
        // the case where guessing produces a hash of the wrong function.
        None => candidates
            .iter()
            .find(|(_, _, depth)| *depth == 0)
            .or_else(|| (candidates.len() == 1).then(|| &candidates[0]))
            .copied(),
    }?;

    text.get(span.0..span.1)
}

/// Hash a symbol body for storage in `code_refs.content_hash`.
///
/// Whitespace runs are collapsed first, so reindenting or reflowing a body does
/// not invalidate a memory while a change to any token in it does. The hash is
/// git's own blob hash (`sha1("blob <len>\0" + text)`), which is stable across
/// Rust releases — `DefaultHasher` is explicitly not, and a persisted hash that
/// moves with the compiler would mark the whole store stale on an upgrade.
pub fn hash_body(body: &str) -> String {
    let normalized = normalize_whitespace(body);
    Oid::hash_object(ObjectType::Blob, normalized.as_bytes())
        .map(|oid| oid.to_string())
        .unwrap_or_default()
}

fn normalize_whitespace(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut pending_space = false;
    for ch in body.chars() {
        if ch.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

fn is_identifier(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Keywords that introduce a *named* declaration in the languages this store
/// sees. `let`/`var` are absent on purpose: a local binding is not a symbol a
/// memory can be about, and matching one would resolve to the wrong text.
const DECL_KEYWORDS: &[&str] = &[
    "fn",
    "struct",
    "enum",
    "trait",
    "impl",
    "const",
    "static",
    "type",
    "mod",
    "union",
    "macro_rules",
    "def",
    "class",
    "function",
    "interface",
    "namespace",
    "record",
];

/// Declarations of `name`: `(body_start, body_end, brace_depth_at_declaration)`.
fn declarations(text: &str, sig: &[(usize, char)], name: &str) -> Vec<(usize, usize, usize)> {
    let mut found = Vec::new();
    for (line_start, line_end) in lines(text) {
        let line = &text[line_start..line_end];
        if !declares(line, name) {
            continue;
        }
        let Some(name_at) = find_identifier(line, name) else {
            continue;
        };
        let decl_start = line_start + name_at;
        if let Some(end) = body_end_from(text, sig, decl_start) {
            let depth = depth_at(sig, decl_start);
            found.push((line_start, end, depth));
        }
    }
    found
}

/// True when `line` introduces a declaration of `name`.
fn declares(line: &str, name: &str) -> bool {
    let tokens = split_identifiers(line);
    let Some(name_at) = tokens.iter().position(|t| *t == name) else {
        return false;
    };
    tokens[..name_at].iter().any(|t| DECL_KEYWORDS.contains(t))
}

/// Identifier-shaped tokens of a line: `pub fn foo<T>(` -> `[pub, fn, foo, T]`.
fn split_identifiers(line: &str) -> Vec<&str> {
    line.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
        .collect()
}

/// Byte offset of `name` as a whole identifier, so `find("new")` cannot land
/// inside `renew`.
fn find_identifier(line: &str, name: &str) -> Option<usize> {
    let mut from = 0usize;
    while let Some(at) = line[from..].find(name).map(|i| i + from) {
        if is_word_boundary(line, at, name.len()) {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

/// The byte ranges of `impl` block bodies whose header names `container`.
fn impl_blocks(text: &str, sig: &[(usize, char)], container: &str) -> Vec<(usize, usize)> {
    let mut blocks = Vec::new();
    let mut search_from = 0usize;
    while let Some(at) = text[search_from..].find("impl").map(|i| i + search_from) {
        search_from = at + 4;
        if !is_word_boundary(text, at, 4) {
            continue;
        }
        // The header ends at the block's opening brace; anything before it that
        // names the container as a whole token will do (`impl Storage`,
        // `impl<'a> Storage<'a>`, `impl Default for Storage`).
        let Some(open_byte) = first_significant_of(sig, at, '{') else {
            continue;
        };
        let header = &text[at..open_byte];
        if !split_identifiers(header).contains(&container) {
            continue;
        }
        if let Some(end) = matching_brace(sig, open_byte) {
            blocks.push((open_byte, end));
        }
    }
    blocks
}

/// End byte (**exclusive**) of the body that starts at `decl_start`.
///
/// Three shapes are recognised: a brace-delimited block (`fn f() { … }`), a
/// statement terminated by `;` (`const X: u32 = 1;`, `struct S;`), and an
/// indented block introduced by a colon (`def f():`). Anything else falls back
/// to the declaration's own line — which hashes to something stable, so at worst
/// a symbol whose body was not understood reports `stale` rather than `fresh`.
fn body_end_from(text: &str, sig: &[(usize, char)], decl_start: usize) -> Option<usize> {
    let (open, semi) = (
        first_significant_of(sig, decl_start, '{'),
        first_significant_of(sig, decl_start, ';'),
    );
    match (open, semi) {
        (Some(open_byte), Some(semi_byte)) if semi_byte < open_byte => Some(semi_byte + 1),
        (Some(open_byte), _) => matching_brace(sig, open_byte),
        (None, Some(semi_byte)) => Some(semi_byte + 1),
        (None, None) => {
            let line_end = text[decl_start..]
                .find('\n')
                .map(|i| decl_start + i)
                .unwrap_or(text.len());
            let line = text[decl_start..line_end].trim_end();
            if line.ends_with(':') {
                indented_block_end(text, decl_start)
            } else {
                Some(line_end)
            }
        }
    }
}

/// End of a Python-style indented block: the last line more indented than the
/// declaration, skipping blank lines.
fn indented_block_end(text: &str, decl_start: usize) -> Option<usize> {
    let line_start = text[..decl_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let base_indent = text[line_start..decl_start]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .count();
    let mut end = decl_start;
    for (start, stop) in lines(text) {
        if start <= line_start {
            continue;
        }
        let line = &text[start..stop];
        if line.trim().is_empty() {
            continue;
        }
        let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        if indent <= base_indent {
            break;
        }
        end = stop;
    }
    (end > decl_start).then_some(end)
}

/// End byte (**exclusive**) of the block opened by the `{` at `open_byte`.
fn matching_brace(sig: &[(usize, char)], open_byte: usize) -> Option<usize> {
    let start = sig.iter().position(|(i, _)| *i == open_byte)?;
    let mut depth = 0usize;
    for (idx, ch) in &sig[start..] {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

/// Brace depth at `at`, from the significant characters before it.
fn depth_at(sig: &[(usize, char)], at: usize) -> usize {
    let mut depth = 0usize;
    for (idx, ch) in sig {
        if *idx >= at {
            break;
        }
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Byte offset of the first `ch` at or after `from`, ignoring comments and the
/// insides of string literals.
fn first_significant_of(sig: &[(usize, char)], from: usize, ch: char) -> Option<usize> {
    sig.iter()
        .find(|(idx, c)| *idx >= from && *c == ch)
        .map(|(idx, _)| *idx)
}

/// `(start, end)` byte offsets of every line, `end` excluding the newline.
fn lines(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            out.push((start, idx));
            start = idx + 1;
        }
    }
    if start < text.len() {
        out.push((start, text.len()));
    }
    out
}

fn is_word_boundary(text: &str, at: usize, len: usize) -> bool {
    let before_ok = text[..at]
        .chars()
        .next_back()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    let after_ok = text[at + len..]
        .chars()
        .next()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
    before_ok && after_ok
}

/// Every character that is **not** inside a comment or a string/char literal,
/// paired with its byte offset.
///
/// Braces, semicolons and quotes found in those places are not code, and a body
/// scanner that counted them would end a function early — or never — the moment
/// someone wrote `"}"` in it. Quote delimiters themselves are kept so the stream
/// still shows where a literal began and ended; their contents are dropped.
fn significant_chars(text: &str) -> Vec<(usize, char)> {
    let mut out: Vec<(usize, char)> = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        match ch {
            '/' if chars.peek().is_some_and(|(_, c)| *c == '/') => {
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            '/' if chars.peek().is_some_and(|(_, c)| *c == '*') => {
                chars.next();
                let mut prev = '\0';
                for (_, c) in chars.by_ref() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
            }
            '"' => {
                out.push((idx, ch));
                // A raw string keeps its own terminator (`r#"…"#`), so the
                // number of `#` after the opening quote has to be matched.
                let hashes = raw_string_hashes(text, idx);
                let mut escaped = false;
                while let Some((i, c)) = chars.next() {
                    if c == '\n' && hashes == 0 && !escaped {
                        // An unterminated string: stop at the newline rather
                        // than swallowing the rest of the file.
                        break;
                    }
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if c == '\\' && hashes == 0 {
                        escaped = true;
                        continue;
                    }
                    if c != '"' {
                        continue;
                    }
                    let mut terminated = true;
                    for _ in 0..hashes {
                        match chars.next() {
                            Some((i, c)) if c == '#' => out.push((i, c)),
                            _ => {
                                terminated = false;
                                break;
                            }
                        }
                    }
                    if terminated {
                        out.push((i, c));
                        break;
                    }
                }
            }
            '\'' => {
                out.push((idx, ch));
                if let Some(len) = char_literal_len(text, idx) {
                    // Consume the literal, keeping only its closing quote.
                    for (i, c) in chars.by_ref() {
                        if i + c.len_utf8() >= idx + len {
                            out.push((i, c));
                            break;
                        }
                    }
                }
            }
            _ => out.push((idx, ch)),
        }
    }
    out
}

/// Number of `#` in the raw-string opener ending at `quote_idx` (0 for a plain
/// `"`), which is also the number that must close it.
fn raw_string_hashes(text: &str, quote_idx: usize) -> usize {
    let before = &text[..quote_idx];
    let hashes = before.chars().rev().take_while(|c| *c == '#').count();
    // `r"`, `r#"`, `br#"` — the `r` immediately before the `#` run is the only
    // thing that makes the terminator carry `#`s of its own.
    if before[..quote_idx - hashes].ends_with('r') {
        hashes
    } else {
        0
    }
}

/// Length in bytes of a char literal starting at `idx`, or `None` when the `'`
/// is something else — a lifetime (`&'a str`) is the common case, and treating
/// it as an unterminated literal would swallow the rest of the file.
fn char_literal_len(text: &str, idx: usize) -> Option<usize> {
    let rest = &text[idx + 1..];
    let mut chars = rest.char_indices();
    match chars.next() {
        // `'\\n'`, `'\\''`, `'\\\\'`: an escape, the escaped character, then the
        // closing quote. Skipping the escaped character is what keeps `'\\''`
        // from terminating on its own escaped quote.
        Some((_, '\\')) => {
            chars.next()?;
            let (i, c) = chars.next()?;
            (c == '\'').then_some(i + 2)
        }
        // `'a'`.
        Some((_, _)) => {
            let (i, c) = chars.next()?;
            (c == '\'').then_some(i + 2)
        }
        None => None,
    }
}
