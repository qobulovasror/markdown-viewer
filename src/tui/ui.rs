//! Drawing of the viewer screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line as TLine, Span as TSpan};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Mode};
use crate::layout::Line;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let [main, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let base = app.depth.adapt(app.theme.text);
    f.render_widget(Block::new().style(base), main);

    draw_content(f, app, main);
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

fn draw_content(f: &mut Frame, app: &mut App, area: Rect) {
    let text = text_area(app, area);
    app.view_height = text.height as usize;
    app.ensure_layout(text.width as usize);
    app.clamp_scroll();

    let end = (app.scroll + text.height as usize).min(app.rendered.lines.len());
    let lines: Vec<TLine> = app.rendered.lines[app.scroll..end]
        .iter()
        .map(|l| to_tline(app, l))
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

fn to_tline<'a>(app: &App, line: &'a Line) -> TLine<'a> {
    TLine::from(
        line.spans
            .iter()
            .map(|s| TSpan::styled(s.text.as_str(), app.depth.adapt(s.style)))
            .collect::<Vec<_>>(),
    )
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let bar = app.depth.adapt(t.ui_bar);
    let accent = app.depth.adapt(t.ui_accent.patch(t.ui_bar));

    let mut left = vec![
        TSpan::styled(" mdv ", accent.add_modifier(Modifier::REVERSED)),
        TSpan::styled(format!(" {} ", app.source.name()), accent),
    ];
    if let Some(msg) = app.message() {
        left.push(TSpan::styled(format!("│ {msg} "), bar));
    } else if let Some(h) = app.current_heading() {
        left.push(TSpan::styled(
            format!("│ {} ", app.rendered.headings[h].title),
            bar,
        ));
    }

    let right = format!(
        " {}% │ ~{} min │ ? help ",
        app.progress(),
        app.reading_minutes()
    );
    let right_w = right.width() as u16;
    let [l, r] = Layout::horizontal([Constraint::Min(1), Constraint::Length(right_w)]).areas(area);
    f.render_widget(Paragraph::new(TLine::from(left)).style(bar), l);
    f.render_widget(Paragraph::new(right).style(bar), r);
}

pub const HELP: &[(&str, &str)] = &[
    ("j / k / ↓ / ↑", "Scroll one line"),
    ("d / u, Ctrl-d / Ctrl-u", "Half page down / up"),
    ("Space / b, PgDn / PgUp", "Page down / up"),
    ("g / G, Home / End", "Top / bottom"),
    ("r", "Reload file"),
    ("M", "Toggle mouse capture (text selection)"),
    ("?", "Toggle this help"),
    ("q / Esc", "Quit"),
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
