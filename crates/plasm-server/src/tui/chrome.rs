//! Otel-tui–inspired shared chrome: tab rail, panel titles, filter bar, footer layout helpers.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::log_render;

/// Vertical chrome for the RUN control station (tab rail + body + footer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunningLayout {
    pub tab_rail: Rect,
    pub body: Rect,
    pub footer: Rect,
}

/// Tab rail height (border + one content line).
pub const RUN_TAB_RAIL_HEIGHT: u16 = 3;
/// Footer block height (top border + content + padding).
pub const RUN_FOOTER_HEIGHT: u16 = 3;

/// Split the terminal into tab rail, scrollable body, and footer.
pub fn split_running_vertical(area: Rect) -> RunningLayout {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(RUN_TAB_RAIL_HEIGHT),
            Constraint::Min(0),
            Constraint::Length(RUN_FOOTER_HEIGHT),
        ])
        .split(area);
    RunningLayout {
        tab_rail: chunks[0],
        body: chunks[1],
        footer: chunks[2],
    }
}

/// True when `NO_COLOR` is set (same semantics as the main TUI).
pub fn no_color() -> bool {
    std::env::var_os("NO_COLOR").is_some()
}

pub fn dim_style() -> Style {
    let mut s = Style::default();
    if !no_color() {
        s = s.fg(Color::DarkGray);
    } else {
        s = s.add_modifier(Modifier::DIM);
    }
    s
}

pub fn active_tab_style() -> Style {
    let mut s = Style::default().add_modifier(Modifier::BOLD);
    if !no_color() {
        s = s.fg(Color::Black).bg(Color::Yellow);
    }
    s
}

pub fn inactive_tab_style() -> Style {
    let mut s = Style::default();
    if !no_color() {
        s = s.fg(Color::DarkGray);
    } else {
        s = s.add_modifier(Modifier::DIM);
    }
    s
}

/// Highlight for the filter value while editing (otel-style purple bar).
pub fn filter_value_editing_style() -> Style {
    let mut s = Style::default().add_modifier(Modifier::BOLD);
    if !no_color() {
        s = s.fg(Color::Black).bg(Color::Magenta);
    }
    s
}

pub fn filter_value_idle_style() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

/// Panel title with optional focus letter: `Catalogues (l)`.
pub fn panel_block(title: &str, focus_key: Option<char>) -> Block<'_> {
    let t = match focus_key {
        Some(c) => format!("{title} ({c})"),
        None => title.to_string(),
    };
    Block::default().borders(Borders::ALL).title(t)
}

/// Horizontal split for list + detail (percent of width for the left pane).
pub fn split_list_detail(area: Rect, left_pct: u16) -> [Rect; 2] {
    split_list_detail_min_right(area, left_pct, 28)
}

/// Like [`split_list_detail`], but guarantees the right pane is at least `min_right_cols` wide.
pub fn split_list_detail_min_right(area: Rect, left_pct: u16, min_right_cols: u16) -> [Rect; 2] {
    let w = area.width;
    if w == 0 {
        return [area, area];
    }
    let min_r = min_right_cols.min(w.saturating_sub(1)).max(1);
    let max_left = w.saturating_sub(min_r).max(1);
    let want_left = ((w as u32) * (left_pct as u32) / 100).clamp(1, max_left as u32) as u16;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(want_left), Constraint::Min(min_r)])
        .split(area);
    [chunks[0], chunks[1]]
}

/// Main content area with optional notice strip at the bottom.
pub fn split_with_notice(area: Rect, show_notice: bool) -> (Rect, Option<Rect>) {
    if !show_notice {
        return (area, None);
    }
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(8)])
        .split(area);
    (split[0], Some(split[1]))
}

/// `< Status | [APIs] | OAuth | … >` plus dim trailing listen port; tab hint omitted when narrow.
pub fn tab_rail_line(
    active_index: usize,
    tab_titles: &[&str],
    listen_port: u16,
    max_cols: u16,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = vec![Span::raw("< ")];
    let n = tab_titles.len();
    for (i, title) in tab_titles.iter().enumerate() {
        let label = (*title).to_string();
        if i == active_index {
            spans.push(Span::styled(format!("[{label}]"), active_tab_style()));
        } else {
            spans.push(Span::styled(label, inactive_tab_style()));
        }
        if i + 1 < n {
            spans.push(Span::styled(" | ", dim_style()));
        }
    }
    spans.push(Span::raw(" >"));
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("listen:{listen_port}"),
        dim_style(),
    ));
    if max_cols >= 72 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("(←/→ or Tab)", dim_style()));
    }
    clip_line_spans_to_width(Line::from(spans), max_cols)
}

/// Truncate a rendered line to fit `max_cols` terminal columns.
pub fn clip_line_spans_to_width(line: Line<'static>, max_cols: u16) -> Line<'static> {
    let flat = line.to_string();
    let clipped = log_render::clip_line_display(&flat, max_cols);
    if clipped == flat {
        line
    } else {
        Line::from(clipped)
    }
}

/// One-line filter bar: `Filter catalogues (/): value` with optional editing highlight on value.
pub fn filter_bar_line(label: &str, value: &str, editing: bool) -> Line<'static> {
    let val_style = if editing {
        filter_value_editing_style()
    } else {
        filter_value_idle_style()
    };
    Line::from(vec![
        Span::styled(format!("{label}: "), dim_style()),
        Span::styled(value.to_string(), val_style),
    ])
}

#[derive(Clone, Copy, Debug)]
pub struct FooterItem {
    pub key: &'static str,
    pub desc: &'static str,
}

impl FooterItem {
    pub const fn new(key: &'static str, desc: &'static str) -> Self {
        Self { key, desc }
    }
}

/// `key: desc` segments joined by ` | ` (keys dim, descriptions normal).
pub fn footer_line(
    global: &[FooterItem],
    screen: &[FooterItem],
    mode_label: Option<&str>,
    admin_hint: Option<&str>,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut first = true;
    for g in global {
        if !first {
            spans.push(Span::styled(" | ", dim_style()));
        }
        first = false;
        spans.push(Span::styled(format!("{}: ", g.key), dim_style()));
        spans.push(Span::raw(g.desc.to_string()));
    }
    for s in screen {
        if !first {
            spans.push(Span::styled(" | ", dim_style()));
        }
        first = false;
        spans.push(Span::styled(format!("{}: ", s.key), dim_style()));
        spans.push(Span::raw(s.desc.to_string()));
    }
    if let Some(m) = mode_label {
        if !first {
            spans.push(Span::styled(" | ", dim_style()));
        }
        first = false;
        spans.push(Span::styled("mode: ", dim_style()));
        spans.push(Span::raw(m.to_string()));
    }
    if let Some(h) = admin_hint {
        if !first {
            spans.push(Span::styled(" | ", dim_style()));
        }
        spans.push(Span::styled(h.to_string(), dim_style()));
    }
    Line::from(spans)
}

/// Paint a full-frame clear before chrome (avoids ghost cells from stderr / prior frames).
pub fn clear_frame(frame: &mut ratatui::Frame<'_>) {
    frame.render_widget(Clear, frame.area());
}

pub fn render_tab_rail(frame: &mut ratatui::Frame<'_>, area: Rect, line: Line<'static>) {
    let max_cols = area.width.max(1);
    let line = clip_line_spans_to_width(line, max_cols);
    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::BOTTOM)),
        area,
    );
}

pub fn render_footer_bar(frame: &mut ratatui::Frame<'_>, area: Rect, line: Line<'static>) {
    let max_cols = area.width.max(1);
    let line = clip_line_spans_to_width(line, max_cols);
    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::TOP)),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_rail_brackets_active_tab() {
        let titles = ["Status", "Clients", "APIs", "OAuth"];
        let line = tab_rail_line(2, &titles, 8080, 120);
        let s = line.to_string();
        assert!(s.contains("< "));
        assert!(s.contains("[APIs]"));
        assert!(s.contains("listen:8080"));
    }

    #[test]
    fn tab_rail_truncates_on_narrow_terminal() {
        use unicode_width::UnicodeWidthStr;
        let titles = ["Status", "Clients", "APIs", "OAuth", "Keys", "Runs", "Storage", "Logs"];
        let line = tab_rail_line(2, &titles, 8080, 40);
        assert!(line.to_string().width() <= 40);
    }

    #[test]
    fn filter_bar_includes_label_and_value() {
        let line = filter_bar_line("Filter catalogues (/)", "github", false);
        let flat = line.to_string();
        assert!(flat.contains("Filter catalogues (/)"));
        assert!(flat.contains("github"));
    }

    #[test]
    fn footer_line_contains_screen_keys() {
        let global = [
            FooterItem::new("←/→", "tab"),
            FooterItem::new("q", "quit"),
        ];
        let screen = [
            FooterItem::new("/", "filter"),
            FooterItem::new("Space", "toggle"),
        ];
        let line = footer_line(&global, &screen, None, None);
        let flat = line.to_string();
        assert!(flat.contains("filter"));
        assert!(flat.contains("toggle"));
    }

    #[test]
    fn running_layout_reserves_tab_rail_and_footer() {
        let area = Rect::new(0, 0, 80, 24);
        let layout = split_running_vertical(area);
        assert_eq!(layout.tab_rail.height, RUN_TAB_RAIL_HEIGHT);
        assert_eq!(layout.footer.height, RUN_FOOTER_HEIGHT);
        assert_eq!(
            layout.tab_rail.height + layout.body.height + layout.footer.height,
            24
        );
    }
}
