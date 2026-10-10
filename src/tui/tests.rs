//! Viewer behavior driven through key events on a test terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

use super::app::{App, Mode, Source};
use super::{keys, ui};
use crate::color::ColorDepth;
use crate::config::{Config, KeyList};
use crate::parser::Format;
use crate::theme::Theme;

fn doc() -> String {
    let mut s = String::from("# Top\n\n[jump](#end)\n\n");
    for i in 0..200 {
        s.push_str(&format!("line {i}\n\n"));
    }
    s.push_str("# End\n\nlast\n");
    s
}

fn app_with(cfg: Config, text: &str) -> (App, Terminal<TestBackend>) {
    let cfg = Config {
        remember_position: false,
        ..cfg
    };
    let source = Source {
        path: None,
        url: None,
        text: text.to_string(),
        format: Format::Markdown,
    };
    let app = App::new(source, Theme::default_dark(), ColorDepth::None, &cfg);
    let term = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let mut t = (app, term);
    draw(&mut t);
    t
}

fn app() -> (App, Terminal<TestBackend>) {
    app_with(Config::default(), &doc())
}

fn draw((app, term): &mut (App, Terminal<TestBackend>)) {
    term.draw(|f| ui::draw(f, app)).unwrap();
}

fn press(t: &mut (App, Terminal<TestBackend>), keys_: &str) {
    for c in keys_.chars() {
        send(t, KeyCode::Char(c));
    }
}

fn send(t: &mut (App, Terminal<TestBackend>), code: KeyCode) {
    keys::handle_key(&mut t.0, KeyEvent::new(code, KeyModifiers::NONE));
    draw(t);
}

fn screen(t: &(App, Terminal<TestBackend>)) -> String {
    let buf = t.1.backend().buffer();
    buf.content.iter().map(|c| c.symbol()).collect()
}

#[test]
fn counts_repeat_motions_and_pick_lines() {
    let mut t = app();
    press(&mut t, "5j");
    assert_eq!(t.0.scroll, 5);
    press(&mut t, "12k");
    assert_eq!(t.0.scroll, 0);
    press(&mut t, "10G");
    assert_eq!(t.0.scroll, 9);
    press(&mut t, "100%");
    assert_eq!(t.0.scroll, t.0.max_scroll());
    press(&mut t, "0%");
    assert_eq!(t.0.scroll, 0);
    press(&mut t, "3");
    assert_eq!(t.0.count, Some(3));
    send(&mut t, KeyCode::Esc);
    assert_eq!(t.0.count, None);
    press(&mut t, "j");
    assert_eq!(t.0.scroll, 1);
}

#[test]
fn panel_keys_drop_a_pending_count() {
    let mut t = app();
    press(&mut t, "t5j");
    send(&mut t, KeyCode::Tab);
    assert_eq!(t.0.count, None);
    press(&mut t, "j");
    assert_eq!(t.0.scroll, 1);
}

#[test]
fn escape_does_not_quit() {
    let mut t = app();
    send(&mut t, KeyCode::Esc);
    assert!(!t.0.quit);
    assert!(t.0.message().unwrap().contains("to quit"));
    press(&mut t, "q");
    assert!(t.0.quit);
}

#[test]
fn back_and_forward() {
    let mut t = app();
    send(&mut t, KeyCode::Tab);
    send(&mut t, KeyCode::Enter);
    let end = t.0.scroll;
    assert!(end > 100);
    send(&mut t, KeyCode::Backspace);
    assert_eq!(t.0.scroll, 0);
    press(&mut t, "L");
    assert_eq!(t.0.scroll, end);
    press(&mut t, "L");
    assert_eq!(t.0.message(), Some("No next location"));
}

#[test]
fn search_input_edits_and_recalls() {
    let mut t = app();
    press(&mut t, "/line 15");
    send(&mut t, KeyCode::Enter);
    assert_eq!(t.0.search.query, "line 15");
    assert_eq!(t.0.mode, Mode::Normal);
    press(&mut t, "/xx");
    send(&mut t, KeyCode::Up);
    assert_eq!(t.0.search.input.text, "line 15");
    send(&mut t, KeyCode::Left);
    send(&mut t, KeyCode::Backspace);
    assert_eq!(t.0.search.input.text, "line 5");
    assert!(screen(&t).contains("/line ▏5"));
    send(&mut t, KeyCode::Enter);
    assert!(!t.0.search.matches.is_empty());
}

#[test]
fn help_scrolls_on_small_terminals() {
    let mut t = app();
    t.1 = Terminal::new(TestBackend::new(60, 12)).unwrap();
    press(&mut t, "?");
    assert_eq!(t.0.mode, Mode::Help);
    press(
        &mut t,
        "jjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjjj",
    );
    assert!(screen(&t).contains("Prefix a count"));
    press(&mut t, "x");
    assert_eq!(t.0.mode, Mode::Normal);
}

#[test]
fn custom_keys_apply() {
    let mut cfg = Config::default();
    cfg.keys.insert("quit".into(), KeyList::One("x".into()));
    cfg.keys.insert("bogus".into(), KeyList::One("y".into()));
    let mut t = app_with(cfg, &doc());
    assert!(t.0.message().unwrap().contains("unknown action 'bogus'"));
    press(&mut t, "q");
    assert!(!t.0.quit);
    press(&mut t, "x");
    assert!(t.0.quit);
}

#[test]
fn risky_link_needs_confirmation_and_click_cancels() {
    let mut t = app_with(Config::default(), "[run](./evil.command)\n");
    send(&mut t, KeyCode::Tab);
    send(&mut t, KeyCode::Enter);
    assert!(t.0.message().unwrap().contains("(y/N)"));
    assert!(t.0.confirm_target.is_some());
    let click = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: 5,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    keys::handle_mouse(&mut t.0, click);
    assert!(t.0.confirm_target.is_none() && t.0.pending.is_none());
    // A later `y` must not open anything; it starts a code-block copy instead.
    press(&mut t, "y");
    assert!(t.0.message().unwrap().starts_with("Copy code"));
}

#[test]
fn starts_at_anchor() {
    let mut t = app_with(Config::default(), "x");
    t.0 = {
        let cfg = Config {
            remember_position: false,
            ..Config::default()
        };
        let src = Source {
            path: None,
            url: None,
            text: doc(),
            format: Format::Markdown,
        };
        let mut a = App::new(src, Theme::default_dark(), ColorDepth::None, &cfg);
        a.start_at_anchor("End".into());
        a
    };
    draw(&mut t);
    assert!(t.0.scroll > 100);
}

#[test]
fn steps_through_file_list() {
    let dir = std::env::temp_dir().join(format!("mdvw-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let files: Vec<std::path::PathBuf> = ["a.md", "b.md", "c.md"]
        .iter()
        .map(|n| {
            let p = dir.join(n);
            std::fs::write(&p, format!("# {n}\n")).unwrap();
            p
        })
        .collect();
    let cfg = Config {
        remember_position: false,
        ..Config::default()
    };
    let src = Source::from_file(&files[0]).unwrap();
    let mut t = (
        App::new(src, Theme::default_dark(), ColorDepth::None, &cfg),
        Terminal::new(TestBackend::new(80, 24)).unwrap(),
    );
    let mut list = files.clone();
    list.push(files[0].clone());
    t.0.set_file_list(list);
    assert_eq!(t.0.files.len(), 3);
    draw(&mut t);
    press(&mut t, "}");
    assert_eq!(t.0.source.name(), "b.md");
    press(&mut t, "2}");
    assert_eq!(t.0.source.name(), "a.md");
    press(&mut t, "{");
    assert_eq!(t.0.source.name(), "c.md");
    assert_eq!(t.0.current_file(), Some(2));
    // Off the list, `}` opens the selected entry first.
    let other = dir.join("other.md");
    std::fs::write(&other, "# o\n").unwrap();
    t.0.open_source(Source::from_file(&other).unwrap());
    press(&mut t, "}");
    assert_eq!(t.0.source.name(), "c.md");
    std::fs::remove_dir_all(dir).ok();
}

#[cfg(feature = "remote")]
#[test]
fn links_in_web_documents_resolve_against_their_url() {
    let mut t = app_with(Config::default(), "[a](../b.md) [s](#x)\n\n# X\n");
    t.0.source.url = Some("https://h.dev/docs/a.md".into());
    assert_eq!(
        t.0.link_target("../b.md"),
        crate::links::Target::External("https://h.dev/b.md".into())
    );
    assert_eq!(
        t.0.link_target("#x"),
        crate::links::Target::Anchor("x".into())
    );
    // `F` and `}` have no folder to list for a web document.
    press(&mut t, "}");
    assert_eq!(t.0.message(), Some("No file list for a web document"));
}
