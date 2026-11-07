//! UTF-8-safe truncation (SPEC 36 R0.6 / LLD G06).
//!
//! `&s[..n]` panics when byte `n` falls inside a multi-byte character, and
//! every such slice in a request path was a remote panic: a tool result, a
//! bio or a message containing a 4-byte emoji at the cut killed the task
//! [V: `src/agent_backend/tool_executor.rs:335` @ 95cabef9 sliced tool output
//! at byte 32_000]. `str::floor_char_boundary` would do this, but it is not
//! stable on the deploy toolchain (rust 1.85, `Dockerfile:16`).

/// The longest prefix of `s` that is at most `max_bytes` long and ends on a
/// char boundary.
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut cut = max_bytes;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    &s[..cut]
}

#[cfg(test)]
mod tests {
    use super::truncate_at_char_boundary as t;

    #[test]
    fn never_splits_a_character() {
        // U+1F600 is 4 bytes: every cut inside it backs off to its start.
        let s = "ab\u{1F600}cd";
        assert_eq!(t(s, 2), "ab");
        for n in 3..=5 {
            assert_eq!(t(s, n), "ab", "cut at {n}");
        }
        assert_eq!(t(s, 6), "ab\u{1F600}");
        assert_eq!(t(s, 100), s);
        assert_eq!(t("", 5), "");
        assert_eq!(t("é", 0), "");
        // A family emoji is several scalars joined by ZWJ; each cut lands on
        // a scalar boundary and never panics.
        let fam = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        for n in 0..=fam.len() {
            assert!(fam.starts_with(t(fam, n)));
        }
    }
}
