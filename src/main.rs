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
    /// Files, a directory, a URL or `gh:owner/repo` to view; `-` or nothing reads stdin.
    /// `file.md#section` opens at a heading.
    #[arg(value_name = "PATH")]
    paths: Vec<PathBuf>,

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
    let format = cli
        .format
        .as_deref()
        .map(|name| parser::Format::from_name(name).context("unknown format"))
        .transpose()?;
    let viewer = std::io::stdout().is_terminal() && !cli.print;
    let inputs = load_inputs(&cli.paths, format, viewer)?;

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
        if let Some(w) = cli.width {
            cfg.max_width = w;
        }
        cfg.watch |= cli.watch;
        cfg.toc |= cli.toc;
        let Inputs {
            mut sources,
            dir_root,
            anchor,
        } = inputs;
        let files: Vec<PathBuf> = sources.iter().filter_map(|s| s.path.clone()).collect();
        let first = sources.remove(0);
        let mut app = tui::App::new(first, theme, depth, &cfg);
        if let Some((root, files)) = dir_root {
            app.set_root(root, files);
            app.panel = Some(tui::Panel::Files);
        } else if files.len() > 1 {
            let n = files.len();
            app.set_file_list(files);
            app.notify(format!("1 of {n} files: }} next, {{ previous, F list"));
        }
        if let Some(a) = anchor {
            app.start_at_anchor(a);
        }
        return tui::run(app, cfg.images && std::io::stdin().is_terminal());
    }

    for w in &cfg.warnings {
        eprintln!("mdvw: warning: {w}");
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
    let opts = mdvw::PrintOptions {
        width,
        theme: &theme,
        depth,
        hyperlinks: cfg.hyperlinks && !cli.no_hyperlinks,
    };
    let many = inputs.sources.len() > 1;
    let mut text = String::new();
    for (i, src) in inputs.sources.iter().enumerate() {
        if many {
            let sep = if i > 0 { "\n" } else { "" };
            text.push_str(&format!("{sep}==> {} <==\n\n", src.location()));
        }
        text.push_str(&mdvw::render_to_string(&src.text, src.format, &opts));
    }
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
    for (key, desc) in tui::Keymap::default().help() {
        writeln!(w, ".TP\n\\fB{}\\fR\n{}", esc(&key), esc(desc))?;
    }
    writeln!(w, ".SH FILES")?;
    writeln!(
        w,
        ".TP\n\\fI~/.config/mdvw/config.toml\\fR\nOptional configuration (theme, max_width, mouse, watch, hyperlinks, toc, remember_position, images, and a [keys] table mapping action names to keys)."
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

/// Documents named on the command line.
struct Inputs {
    sources: Vec<tui::Source>,
    /// Directory mode: root and its documents.
    dir_root: Option<(PathBuf, Vec<PathBuf>)>,
    /// Heading to open at (`file.md#install`).
    anchor: Option<String>,
}

fn load_inputs(paths: &[PathBuf], format: Option<parser::Format>, viewer: bool) -> Result<Inputs> {
    let mut inputs = Inputs {
        sources: Vec::new(),
        dir_root: None,
        anchor: None,
    };
    let with_format = |mut s: tui::Source| {
        if let Some(f) = format {
            s.format = f;
        }
        s
    };
    if paths.is_empty() || (paths.len() == 1 && paths[0].as_os_str() == "-") {
        inputs.sources.push(tui::Source {
            path: None,
            url: None,
            text: read_stdin()?,
            format: format.unwrap_or(parser::Format::Markdown),
        });
        return Ok(inputs);
    }
    let remote = |p: &PathBuf| mdvw::links::is_remote(&p.to_string_lossy()) && !p.exists();
    if paths.len() > 1 {
        if paths.iter().any(|p| p.as_os_str() == "-") {
            bail!("`-` (stdin) cannot be combined with other inputs");
        }
        // The viewer's file list holds local files only.
        if viewer && paths.iter().any(remote) {
            bail!("the viewer opens one URL at a time (use -p to print several)");
        }
    }
    for (i, p) in paths.iter().enumerate() {
        let s = p.to_string_lossy();
        if remote(p) {
            let (url, anchor) = match s.rsplit_once('#') {
                Some((u, a)) if !a.is_empty() => (u, Some(a.to_string())),
                _ => (s.as_ref(), None),
            };
            if i == 0 {
                inputs.anchor = anchor;
            }
            inputs
                .sources
                .push(with_format(tui::Source::from_url(url)?));
            continue;
        }
        if p.is_dir() {
            if paths.len() > 1 {
                bail!("{} is a directory; give it on its own", p.display());
            }
            // A directory opens its README/index (or first document) with a file panel.
            let files = files::scan(p);
            let idx = files::default_file(&files)
                .with_context(|| format!("no documents found in {}", p.display()))?;
            let path = p.join(&files[idx]);
            inputs
                .sources
                .push(with_format(tui::Source::from_file(&path)?));
            inputs.dir_root = Some((p.clone(), files));
            continue;
        }
        let (path, anchor) = split_anchor(p);
        if i == 0 {
            inputs.anchor = anchor;
        }
        let src = tui::Source::from_file(&path)
            .with_context(|| format!("cannot open {}", p.display()))?;
        inputs.sources.push(with_format(src));
    }
    Ok(inputs)
}

/// `notes.md#setup` → (`notes.md`, `setup`) unless a file has that literal name.
fn split_anchor(p: &std::path::Path) -> (PathBuf, Option<String>) {
    let s = p.to_string_lossy();
    if !p.exists()
        && let Some((file, anchor)) = s.rsplit_once('#')
        && !anchor.contains(['/', '\\'])
        && !file.is_empty()
    {
        let anchor = (!anchor.is_empty()).then(|| anchor.to_string());
        return (PathBuf::from(file), anchor);
    }
    (p.to_path_buf(), None)
}

fn read_stdin() -> Result<String> {
    if std::io::stdin().is_terminal() {
        bail!("no input file given (try `mdvw README.md` or `mdvw --help`)");
    }
    let mut buf = Vec::new();
    std::io::stdin()
        .read_to_end(&mut buf)
        .context("cannot read stdin")?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn anchors_split_only_for_missing_files() {
        let (p, a) = split_anchor(std::path::Path::new("README.md#install"));
        assert_eq!(
            (p, a.as_deref()),
            (PathBuf::from("README.md"), Some("install"))
        );
        let (p, a) = split_anchor(std::path::Path::new("x.md#"));
        assert_eq!((p, a), (PathBuf::from("x.md"), None));
        let (p, a) = split_anchor(std::path::Path::new("Cargo.toml"));
        assert_eq!((p, a), (PathBuf::from("Cargo.toml"), None));
    }

    #[test]
    fn man_page_has_keys_section() {
        let mut out = Vec::new();
        write_man(&mut out).unwrap();
        let man = String::from_utf8(out).unwrap();
        assert!(man.contains(".SH KEYS") && man.contains(".SH OPTIONS"));
    }
}
