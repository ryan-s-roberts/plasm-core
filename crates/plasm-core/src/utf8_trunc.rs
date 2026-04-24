//! UTF-8 **byte**–budget truncation with a single trailing ellipsis (`…`).
//!
//! Use when the limit is measured in UTF-8 bytes (e.g. terminal columns, MCP cell budgets).
//! For **character**-count limits, use a different helper — byte and char caps differ for non-ASCII text.

const ELLIPSIS: char = '…';

/// Truncate `s` to at most `max_bytes` UTF-8 bytes, including the ellipsis when truncated.
pub(crate) fn truncate_utf8_bytes_with_ellipsis(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }
    let ell_len = ELLIPSIS.len_utf8();
    if max_bytes <= ell_len {
        return ELLIPSIS.to_string();
    }
    let mut end = max_bytes - ell_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    if end == 0 {
        return ELLIPSIS.to_string();
    }
    let mut out = String::with_capacity(max_bytes);
    out.push_str(&s[..end]);
    out.push(ELLIPSIS);
    out
}

/// Like [`truncate_utf8_bytes_with_ellipsis`], but reuses the allocation when `s` is already short enough.
pub(crate) fn truncate_utf8_owned_with_ellipsis(s: String, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s;
    }
    truncate_utf8_bytes_with_ellipsis(s.as_str(), max_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_truncates_under_byte_cap() {
        let s = "hello world";
        assert_eq!(truncate_utf8_bytes_with_ellipsis(s, 8), "hello…");
    }

    #[test]
    fn greek_does_not_split_codepoints() {
        let s = "α".repeat(30);
        let out = truncate_utf8_bytes_with_ellipsis(&s, 10);
        assert!(out.ends_with('…'));
        assert!(out.len() <= 10);
    }
}
