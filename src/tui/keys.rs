//! Keyboard and mouse bindings.

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthStr;

use super::app::{App, Focus, Mode, Panel, Pending};
use super::input::Edit;
use super::keymap::Action;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    if key.kind == KeyEventKind::Release {
        return;
    }
    if app.mode == Mode::Normal
        && let Some(p) = app.pending.take()
    {
        pending(app, p, key);
        return;
    }
    match app.mode {
        Mode::Help => help(app, key),
        Mode::Info => {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q' | '?' | 'i') | KeyCode::Enter
            ) {
                app.mode = Mode::Normal;
            }
        }
        Mode::Search => search_input(app, key),
        Mode::Finder => finder(app, key),
        Mode::Normal if app.focus == Focus::Panel && app.panel == Some(Panel::Toc) => {
            if toc(app, key) {
                app.count = None;
            } else {
                normal(app, key);
            }
        }
        Mode::Normal if app.focus == Focus::Panel && app.panel == Some(Panel::Files) => {
            if files(app, key) {
                app.count = None;
            } else {
                normal(app, key);
            }
        }
        Mode::Normal => normal(app, key),
    }
}

fn help(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.help_scroll += 1,
        KeyCode::Char('k') | KeyCode::Up => app.help_scroll = app.help_scroll.saturating_sub(1),
        KeyCode::Char(' ' | 'd') | KeyCode::PageDown => app.help_scroll += 10,
        KeyCode::Char('b' | 'u') | KeyCode::PageUp => {
            app.help_scroll = app.help_scroll.saturating_sub(10)
        }
        KeyCode::Char('g') | KeyCode::Home => app.help_scroll = 0,
        _ => app.mode = Mode::Normal,
    }
}

fn search_input(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => app.finish_search(true),
        KeyCode::Esc => app.finish_search(false),
        KeyCode::Backspace if app.search.input.is_empty() => app.finish_search(false),
        KeyCode::Up | KeyCode::Down => {
            let back = key.code == KeyCode::Up;
            if let Some(text) = app.search.history.step(back, &app.search.input.text) {
                app.search.input.set(&text);
                app.update_search_input();
            }
        }
        _ => {
            if app.search.input.handle(&key) == Edit::Changed {
                app.search.history.reset();
                app.update_search_input();
            }
        }
    }
}

fn finder(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Esc => app.mode = Mode::Normal,
        KeyCode::Enter => app.finder_open(),
        KeyCode::Down | KeyCode::Tab => app.finder_move(1),
        KeyCode::Char('n' | 'j') if ctrl => app.finder_move(1),
        KeyCode::Up | KeyCode::BackTab => app.finder_move(-1),
        KeyCode::Char('p' | 'k') if ctrl => app.finder_move(-1),
        _ => {
            if app.finder.input.handle(&key) == Edit::Changed {
                app.finder_update();
            }
        }
    }
}

/// File panel keys; returns false when the key should fall through.
fn files(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.files_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.files_move(-1),
        KeyCode::Char('g') | KeyCode::Home => app.files_selected = 0,
        KeyCode::Char('G') | KeyCode::End => app.files_move(isize::MAX),
        KeyCode::Enter | KeyCode::Char('o' | 'l') | KeyCode::Right => {
            app.open_file(app.files_selected);
            app.focus = Focus::Content;
        }
        KeyCode::Char(' ') => app.open_file(app.files_selected),
        KeyCode::Esc | KeyCode::Tab => app.focus = Focus::Content,
        _ => return false,
    }
    true
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

fn pending(app: &mut App, p: Pending, key: KeyEvent) {
    if p == Pending::Open {
        let target = app.confirm_target.take();
        match (key.code, target) {
            (KeyCode::Char('y' | 'Y'), Some(t)) => app.open_external(&t),
            _ => app.notify("Cancelled"),
        }
        return;
    }
    let KeyCode::Char(c) = key.code else {
        app.notify("Cancelled");
        return;
    };
    match p {
        Pending::Mark if c.is_ascii_alphanumeric() => app.set_mark(c),
        Pending::Jump if c.is_ascii_alphanumeric() => app.jump_mark(c),
        Pending::Yank if app.keymap.action(&key) == Some(Action::CopyCode) => app.copy_code(None),
        Pending::Yank if c.is_ascii_digit() && c != '0' => {
            app.copy_code(c.to_digit(10).map(|d| d as usize))
        }
        _ => app.notify("Cancelled"),
    }
}

fn normal(app: &mut App, key: KeyEvent) {
    let action = app.keymap.action(&key);
    // Digits build a count (`5j`, `50%`); a bound digit only starts an action.
    if let KeyCode::Char(c @ '0'..='9') = key.code
        && (action.is_none() || app.count.is_some())
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        let d = c as usize - '0' as usize;
        app.count = Some(
            app.count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(d)
                .min(999_999),
        );
        return;
    }
    let count = app.count.take();
    if let Some(a) = action {
        run(app, a, count);
    }
}

/// Performs `action`; `count` repeats motions or picks a line / percent.
pub fn run(app: &mut App, action: Action, count: Option<usize>) {
    let n = count.unwrap_or(1).max(1) as isize;
    let page = app.view_height.max(1) as isize;
    let half = (page / 2).max(1);
    let repeat = |app: &mut App, f: fn(&mut App)| {
        for _ in 0..n.min(10_000) {
            f(app);
        }
    };
    match action {
        Action::Quit => app.quit = true,
        Action::Cancel => {
            if count.is_some() {
                app.notify("Count cleared");
            } else if app.link_focus.is_some() {
                app.link_focus = None;
            } else if !app.search.query.is_empty() {
                app.clear_search();
            } else {
                let q = app.keymap.label(Action::Quit);
                app.notify(format!("Press {q} to quit"));
            }
        }
        Action::Down => app.scroll_by(n),
        Action::Up => app.scroll_by(-n),
        Action::HalfDown => app.scroll_by(half.saturating_mul(n)),
        Action::HalfUp => app.scroll_by(-half.saturating_mul(n)),
        Action::PageDown => app.scroll_by(page.saturating_mul(n)),
        Action::PageUp => app.scroll_by(-page.saturating_mul(n)),
        Action::Top | Action::Bottom => match count {
            Some(line) => app.goto_line(line),
            None if action == Action::Top => app.scroll_to(0),
            None => app.scroll_to(usize::MAX),
        },
        Action::Percent => match count {
            Some(p) => app.goto_percent(p),
            None => app.notify("Type a number first, e.g. 50%"),
        },
        Action::NextHeading => repeat(app, |a| a.jump_heading(true)),
        Action::PrevHeading => repeat(app, |a| a.jump_heading(false)),
        Action::Toc => app.toggle_toc(),
        Action::Files => app.toggle_files(),
        Action::Finder => app.open_finder(),
        Action::NextFile => app.step_file(n),
        Action::PrevFile => app.step_file(-n),
        Action::FocusPanel => {
            if app.panel.is_some() {
                app.focus = Focus::Panel;
            }
        }
        Action::Search => app.start_search(),
        Action::NextMatch => repeat(app, |a| a.next_match(true)),
        Action::PrevMatch => repeat(app, |a| a.next_match(false)),
        Action::NextLink => repeat(app, |a| a.cycle_link(true)),
        Action::PrevLink => repeat(app, |a| a.cycle_link(false)),
        Action::Follow => match app.focused_link_id() {
            Some(id) => app.follow_link(id),
            None => app.scroll_by(n),
        },
        Action::OpenExternal => match app.focused_link_id() {
            Some(id) => app.open_link_externally(id),
            None => app.notify("No link focused (Tab selects one)"),
        },
        Action::Back => repeat(app, App::back),
        Action::Forward => repeat(app, App::forward),
        Action::Mark => {
            app.pending = Some(Pending::Mark);
            app.notify("Set bookmark: press a letter");
        }
        Action::JumpMark => {
            app.pending = Some(Pending::Jump);
            app.notify("Jump to bookmark: press a letter");
        }
        Action::CopyCode => match count {
            Some(c) => app.copy_code(Some(c)),
            None => {
                app.pending = Some(Pending::Yank);
                app.notify("Copy code: 1-9 block number, y = block on screen");
            }
        },
        Action::Edit => app.open_editor = true,
        Action::Reload => match app.reload() {
            Ok(()) if app.fetch.is_none() => app.notify("Reloaded"),
            Ok(()) => {}
            Err(e) => app.notify(format!("Reload failed: {e:#}")),
        },
        Action::Watch => app.toggle_watch(),
        Action::FocusMode => app.toggle_full_width(),
        Action::Theme => app.cycle_theme(),
        Action::Mouse => {
            app.mouse = !app.mouse;
            let state = if app.mouse {
                "on"
            } else {
                "off (text selection enabled)"
            };
            app.notify(format!("Mouse {state}"));
        }
        Action::Info => app.mode = Mode::Info,
        Action::Help => {
            app.help_scroll = 0;
            app.mode = Mode::Help;
        }
    }
}

pub fn handle_mouse(app: &mut App, m: MouseEvent) {
    app.count = None;
    // A click cancels a pending prompt instead of acting behind it.
    if matches!(m.kind, MouseEventKind::Down(_)) && app.pending.is_some() {
        app.cancel_pending();
        return;
    }
    let pos = Position::new(m.column, m.row);
    let in_toc = app.panel_rect.is_some_and(|r| r.contains(pos));
    match m.kind {
        MouseEventKind::ScrollDown if in_toc && app.panel == Some(Panel::Files) => {
            app.files_move(3)
        }
        MouseEventKind::ScrollUp if in_toc && app.panel == Some(Panel::Files) => app.files_move(-3),
        MouseEventKind::ScrollDown if in_toc => app.toc_move(3),
        MouseEventKind::ScrollUp if in_toc => app.toc_move(-3),
        MouseEventKind::ScrollDown => app.scroll_by(3),
        MouseEventKind::ScrollUp => app.scroll_by(-3),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(r) = app.panel_rect.filter(|_| in_toc) {
                // Mirrors the list offset logic in ui::draw_panel.
                if app.panel == Some(Panel::Files) {
                    let row = (m.row - r.y) as usize + app.files_offset(r.height as usize);
                    if row < app.files.len() {
                        app.open_file(row);
                    }
                } else {
                    let row = (m.row - r.y) as usize + app.toc_offset(r.height as usize);
                    if row < app.rendered.headings.len() {
                        app.toc_jump(row);
                    }
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
    let line = app
        .rendered
        .lines
        .get(app.scroll + (pos.y - r.y) as usize)?;
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
