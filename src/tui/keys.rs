//! Keyboard and mouse bindings.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Focus, Mode};

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    match app.mode {
        Mode::Help => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q' | '?')) {
                app.mode = Mode::Normal;
            }
        }
        Mode::Search => search_input(app, key),
        Mode::Normal if app.focus == Focus::Toc && app.show_toc => {
            if !toc(app, key) {
                normal(app, key);
            }
        }
        Mode::Normal => normal(app, key),
    }
}

fn search_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.finish_search(true),
        KeyCode::Esc => app.finish_search(false),
        KeyCode::Backspace => {
            if app.search.input.pop().is_none() {
                app.finish_search(false);
                return;
            }
            app.update_search_input();
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.search.input.clear();
            app.update_search_input();
        }
        KeyCode::Char(c) => {
            app.search.input.push(c);
            app.update_search_input();
        }
        _ => {}
    }
}

/// TOC-specific keys; returns false when the key should fall through.
fn toc(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.toc_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.toc_move(-1),
        KeyCode::Char('g') | KeyCode::Home => app.toc_selected = 0,
        KeyCode::Char('G') | KeyCode::End => app.toc_move(isize::MAX),
        KeyCode::Enter | KeyCode::Char('o') => {
            app.toc_jump(app.toc_selected);
            app.focus = Focus::Content;
        }
        KeyCode::Char(' ') => app.toc_jump(app.toc_selected),
        KeyCode::Esc | KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => {
            app.focus = Focus::Content
        }
        _ => return false,
    }
    true
}

fn normal(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let page = app.view_height.max(1) as isize;
    let half = (page / 2).max(1);
    match key.code {
        KeyCode::Char('c') if ctrl => app.quit = true,
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Esc => {
            if app.link_focus.is_some() {
                app.link_focus = None;
            } else if !app.search.query.is_empty() {
                app.clear_search();
            } else {
                app.quit = true;
            }
        }
        KeyCode::Char('j') | KeyCode::Down => app.scroll_by(1),
        KeyCode::Char('k') | KeyCode::Up => app.scroll_by(-1),
        KeyCode::Enter => match app.focused_link_id() {
            Some(id) => app.follow_link(id),
            None => app.scroll_by(1),
        },
        KeyCode::Char('d') if ctrl => app.scroll_by(half),
        KeyCode::Char('u') if ctrl => app.scroll_by(-half),
        KeyCode::Char('f') if ctrl => app.scroll_by(page),
        KeyCode::Char('b') if ctrl => app.scroll_by(-page),
        KeyCode::Char('o') if ctrl => app.back(),
        KeyCode::Char('d') => app.scroll_by(half),
        KeyCode::Char('u') => app.scroll_by(-half),
        KeyCode::Char(' ' | 'f') | KeyCode::PageDown => app.scroll_by(page),
        KeyCode::Char('b') | KeyCode::PageUp => app.scroll_by(-page),
        KeyCode::Char('g') | KeyCode::Home => app.scroll_to(0),
        KeyCode::Char('G') | KeyCode::End => app.scroll_to(usize::MAX),
        KeyCode::Char(']') => app.jump_heading(true),
        KeyCode::Char('[') => app.jump_heading(false),
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('n') => app.next_match(true),
        KeyCode::Char('N') => app.next_match(false),
        KeyCode::Tab => app.cycle_link(true),
        KeyCode::BackTab => app.cycle_link(false),
        KeyCode::Backspace => app.back(),
        KeyCode::Char('t') => app.toggle_toc(),
        KeyCode::Char('h') | KeyCode::Left if app.show_toc => app.focus = Focus::Toc,
        KeyCode::Char('?') => app.mode = Mode::Help,
        KeyCode::Char('T') => app.cycle_theme(),
        KeyCode::Char('M') => {
            app.mouse = !app.mouse;
            let state = if app.mouse { "on" } else { "off (text selection enabled)" };
            app.notify(format!("Mouse {state}"));
        }
        KeyCode::Char('r') => match app.reload() {
            Ok(()) => app.notify("Reloaded"),
            Err(e) => app.notify(format!("Reload failed: {e:#}")),
        },
        _ => {}
    }
}

pub fn handle_mouse(app: &mut App, m: MouseEvent) {
    let pos = Position::new(m.column, m.row);
    let in_toc = app.toc_rect.is_some_and(|r| r.contains(pos));
    match m.kind {
        MouseEventKind::ScrollDown if in_toc => app.toc_move(3),
        MouseEventKind::ScrollUp if in_toc => app.toc_move(-3),
        MouseEventKind::ScrollDown => app.scroll_by(3),
        MouseEventKind::ScrollUp => app.scroll_by(-3),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(r) = app.toc_rect.filter(|_| in_toc) {
                // Mirrors the list offset logic in ui::draw_toc.
                let row = (m.row - r.y) as usize + app.toc_offset(r.height as usize);
                if row < app.rendered.headings.len() {
                    app.toc_jump(row);
                }
            } else if let Some(id) = link_at(app, app.text_rect, pos) {
                app.follow_link(id);
            }
        }
        _ => {}
    }
}

/// Link id under a screen position inside the text area.
fn link_at(app: &App, r: Rect, pos: Position) -> Option<usize> {
    if !r.contains(pos) {
        return None;
    }
    let line = app.rendered.lines.get(app.scroll + (pos.y - r.y) as usize)?;
    let col = (pos.x - r.x) as usize;
    let mut x = 0;
    for s in &line.spans {
        let w = s.text.width();
        if col < x + w {
            return s.link;
        }
        x += w;
    }
    None
}
