//! Log list and detail rendering for the control station Logs tab.

use std::time::SystemTime;

use chrono::{DateTime, Utc};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use tracing::Level;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::appliance_log::ApplianceLogEntry;

use super::chrome;

/// Width reserved for `HH:MM:SS.mmm` in the list panel (plus one leading space).
pub const LIST_TIME_WIDTH: usize = 13;

/// Truncate `s` to at most `max_cols` terminal columns (Unicode display width).
pub(crate) fn clip_line_display(s: &str, max_cols: u16) -> String {
    clip_string_to_width(s, max_cols)
}

fn clip_string_to_width(s: &str, max_cols: u16) -> String {
    let max = max_cols.max(1) as usize;
    let sw = s.width();
    if sw <= max {
        return s.to_string();
    }
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    let mut w = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
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

pub(crate) fn level_label(level: Level) -> &'static str {
    match level {
        Level::ERROR => "ERROR",
        Level::WARN => "WARN",
        Level::INFO => "INFO",
        Level::DEBUG => "DEBUG",
        Level::TRACE => "TRACE",
    }
}

pub(crate) fn level_style(level: Level) -> Style {
    if chrome::no_color() {
        let s = Style::default();
        return match level {
            Level::ERROR | Level::WARN => s.add_modifier(Modifier::BOLD),
            Level::DEBUG | Level::TRACE => s.add_modifier(Modifier::DIM),
            Level::INFO => s,
        };
    }
    match level {
        Level::ERROR => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        Level::WARN => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        Level::INFO => Style::default().fg(Color::Cyan),
        Level::DEBUG | Level::TRACE => Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    }
}

pub(crate) fn log_list_unselected_style() -> Style {
    if chrome::no_color() {
        Style::default().add_modifier(Modifier::DIM)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn format_list_time_short(ts: SystemTime) -> String {
    let dt: DateTime<Utc> = ts.into();
    dt.format("%H:%M:%S%.3f").to_string()
}

fn format_detail_timestamp(ts: SystemTime) -> String {
    let dt: DateTime<Utc> = ts.into();
    dt.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}

fn truncate_target(target: &str, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    if target.width() <= max_cols {
        return target.to_string();
    }
    clip_string_to_width(target, max_cols as u16)
}

/// Build a list row: `LEVEL target: message` with optional dim time on the right.
pub(crate) fn format_list_line(
    entry: &ApplianceLogEntry,
    selected: bool,
    marker_style: Style,
    clip_cols: u16,
) -> Line<'static> {
    let max = clip_cols.max(1) as usize;
    let time_suffix = format_list_time_short(entry.timestamp);
    let reserve_time = if max > LIST_TIME_WIDTH + 4 {
        LIST_TIME_WIDTH
    } else {
        0
    };
    let body_budget = max.saturating_sub(2 + reserve_time); // marker

    let level_s = level_label(entry.level);
    let level_w = level_s.width() + 1; // trailing space
    let prefix = format!("{} ", level_s);
    let target_max = body_budget.saturating_sub(level_w + 2); // ": "
    let target = truncate_target(&entry.target, target_max);
    let head = format!("{prefix}{target}: ");
    let head_w = head.width();
    let msg_budget = body_budget.saturating_sub(head_w);
    let message = clip_string_to_width(&entry.message, msg_budget as u16);

    let base = if selected {
        marker_style
    } else {
        log_list_unselected_style()
    };
    let level_style = if selected {
        marker_style.add_modifier(Modifier::BOLD)
    } else {
        level_style(entry.level)
    };
    let dim = if selected {
        marker_style
    } else {
        chrome::dim_style()
    };

    let mut spans = vec![
        Span::styled(if selected { "› " } else { "  " }, marker_style),
        Span::styled(level_s.to_string(), level_style),
        Span::raw(" "),
        Span::styled(target, dim),
        Span::styled(": ", base),
        Span::styled(message, base),
    ];

    if reserve_time > 0 {
        let used: usize = spans.iter().map(|s| s.content.width()).sum();
        let pad = max.saturating_sub(used + time_suffix.width());
        if pad > 0 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
        spans.push(Span::styled(time_suffix, dim));
    }

    Line::from(spans)
}

/// Detail panel lines for the selected log entry.
pub(crate) fn format_detail_lines(entry: &ApplianceLogEntry) -> Vec<Line<'static>> {
    let ts = format_detail_timestamp(entry.timestamp);
    let lvl = level_label(entry.level);
    vec![
        Line::from(vec![
            Span::styled("time   ", chrome::dim_style()),
            Span::raw(ts),
        ]),
        Line::from(vec![
            Span::styled("level  ", chrome::dim_style()),
            Span::styled(lvl.to_string(), level_style(entry.level)),
        ]),
        Line::from(vec![
            Span::styled("target ", chrome::dim_style()),
            Span::raw(entry.target.clone()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            entry.message.clone(),
            Style::default(),
        )),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn sample_entry(message: &str) -> ApplianceLogEntry {
        ApplianceLogEntry {
            timestamp: UNIX_EPOCH + Duration::from_secs(3661),
            level: Level::INFO,
            target: "plasm_appliance_boot".into(),
            message: message.into(),
        }
    }

    #[test]
    fn list_line_prioritizes_message_over_timestamp() {
        let entry = sample_entry("embedded postgres: server ready");
        let line = format_list_line(&entry, false, log_list_unselected_style(), 80);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("embedded postgres: server ready"));
        assert!(text.contains("01:01:01"));
        assert!(!text.contains("1970-01-01T"));
    }

    #[test]
    fn narrow_width_drops_right_timestamp() {
        let entry = sample_entry("short");
        let line = format_list_line(&entry, false, log_list_unselected_style(), 14);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("01:01:01"));
    }

    #[test]
    fn level_label_present_in_list_line() {
        let entry = ApplianceLogEntry {
            timestamp: UNIX_EPOCH,
            level: Level::ERROR,
            target: "t".into(),
            message: "x".into(),
        };
        let line = format_list_line(&entry, false, log_list_unselected_style(), 60);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("ERROR"));
    }
}
