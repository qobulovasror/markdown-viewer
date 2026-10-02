mod clipboard;
mod color;
mod config;
mod highlight;
mod links;
mod layout;
mod parser;
mod render;
mod search;
mod state;
mod theme;
mod tui;
mod watch;

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};

use color::ColorDepth;
use theme::Theme;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

/// Fast terminal viewer for Markdown and similar documents.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// File to view; `-` or nothing reads stdin.
    path: Option<PathBuf>,

    /// Print rendered output to stdout instead of opening the viewer.
    #[arg(short, long)]
    print: bool,

    /// Text width in columns (print: default terminal width; viewer: max width).
    #[arg(long)]
    width: Option<usize>,

    /// Color theme.
    #[arg(short, long, value_parser = clap::builder::PossibleValuesParser::new(theme::THEME_NAMES))]
    theme: Option<String>,

    /// When to use colors.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,

    /// Disable OSC 8 clickable links.
    #[arg(long)]
    no_hyperlinks: bool,

    /// Reload automatically when the file changes.
    #[arg(short, long)]
    watch: bool,

    /// Open the table of contents on start.
    #[arg(long)]
    toc: bool,

    /// Ignore the config file.
    #[arg(long)]
    no_config: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mdv: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut cfg = if cli.no_config {
        config::Config::default()
    } else {
        config::Config::load()?
    };
    let src = read_input(cli.path.as_ref())?;

    let stdout = std::io::stdout();
    let tty = stdout.is_terminal();
    let theme = match cli.theme.as_ref().or(cfg.theme.as_ref()) {
        Some(name) => Theme::by_name(name).with_context(|| {
            format!("unknown theme '{name}' (available: {})", theme::THEME_NAMES.join(", "))
        })?,
        None if tty && std::io::stdin().is_terminal() => theme::auto(),
        None => Theme::default_dark(),
    };
    if tty && !cli.print {
        let depth = match cli.color {
            ColorChoice::Never => ColorDepth::None,
            _ => ColorDepth::detect(),
        };
        let path = cli.path.filter(|p| p.as_os_str() != "-");
        let source = tui::Source { path, text: src };
        if let Some(w) = cli.width {
            cfg.max_width = w;
        }
        cfg.watch |= cli.watch;
        cfg.toc |= cli.toc;
        return tui::run(tui::App::new(source, theme, depth, &cfg));
    }

    let doc = parser::markdown::parse(&src);
    let width = cli.width.unwrap_or_else(|| {
        if tty {
            crossterm::terminal::size().map_or(80, |(w, _)| w as usize)
        } else {
            80
        }
    });
    let depth = match cli.color {
        ColorChoice::Never => ColorDepth::None,
        ColorChoice::Always => match ColorDepth::detect() {
            ColorDepth::None => ColorDepth::Ansi256,
            d => d,
        },
        ColorChoice::Auto if !tty => ColorDepth::None,
        ColorChoice::Auto => ColorDepth::detect(),
    };

    let rendered = layout::layout(
        &doc,
        &theme,
        &layout::Options {
            width,
            code_numbers: false,
        },
    );
    let text = render::ansi::render(
        &rendered,
        &render::ansi::AnsiOptions {
            depth,
            hyperlinks: cfg.hyperlinks && !cli.no_hyperlinks,
        },
    );
    let mut lock = stdout.lock();
    if let Err(e) = lock.write_all(text.as_bytes()).and_then(|_| lock.flush())
        && e.kind() != std::io::ErrorKind::BrokenPipe
    {
        return Err(e.into());
    }
    Ok(())
}

fn read_input(path: Option<&PathBuf>) -> Result<String> {
    let bytes = match path {
        Some(p) if p.as_os_str() != "-" => {
            if p.is_dir() {
                bail!("{} is a directory", p.display());
            }
            std::fs::read(p).with_context(|| format!("cannot read {}", p.display()))?
        }
        _ => {
            if std::io::stdin().is_terminal() {
                bail!("no input file given (try `mdv README.md` or `mdv --help`)");
            }
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .context("cannot read stdin")?;
            buf
        }
    };
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
