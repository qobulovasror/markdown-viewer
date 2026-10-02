//! Keyboard and mouse bindings.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};

use super::app::{App, Mode};

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
        Mode::Normal => normal(app, key),
    }
}

fn normal(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let page = app.view_height.max(1) as isize;
    let half = (page / 2).max(1);
    match key.code {
        KeyCode::Char('c') if ctrl => app.quit = true,
        KeyCode::Char('q') | KeyCode::Esc => app.quit = true,
        KeyCode::Char('j') | KeyCode::Down | KeyCode::Enter => app.scroll_by(1),
        KeyCode::Char('k') | KeyCode::Up => app.scroll_by(-1),
        KeyCode::Char('d') if ctrl => app.scroll_by(half),
        KeyCode::Char('u') if ctrl => app.scroll_by(-half),
        KeyCode::Char('f') if ctrl => app.scroll_by(page),
        KeyCode::Char('b') if ctrl => app.scroll_by(-page),
        KeyCode::Char('d') => app.scroll_by(half),
        KeyCode::Char('u') => app.scroll_by(-half),
        KeyCode::Char(' ' | 'f') | KeyCode::PageDown => app.scroll_by(page),
        KeyCode::Char('b') | KeyCode::PageUp => app.scroll_by(-page),
        KeyCode::Char('g') | KeyCode::Home => app.scroll_to(0),
        KeyCode::Char('G') | KeyCode::End => app.scroll_to(usize::MAX),
        KeyCode::Char('?') => app.mode = Mode::Help,
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
    match m.kind {
        MouseEventKind::ScrollDown => app.scroll_by(3),
        MouseEventKind::ScrollUp => app.scroll_by(-3),
        _ => {}
    }
}
