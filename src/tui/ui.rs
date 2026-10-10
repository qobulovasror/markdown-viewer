//! Drawing of the viewer screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line as TLine, Span as TSpan};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Focus, Mode, Panel};
use crate::layout::Line;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let [main, status] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(area);
    let base = app.depth.adapt(app.theme.text);
    f.render_widget(Block::new().style(base), main);

    let content = match app.panel {
        Some(panel) if main.width >= 60 => {
            let toc_w = (main.width / 3).clamp(20, 36);
            let [side, content] =
                Layout::horizontal([Constraint::Length(toc_w), Constraint::Min(1)]).areas(main);
            match panel {
                Panel::Toc => draw_toc(f, app, side),
                Panel::Files => draw_files(f, app, side),
            }
            content
        }
        _ => {
            app.panel_rect = None;
            if app.focus == Focus::Panel {
                app.focus = Focus::Content;
            }
            main
        }
    };
    draw_content(f, app, content);
    draw_status(f, app, status);
    match app.mode {
        Mode::Help => draw_help(f, app, area),
        Mode::Info => draw_info(f, app, area),
        Mode::Finder => draw_finder(f, app, area),
        Mode::Grep => draw_grep(f, app, area),
        _ => {}
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

/// Side panel frame; returns the inner area.
fn panel_block(f: &mut Frame, app: &mut App, area: Rect, title: &str) -> Rect {
    let t = &app.theme;
    let border = if app.focus == Focus::Panel {
        t.ui_accent
    } else {
        t.dim
    };
    let block = Block::new()
        .borders(Borders::RIGHT)
        .border_style(app.depth.adapt(border))
        .title(TSpan::styled(
            title.to_string(),
            app.depth.adapt(t.ui_accent),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.panel_rect = Some(inner);
    inner
}

fn draw_files(f: &mut Frame, app: &mut App, area: Rect) {
    let inner = panel_block(f, app, area, " Files ");
    let t = &app.theme;
    let focused = app.focus == Focus::Panel;
    let height = inner.height as usize;
    let width = inner.width as usize;
    let current = app.current_file();
    let lines: Vec<TLine> = app
        .files
        .iter()
        .enumerate()
        .skip(app.files_offset(height))
        .take(height)
        .map(|(i, p)| {
            let marker = if Some(i) == current { "▸ " } else { "  " };
            let dir = p
                .parent()
                .map(|d| d.to_string_lossy().into_owned())
                .filter(|d| !d.is_empty())
                .map(|d| format!("{d}/"))
                .unwrap_or_default();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let full = truncate(&format!("{marker}{dir}{name}"), width);
            let mut split = (marker.len() + dir.len()).min(full.len());
            while !full.is_char_boundary(split) {
                split -= 1;
            }
            let (head, tail) = full.split_at(split);
            let mut name_style = if Some(i) == current {
                t.ui_accent
            } else {
                t.text
            };
            let mut dir_style = t.dim;
            if i == app.files_selected && focused {
                name_style = name_style.add_modifier(Modifier::REVERSED);
                dir_style = dir_style.add_modifier(Modifier::REVERSED);
            }
            TLine::from(vec![
                TSpan::styled(head.to_string(), app.depth.adapt(dir_style)),
                TSpan::styled(tail.to_string(), app.depth.adapt(name_style)),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_grep(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let g = &app.grep;
    let w = (area.width * 4 / 5).clamp(30.min(area.width), area.width);
    let h = (area.height * 4 / 5).max(5.min(area.height));
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 3,
        w,
        h,
    );
    let (a, b) = g.input.split();
    let status = if g.ran.is_empty() || g.ran != g.input.text {
        "Enter to search".to_string()
    } else if g.hits.is_empty() {
        "no matches".to_string()
    } else {
        let more = if g.truncated { "+" } else { "" };
        format!("{}/{}{more}  Enter opens", g.selected + 1, g.hits.len())
    };
    let mut lines = vec![
        TLine::from(vec![
            TSpan::styled(" / ", app.depth.adapt(t.ui_accent)),
            TSpan::raw(a),
            TSpan::styled("▏", app.depth.adapt(t.ui_accent)),
            TSpan::raw(b),
            TSpan::styled(format!("  {status}"), app.depth.adapt(t.dim)),
        ]),
        TLine::raw(""),
    ];
    let rows = (h as usize).saturating_sub(4).max(1);
    let offset = g.selected.saturating_sub(rows.saturating_sub(1));
    let inner_w = w.saturating_sub(4) as usize;
    for (k, hit) in g.hits.iter().enumerate().skip(offset).take(rows) {
        let loc = format!(" {}:{}: ", app.files[hit.file].display(), hit.line);
        let text = truncate(&format!("{loc}{}", hit.text), inner_w);
        let split = loc.len().min(text.len());
        let (head, tail) = text.split_at(if text.is_char_boundary(split) {
            split
        } else {
            0
        });
        let mut loc_style = app.depth.adapt(t.dim);
        let mut text_style = app.depth.adapt(t.text);
        if k == g.selected {
            loc_style = loc_style.add_modifier(Modifier::REVERSED);
            text_style = text_style.add_modifier(Modifier::REVERSED);
        }
        lines.push(TLine::from(vec![
            TSpan::styled(head.to_string(), loc_style),
            TSpan::styled(tail.to_string(), text_style),
        ]));
    }
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(app.depth.adapt(t.dim))
        .title(format!(" Search {} files ", app.files.len()))
        .style(app.depth.adapt(t.text));
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

fn draw_finder(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let w = (area.width * 3 / 5).clamp(30.min(area.width), area.width);
    let max_rows = (area.height * 3 / 5).max(3) as usize;
    let rows = app
        .finder
        .results
        .len()
        .clamp(1, max_rows.saturating_sub(3).max(1));
    let h = (rows as u16 + 4).min(area.height);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 3,
        w,
        h,
    );
    let mut lines = vec![
        TLine::from(vec![
            TSpan::styled(" > ", app.depth.adapt(t.ui_accent)),
            TSpan::raw(app.finder.input.split().0),
            TSpan::styled("▏", app.depth.adapt(t.ui_accent)),
            TSpan::raw(app.finder.input.split().1),
            TSpan::styled(
                format!(
                    "  {}/{}",
                    app.finder.results.len(),
                    app.finder_items().len()
                ),
                app.depth.adapt(t.dim),
            ),
        ]),
        TLine::raw(""),
    ];
    let offset = app.finder.selected.saturating_sub(rows.saturating_sub(1));
    let inner_w = w.saturating_sub(4) as usize;
    for (k, &i) in app
        .finder
        .results
        .iter()
        .enumerate()
        .skip(offset)
        .take(rows)
    {
        let text = truncate(&format!(" {}", app.finder_items()[i].display()), inner_w);
        let mut style = app.depth.adapt(t.text);
        if k == app.finder.selected {
            style = style.add_modifier(Modifier::REVERSED);
        }
        lines.push(TLine::from(TSpan::styled(text, style)));
    }
    if app.finder.results.is_empty() {
        lines.push(TLine::from(TSpan::styled(
            " no matches",
            app.depth.adapt(t.dim),
        )));
    }
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(app.depth.adapt(t.dim))
        .title(if app.finder.recent.is_some() {
            " Recent files "
        } else {
            " Open file "
        })
        .style(app.depth.adapt(t.text));
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

fn draw_toc(f: &mut Frame, app: &mut App, area: Rect) {
    let title = if app.toc_filtering || !app.toc_filter.is_empty() {
        let (a, b) = app.toc_filter.split();
        let cursor = if app.toc_filtering { "▏" } else { "" };
        format!(" Contents /{a}{cursor}{b} ")
    } else {
        " Contents ".to_string()
    };
    let inner = panel_block(f, app, area, &title);
    let t = &app.theme;
    let focused = app.focus == Focus::Panel;

    let height = inner.height as usize;
    let offset = app.toc_offset(height);
    let highlight = app.toc_highlight();
    let current = app.current_heading();
    let min_level = app
        .rendered
        .headings
        .iter()
        .map(|h| h.level)
        .min()
        .unwrap_or(1);
    let width = inner.width as usize;

    let entries = app.toc_entries();
    if entries.is_empty() {
        let msg = TLine::styled(" no matching headings", app.depth.adapt(t.dim));
        f.render_widget(Paragraph::new(msg), inner);
        return;
    }
    let lines: Vec<TLine> = entries
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(row, &i)| {
            let h = &app.rendered.headings[i];
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
            if row == highlight && focused {
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
        .map(|n| {
            let line = &app.rendered.lines[n];
            let tl = to_tline(app, n, line);
            match line.hscroll {
                Some(inner) => shift_code_row(tl, app.hscroll, inner as usize),
                None => tl,
            }
        })
        .collect();
    f.render_widget(Paragraph::new(lines), text);
    draw_images(f, app, text, end);

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

/// Draws images whose reserved rows are fully on screen.
fn draw_images(f: &mut Frame, app: &mut App, text: Rect, end: usize) {
    if app.images.is_none() {
        return;
    }
    let slots: Vec<(usize, String, u16)> = (app.scroll..end)
        .filter_map(|n| {
            let (url, rows) = app.rendered.lines[n].image.clone()?;
            (n + rows as usize <= end).then_some((n, url, rows))
        })
        .collect();
    // Disjoint field borrows: no copy of the document text per frame.
    let (src, scroll) = (&app.source, app.scroll);
    let Some(images) = app.images.as_mut() else {
        return;
    };
    for (n, url, rows) in slots {
        let Some(proto) = images.protocol(&url, src) else {
            continue;
        };
        let area = Rect::new(text.x, text.y + (n - scroll) as u16, text.width, rows);
        f.render_widget(Clear, area);
        f.render_stateful_widget(ratatui_image::StatefulImage::default(), area, proto);
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

/// Shows columns `h..h + inner` of an unwrapped `│ text │` code row.
fn shift_code_row(line: TLine<'_>, h: usize, inner: usize) -> TLine<'static> {
    let total: usize = line.spans.iter().map(|s| s.content.width()).sum();
    let body_end = total.saturating_sub(2);
    let mut spans = slice_cols(&line.spans, 0, 2);
    let body = slice_cols(
        &line.spans,
        (2 + h).min(body_end),
        (2 + h + inner).min(body_end),
    );
    let shown: usize = body.iter().map(|s| s.content.width()).sum();
    spans.extend(body);
    if shown < inner {
        // The span before the closing border is the row's padding.
        let n = line.spans.len();
        let style = line
            .spans
            .get(n.saturating_sub(2))
            .map(|s| s.style)
            .unwrap_or_default();
        spans.push(TSpan::styled(" ".repeat(inner - shown), style));
    }
    spans.extend(slice_cols(&line.spans, body_end, total));
    TLine::from(spans)
}

/// Display columns `start..end` of `spans`, measured per grapheme like the
/// layout; a wide grapheme cut in half becomes spaces.
fn slice_cols(spans: &[TSpan<'_>], start: usize, end: usize) -> Vec<TSpan<'static>> {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out = Vec::new();
    let mut col = 0;
    for s in spans {
        let mut text = String::new();
        for g in s.content.graphemes(true) {
            let w = g.width();
            if col >= start && col + w <= end {
                text.push_str(g);
            } else if col < end && col + w > start {
                text.push_str(&" ".repeat((col + w).min(end) - col.max(start)));
            }
            col += w;
        }
        if !text.is_empty() {
            out.push(TSpan::styled(text, s.style));
        }
    }
    out
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let bar = app.depth.adapt(t.ui_bar);
    let accent = app.depth.adapt(t.ui_accent.patch(t.ui_bar));

    if app.mode == Mode::Search {
        let line = TLine::from(vec![
            TSpan::styled(" /", accent),
            TSpan::styled(app.search.input.split().0, bar),
            TSpan::styled("▏", accent),
            TSpan::styled(app.search.input.split().1, bar),
            TSpan::styled(
                format!("  {}", match_count(app)),
                app.depth.adapt(t.dim.patch(t.ui_bar)),
            ),
        ]);
        f.render_widget(Paragraph::new(line).style(bar), area);
        return;
    }

    let mut left = vec![
        TSpan::styled(" mdvw ", accent.add_modifier(Modifier::REVERSED)),
        TSpan::styled(format!(" {} ", app.source.name()), accent),
    ];
    if let Some(msg) = app.message() {
        left.push(TSpan::styled(format!("│ {msg} "), bar));
    } else if let Some(id) = app.focused_link_id() {
        left.push(TSpan::styled(
            format!("│ → {} ", app.rendered.links[id]),
            bar,
        ));
    } else if let Some(h) = app.current_heading() {
        left.push(TSpan::styled(
            format!("│ {} ", app.rendered.headings[h].title),
            bar,
        ));
    }

    let mut right = String::new();
    if let Some(c) = app.count {
        right.push_str(&format!(" {c} │"));
    }
    if app.hscroll > 0 {
        right.push_str(&format!(" →{} │", app.hscroll));
    }
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

fn draw_info(f: &mut Frame, app: &App, area: Rect) {
    let t = &app.theme;
    let key = app.depth.adapt(t.ui_accent);
    let r = &app.rendered;
    let path = app
        .source
        .path
        .as_ref()
        .map_or("stdin".to_string(), |p| p.display().to_string());
    let mut rows: Vec<(String, String)> = vec![
        ("File".into(), path),
        ("Format".into(), format!("{:?}", app.source.format)),
        (
            "Words".into(),
            format!("{} (~{} min)", app.words, app.reading_minutes()),
        ),
        ("Headings".into(), r.headings.len().to_string()),
        ("Links".into(), r.links.len().to_string()),
        ("Code blocks".into(), r.code_blocks.len().to_string()),
    ];
    if let Some(fm) = &app.doc.front_matter {
        rows.push((String::new(), String::new()));
        for l in fm.lines() {
            let (k, v) = l.split_once(':').unwrap_or((l, ""));
            rows.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let key_w = rows.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    let lines: Vec<TLine> = rows
        .into_iter()
        .map(|(k, v)| {
            TLine::from(vec![
                TSpan::styled(format!(" {k:<key_w$}  "), key),
                TSpan::raw(v),
            ])
        })
        .collect();
    popup(f, app, area, " Document ", lines);
}

fn popup(f: &mut Frame, app: &App, area: Rect, title: &str, lines: Vec<TLine>) {
    let t = &app.theme;
    let w = (lines.iter().map(TLine::width).max().unwrap_or(20) as u16 + 4).min(area.width);
    let h = (lines.len() as u16 + 2).min(area.height);
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    f.render_widget(Clear, rect);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(app.depth.adapt(t.dim))
        .title(title.to_string())
        .style(app.depth.adapt(t.text));
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .wrap(ratatui::widgets::Wrap { trim: false }),
        rect,
    );
}

fn draw_help(f: &mut Frame, app: &mut App, area: Rect) {
    let t = &app.theme;
    let rows = app.keymap.help();
    let key_w = rows.iter().map(|(k, _)| k.width()).max().unwrap_or(0);
    // Lines are cut to fit so each takes one row and scrolling stays exact.
    let fit = (area.width as usize).saturating_sub(4);
    let desc_w = fit.saturating_sub(key_w + 3).max(1);
    let mut lines: Vec<TLine> = rows
        .into_iter()
        .map(|(k, d)| {
            TLine::from(vec![
                TSpan::styled(
                    truncate(&format!(" {k:<key_w$}  "), fit),
                    app.depth.adapt(t.ui_accent),
                ),
                TSpan::raw(truncate(d, desc_w)),
            ])
        })
        .collect();
    lines.push(TLine::raw(""));
    lines.push(TLine::styled(
        truncate(" Prefix a count: 5j, 3], 42G (line 42), 50%", fit),
        app.depth.adapt(t.dim),
    ));
    let visible = area.height.saturating_sub(2) as usize;
    let max = lines.len().saturating_sub(visible);
    app.help_scroll = app.help_scroll.min(max);
    let title = if max > 0 {
        " Keys (j/k to scroll) "
    } else {
        " Keys "
    };
    let lines = lines.split_off(app.help_scroll);
    popup(f, app, area, title, lines);
}
