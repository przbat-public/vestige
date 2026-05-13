//! UTF-8 safe truncate for description rendering.

/// Truncate string for display
pub(super) fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..s.floor_char_boundary(max_len)]
    }
}
