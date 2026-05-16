//! Log list clipping (display width) for the control station Logs tab.

use std::borrow::Cow;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::chrome;

/// Strip ANSI CSI sequences so terminal width accounting matches visible glyphs.
pub(crate) fn strip_ansi_for_log_view(s: &str) -> String {
    String::from_utf8_lossy(&strip_ansi_escapes::strip(s.as_bytes())).into_owned()
}

/// Truncate `s` to at most `max_cols` terminal columns (Unicode display width).
/// Reserves one column for `…` when truncation occurs. Strips ANSI when escape bytes are present.
pub(crate) fn clip_log_line_display(s: &str, max_cols: u16) -> String {
    let plain: Cow<str> = if s.contains('\x1b') {
        Cow::Owned(strip_ansi_for_log_view(s))
    } else {
        Cow::Borrowed(s)
    };
    let s = plain.as_ref();
    let max = max_cols.max(1) as usize;
    let sw = s.width();
    if sw <= max {
        return plain.into_owned();
    }
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        let cw = if cw == 0 { 0 } else { cw };
        if cw == 0 {
            out.push(ch);
            continue;
        }
        if w + cw > budget {
            out.push('…');
            return out;
        }
        out.push(ch);
        w += cw;
    }
    out
}

/// Plain list-row style for log lines (readable on light backgrounds; still visible on dark).
pub(crate) fn log_list_unselected_style() -> ratatui::style::Style {
    use ratatui::style::{Color, Modifier, Style};
    let mut s = Style::default();
    if chrome::no_color() {
        s = s.add_modifier(Modifier::DIM);
    } else {
        s = s.fg(Color::Gray);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_ascii_truncates_with_ellipsis_budget() {
        let s = "0123456789";
        assert_eq!(clip_log_line_display(s, 12), s);
        assert_eq!(clip_log_line_display(s, 5), "0123…");
    }

    #[test]
    fn clip_wide_char_respects_display_width() {
        let s = "ab你好";
        assert!(s.width() > 4);
        let clipped = clip_log_line_display(s, 4);
        assert_eq!(clipped, "ab…");
        assert!(
            clipped.width() <= 4,
            "clipped={clipped:?} width {}",
            clipped.width()
        );
    }

    #[test]
    fn strip_ansi_removes_green() {
        let raw = "\x1b[32mINFO\x1b[0m hello";
        assert_eq!(strip_ansi_for_log_view(raw), "INFO hello");
    }
}
