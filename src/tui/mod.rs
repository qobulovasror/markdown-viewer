//! Interactive full-screen viewer.

mod app;
mod keys;
mod ui;

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::execute;

pub use app::{App, Source};

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = ratatui::init();
    // ratatui::init's hook restores the terminal; we also release the mouse.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        hook(info);
    }));
    if app.mouse {
        execute!(std::io::stdout(), EnableMouseCapture)?;
    }

    let result = (|| -> Result<()> {
        while !app.quit {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if event::poll(Duration::from_millis(250))? {
                let mouse_before = app.mouse;
                match event::read()? {
                    Event::Key(k) => keys::handle_key(&mut app, k),
                    Event::Mouse(m) => keys::handle_mouse(&mut app, m),
                    Event::Resize(..) => {}
                    _ => {}
                }
                if app.mouse != mouse_before {
                    if app.mouse {
                        execute!(std::io::stdout(), EnableMouseCapture)?;
                    } else {
                        execute!(std::io::stdout(), DisableMouseCapture)?;
                    }
                }
            }
            app.tick();
        }
        Ok(())
    })();

    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}
