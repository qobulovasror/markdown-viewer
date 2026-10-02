use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use clap::{CommandFactory, Parser, ValueEnum};

use mdvw::color::ColorDepth;
use mdvw::theme::{self, Theme};
use mdvw::{config, files, parser, tui};

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

    /// Input format (default: from file extension, Markdown for stdin).
    #[arg(short, long, value_parser = clap::builder::PossibleValuesParser::new(parser::FORMAT_NAMES))]
    format: Option<String>,

    /// Ignore the config file.
    #[arg(long)]
    no_config: bool,

    /// Print shell completions to stdout.
    #[arg(long, value_name = "SHELL", exclusive = true)]
    completions: Option<clap_complete::Shell>,

    /// Print the man page (roff) to stdout.
    #[arg(long, exclusive = true)]
    man: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mdvw: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Some(shell) = cli.completions {
        clap_complete::generate(shell, &mut Cli::command(), "mdvw", &mut std::io::stdout());
        return Ok(());
    }
    if cli.man {
        return write_man(&mut std::io::stdout().lock());
    }
    let mut cfg = if cli.no_config {
        config::Config::default()
    } else {
        config::Config::load()?
    };
    // A directory opens its README/index (or first document) with a file panel.
    let mut path = cli.path.clone().filter(|p| p.as_os_str() != "-");
    let mut dir_root = None;
    if let Some(dir) = path.clone().filter(|p| p.is_dir()) {
        let files = files::scan(&dir);
        let idx = files::default_file(&files)
            .with_context(|| format!("no documents found in {}", dir.display()))?;
        path = Some(dir.join(&files[idx]));
        dir_root = Some((dir, files));
    }
    let src = read_input(path.as_ref())?;
    let format = match (&cli.format, &path) {
        (Some(name), _) => parser::Format::from_name(name).context("unknown format")?,
        (None, Some(p)) => parser::Format::from_path(p),
        (None, None) => parser::Format::Markdown,
    };

    let stdout = std::io::stdout();
    let tty = stdout.is_terminal();
    let theme = match cli.theme.as_ref().or(cfg.theme.as_ref()) {
        Some(name) => Theme::by_name(name).with_context(|| {
            format!(
                "unknown theme '{name}' (available: {})",
                theme::THEME_NAMES.join(", ")
            )
        })?,
        None if tty && std::io::stdin().is_terminal() => theme::auto(),
        None => Theme::default_dark(),
    };
    if tty && !cli.print {
        let depth = match cli.color {
            ColorChoice::Never => ColorDepth::None,
            _ => ColorDepth::detect(),
        };
        let source = tui::Source {
            path,
            text: src,
            format,
        };
        if let Some(w) = cli.width {
            cfg.max_width = w;
        }
        cfg.watch |= cli.watch;
        cfg.toc |= cli.toc;
        let mut app = tui::App::new(source, theme, depth, &cfg);
        if let Some((root, files)) = dir_root {
            app.set_root(root, files);
            app.panel = Some(tui::Panel::Files);
        }
        return tui::run(app, cfg.images && std::io::stdin().is_terminal());
    }

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
    let text = mdvw::render_to_string(
        &src,
        format,
        &mdvw::PrintOptions {
            width,
            theme: &theme,
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

/// Man page: clap's sections plus viewer keys and file locations.
fn write_man(w: &mut dyn Write) -> Result<()> {
    let man = clap_mangen::Man::new(Cli::command());
    man.render_title(w)?;
    man.render_name_section(w)?;
    man.render_synopsis_section(w)?;
    man.render_description_section(w)?;
    man.render_options_section(w)?;
    let esc = |s: &str| s.replace('\\', "\\\\").replace('-', "\\-");
    writeln!(w, ".SH KEYS")?;
    for (key, desc) in tui::HELP {
        writeln!(w, ".TP\n\\fB{}\\fR\n{}", esc(key), esc(desc))?;
    }
    writeln!(w, ".SH FILES")?;
    writeln!(
        w,
        ".TP\n\\fI~/.config/mdvw/config.toml\\fR\nOptional configuration (theme, max_width, mouse, watch, hyperlinks, toc, remember_position, images)."
    )?;
    writeln!(
        w,
        ".TP\n\\fI~/.local/state/mdvw/state.json\\fR\nSaved reading positions and bookmarks."
    )?;
    writeln!(w, ".SH ENVIRONMENT")?;
    writeln!(
        w,
        ".TP\n\\fBNO_COLOR\\fR\nDisable colors.\n.TP\n\\fBVISUAL\\fR, \\fBEDITOR\\fR\nEditor opened with the \\fBe\\fR key."
    )?;
    man.render_version_section(w)?;
    Ok(())
}

fn read_input(path: Option<&PathBuf>) -> Result<String> {
    let bytes = match path {
        Some(p) => std::fs::read(p).with_context(|| format!("cannot read {}", p.display()))?,
        _ => {
            if std::io::stdin().is_terminal() {
                bail!("no input file given (try `mdvw README.md` or `mdvw --help`)");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn man_page_has_keys_section() {
        let mut out = Vec::new();
        write_man(&mut out).unwrap();
        let man = String::from_utf8(out).unwrap();
        assert!(man.contains(".SH KEYS") && man.contains(".SH OPTIONS"));
    }
}
