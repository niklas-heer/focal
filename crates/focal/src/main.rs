//! `focal`: open a Markdown file from the terminal in a calm, live-rendered
//! editor window.

mod accessibility;
mod bar;
mod document;
mod editor;
mod folder;
mod grid;
mod highlight;
mod menus;
mod prefix;
mod settings;
mod spell;
mod switcher;
mod table_view;
mod theme;
#[cfg(test)]
mod ui_tests;
mod workspace;

use std::borrow::Cow;
use std::ffi::OsString;
use std::io::{IsTerminal as _, Read as _};
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context as _, Result, bail};
use gpui_kit::{
    App, AppContext as _, Bounds, TitlebarOptions, WindowBounds, WindowOptions, point, px, size,
};

use crate::document::Document;
use crate::editor::Quit;
use crate::workspace::Workspace;

const USAGE: &str = "\
Usage: focal [--wait] [FILE | FOLDER]

Opens FILE (created on first save if missing), or FOLDER with a sidebar of its
Markdown files. With neither, opens text piped to standard input as an
untitled document.

Options:
  -w, --wait     Stay in the foreground until the window closes (for $EDITOR)
  -h, --help     Show this help
  -V, --version  Show the version";

/// Set on the detached child so it does not detach again.
const FOREGROUND_ENV: &str = "FOCAL_FOREGROUND";

struct Args {
    path: Option<PathBuf>,
    wait: bool,
    /// A temporary file holding piped input, handed to the detached child.
    untitled_from: Option<PathBuf>,
}

enum Source {
    File(PathBuf),
    Folder(PathBuf),
    Untitled(String),
}

fn main() {
    if let Err(error) = run() {
        eprintln!("focal: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let Some(args) = parse_args(std::env::args_os().skip(1))? else {
        return Ok(());
    };
    let source = if let Some(temp) = &args.untitled_from {
        let text = document::read(temp)?;
        std::fs::remove_file(temp).ok();
        Source::Untitled(text)
    } else if let Some(path) = &args.path {
        let path = std::path::absolute(path).context("resolving the path")?;
        if path.is_dir() {
            Source::Folder(path)
        } else {
            Source::File(path)
        }
    } else if std::io::stdin().is_terminal() {
        Source::Untitled(String::new())
    } else {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("reading standard input as UTF-8")?;
        Source::Untitled(text)
    };

    if args.wait || std::env::var_os(FOREGROUND_ENV).is_some() {
        run_app(source);
        return Ok(());
    }
    detach(&source)
}

fn parse_args(args: impl Iterator<Item = OsString>) -> Result<Option<Args>> {
    let mut parsed = Args {
        path: None,
        wait: false,
        untitled_from: None,
    };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return Ok(None);
            }
            Some("-V" | "--version") => {
                println!("focal {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            Some("-w" | "--wait") => parsed.wait = true,
            Some("--untitled-from") => {
                parsed.untitled_from =
                    Some(args.next().context("--untitled-from needs a path")?.into());
            }
            Some(flag) if flag.starts_with('-') && flag != "-" => {
                bail!("unknown option {flag}\n\n{USAGE}")
            }
            _ if parsed.path.is_some() => bail!("only one file can be opened at a time\n\n{USAGE}"),
            _ => parsed.path = Some(arg.into()),
        }
    }
    Ok(Some(parsed))
}

/// Relaunches `focal` in the background so the terminal is free again.
fn detach(source: &Source) -> Result<()> {
    let mut command = Command::new(std::env::current_exe().context("locating focal")?);
    match source {
        Source::File(path) | Source::Folder(path) => {
            command.arg(path);
        }
        Source::Untitled(text) => {
            let temp = std::env::temp_dir().join(format!("focal-stdin-{}.md", std::process::id()));
            std::fs::write(&temp, text).context("buffering standard input")?;
            command.arg("--untitled-from").arg(temp);
        }
    }
    command
        .env(FOREGROUND_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("starting the Focal window")?;
    Ok(())
}

fn run_app(source: Source) {
    gpui_kit::application().run(move |cx: &mut App| {
        gpui_kit::init(cx);
        load_fonts(cx);
        editor::bind_keys(cx);
        workspace::bind_keys(cx);
        switcher::bind_keys(cx);
        settings::init(cx);
        cx.on_action(|_: &Quit, cx| cx.quit());
        menus::set_menus(cx);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let (title, folder, opened) = match source {
            Source::Folder(root) => {
                let title = root.file_name().map_or_else(
                    || root.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                (title, Some(root), None)
            }
            Source::File(path) => match Document::open(path) {
                Ok(opened) => (opened.0.title(), None, Some(opened)),
                Err(error) => {
                    eprintln!("focal: {error:#}");
                    cx.quit();
                    return;
                }
            },
            Source::Untitled(text) => {
                let document = Document::untitled();
                (document.title(), None, Some((document, text)))
            }
        };
        let bounds = Bounds::centered(None, size(px(860.), px(920.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(title.into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.), px(12.))),
            }),
            ..WindowOptions::default()
        };
        let window = gpui_kit::open_window(options, cx, |window, cx| {
            let workspace = cx.new(|cx| match (folder, opened) {
                (Some(root), _) => Workspace::new_folder(root, window, cx),
                (None, Some((document, text))) => Workspace::new(document, text, window, cx),
                (None, None) => Workspace::new(Document::untitled(), String::new(), window, cx),
            });
            Workspace::focus_editor(&workspace, window, cx);
            workspace
        });
        if let Err(error) = window {
            eprintln!("focal: could not open a window: {error:#}");
            cx.quit();
            return;
        }
        cx.activate(true);
    });
}

fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/iAWriterQuattroS-Regular.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/iAWriterQuattroS-Italic.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/iAWriterDuoS-Bold.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/iAWriterDuoS-BoldItalic.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/iAWriterMonoS-Regular.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/iAWriterMonoS-Bold.ttf"
        )),
        Cow::Borrowed(include_bytes!(
            "../../../assets/fonts/iAWriterMonoS-Italic.ttf"
        )),
    ];
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("focal: could not load the iA Writer fonts: {error:#}");
    }
}
