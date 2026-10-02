//! Drawing of the viewer screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line as TLine, Span as TSpan};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Focus, Mode};
use crate::layout::Line;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let [main, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let base = app.depth.adapt(app.theme.text);
    f.render_widget(Block::new().style(base), main);

    let content = if app.show_toc && main.width >= 60 {
        let toc_w = (main.width / 3).clamp(20, 36);
        let [toc, content] =
            Layout::horizontal([Constraint::Length(toc_w), Constraint::Min(1)]).areas(main);
        draw_toc(f, app, toc);
        content
    } else {
        app.toc_rect = None;
        if app.focus == Focus::Toc {
            app.focus = Focus::Content;
        }
        main
    };
    draw_content(f, app, content);
    draw_status(f, app, status);
    if app.mode == Mode::Help {
        draw_help(f, app, area);
    }
}

/// Text column: width-limited and centered inside `area`.
fn text_area(app: &App, area: Rect) -> Rect {
    // One column margin on each side, one for the scrollbar.
    let avail = area.width.saturating_sub(3);
    if avail < 10 {
        return area;
    }
    let width = if app.max_width > 0 {
        avail.min(app.max_width as u16)
    } else {
        avail
    };
    let x = area.x + 1 + (avail - width) / 2;
    Rect::new(x, area.y, width, area.height)
}

fn draw_toc(f: &mut Frame, app: &mut App, area: Rect) {
    let t = &app.theme;
    let focused = app.focus == Focus::Toc;
    let border = if focused { t.ui_accent } else { t.dim };
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(app.depth.adapt(border))
        .title(TSpan::styled(" Contents ", app.depth.adapt(t.ui_accent)));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.toc_rect = Some(inner);

    let height = inner.height as usize;
    let offset = app.toc_offset(height);
    let highlight = app.toc_highlight();
    let current = app.current_heading();
    let min_level = app.rendered.headings.iter().map(|h| h.level).min().unwrap_or(1);
    let width = inner.width as usize;

    let lines: Vec<TLine> = app
        .rendered
        .headings
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(i, h)| {
            let indent = "  ".repeat((h.level - min_level) as usize);
            let marker = if Some(i) == current { "▸ " } else { "  " };
            let mut text = format!("{marker}{indent}{}", h.title);
            text = truncate(&text, width);
            let mut style = if h.level == min_level {
                t.headings[0]
            } else {
                t.text
            };
            if Some(i) == current {
                style = style.patch(t.ui_accent);
            }
            if i == highlight && focused {
                style = style.add_modifier(Modifier::REVERSED);
            }
            TLine::from(TSpan::styled(text, app.depth.adapt(style)))
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw + 1 > width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

fn draw_content(f: &mut Frame, app: &mut App, area: Rect) {
    let text = text_area(app, area);
    app.text_rect = text;
    app.view_height = text.height as usize;
    app.ensure_layout(text.width as usize);
    app.clamp_scroll();

    let end = (app.scroll + text.height as usize).min(app.rendered.lines.len());
    let lines: Vec<TLine> = (app.scroll..end)
        .map(|n| to_tline(app, n, &app.rendered.lines[n]))
        .collect();
    f.render_widget(Paragraph::new(lines), text);

    if app.max_scroll() > 0 {
        let mut state = ScrollbarState::new(app.max_scroll()).position(app.scroll);
        let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(Some(" "))
            .thumb_style(app.depth.adapt(app.theme.dim));
        f.render_stateful_widget(bar, area, &mut state);
    }
}

/// Converts a layout line, applying search and link-focus highlights.
fn to_tline<'a>(app: &App, n: usize, line: &'a Line) -> TLine<'a> {
    let t = &app.theme;
    let focus = app.focused_link_id();
    let mut ranges: Vec<(usize, usize, Style)> = Vec::new();
    let first = app.search.matches.partition_point(|m| m.line < n);
    for (i, m) in app.search.matches[first..]
        .iter()
        .enumerate()
        .take_while(|(_, m)| m.line == n)
    {
        let style = if app.search.current == Some(first + i) {
            t.search_current
        } else {
            t.search_match
        };
        ranges.push((m.start, m.end, style));
    }

    let mut spans = Vec::with_capacity(line.spans.len());
    let mut offset = 0;
    for s in &line.spans {
        let mut style = s.style;
        if focus.is_some() && s.link == focus {
            style = style.patch(t.link_focus);
        }
        let (start, end) = (offset, offset + s.text.len());
        offset = end;
        let mut cuts = vec![start, end];
        for &(a, b, _) in &ranges {
            for c in [a, b] {
                if c > start && c < end {
                    cuts.push(c);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for w in cuts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let mut st = style;
            if let Some(&(_, _, hl)) = ranges.iter().find(|(ra, rb, _)| *ra <= a && b <= *rb) {
                st = st.patch(hl);
            }
            if let Some(part) = s.text.get(a - start..b - start) {
                spans.push(TSpan::styled(part, app.depth.adapt(st)));
            }
        }
    }
    TLine::from(spans)
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let bar = app.depth.adapt(t.ui_bar);
    let accent = app.depth.adapt(t.ui_accent.patch(t.ui_bar));

    if app.mode == Mode::Search {
        let line = TLine::from(vec![
            TSpan::styled(" /", accent),
            TSpan::styled(app.search.input.as_str(), bar),
            TSpan::styled("▏", accent),
            TSpan::styled(format!("  {}", match_count(app)), app.depth.adapt(t.dim.patch(t.ui_bar))),
        ]);
        f.render_widget(Paragraph::new(line).style(bar), area);
        return;
    }

    let mut left = vec![
        TSpan::styled(" mdv ", accent.add_modifier(Modifier::REVERSED)),
        TSpan::styled(format!(" {} ", app.source.name()), accent),
    ];
    if let Some(msg) = app.message() {
        left.push(TSpan::styled(format!("│ {msg} "), bar));
    } else if let Some(id) = app.focused_link_id() {
        left.push(TSpan::styled(format!("│ → {} ", app.rendered.links[id]), bar));
    } else if let Some(h) = app.current_heading() {
        left.push(TSpan::styled(
            format!("│ {} ", app.rendered.headings[h].title),
            bar,
        ));
    }

    let mut right = String::new();
    if app.watcher.is_some() {
        right.push_str(" ● watch │");
    }
    if !app.search.query.is_empty() {
        right.push_str(&format!(" /{} {} │", app.search.query, match_count(app)));
    }
    right.push_str(&format!(
        " {}% │ ~{} min │ ? help ",
        app.progress(),
        app.reading_minutes()
    ));
    let right_w = (right.width() as u16).min(area.width / 2);
    let [l, r] = Layout::horizontal([Constraint::Min(1), Constraint::Length(right_w)]).areas(area);
    f.render_widget(Paragraph::new(TLine::from(left)).style(bar), l);
    f.render_widget(Paragraph::new(right).style(bar), r);
}

fn match_count(app: &App) -> String {
    match (app.search.current, app.search.matches.len()) {
        (_, 0) if app.search.query.is_empty() => String::new(),
        (_, 0) => "[no match]".into(),
        (Some(c), n) => format!("[{}/{n}]", c + 1),
        (None, n) => format!("[{n}]"),
    }
}

pub const HELP: &[(&str, &str)] = &[
    ("j / k / ↓ / ↑", "Scroll one line"),
    ("d / u, Ctrl-d / Ctrl-u", "Half page down / up"),
    ("Space / b, PgDn / PgUp", "Page down / up"),
    ("g / G, Home / End", "Top / bottom"),
    ("] / [", "Next / previous heading"),
    ("t", "Table of contents (h/l switch focus)"),
    ("/  n / N", "Search (regex, smart-case), next / prev"),
    ("Tab / Shift-Tab", "Next / previous link"),
    ("Enter", "Open focused link"),
    ("Backspace, Ctrl-o", "Go back"),
    ("T", "Cycle color theme"),
    ("y 1-9 / y y", "Copy code block [n] / on screen"),
    ("m <a-z>, ' <a-z>", "Set / jump to bookmark"),
    ("e", "Edit file in $EDITOR"),
    ("w", "Watch file for changes"),
    ("z", "Toggle focus mode / full width"),
    ("r", "Reload file"),
    ("M", "Toggle mouse capture (text selection)"),
    ("Esc", "Clear search / link focus, then quit"),
    ("?", "Toggle this help"),
    ("q", "Quit"),
];

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let key_w = HELP.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    let lines: Vec<TLine> = HELP
        .iter()
        .map(|(k, d)| {
            TLine::from(vec![
                TSpan::styled(format!(" {k:<key_w$}  "), app.depth.adapt(t.ui_accent)),
                TSpan::raw(*d),
            ])
        })
        .collect();
    let w = (lines.iter().map(TLine::width).max().unwrap_or(20) as u16 + 4).min(area.width);
    let h = (lines.len() as u16 + 2).min(area.height);
    let popup = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(app.depth.adapt(t.dim))
        .title(" Keys ")
        .style(app.depth.adapt(t.text));
    f.render_widget(Paragraph::new(lines).block(block), popup);
}
