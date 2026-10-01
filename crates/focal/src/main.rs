//! `focal`: open a Markdown file from the terminal in a calm, live-rendered
//! editor window.

mod accessibility;
mod bar;
mod cli_install;
mod diagram;
mod document;
mod drafts;
mod editor;
mod export;
mod find_bar;
mod fold_memory;
mod folder;
mod grid;
mod highlight;
mod instance;
mod islands;
mod mac;
mod maps;
mod math;
mod menus;
mod prefix;
mod print;
mod recent;
mod settings;
mod spell;
mod stl;
mod switcher;
mod table_view;
mod theme;
mod tools;
#[cfg(test)]
mod ui_tests;
mod updates;
mod vega;
mod windows;
mod workspace;

use std::borrow::Cow;
use std::ffi::OsString;
use std::io::{ErrorKind, IsTerminal as _, Read as _};
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result, bail};
use gpui_kit::App;

use crate::instance::Request;

const USAGE: &str = "\
Usage: focal [--wait] [FILE | FOLDER]

Opens FILE (created on first save if missing), or FOLDER with a sidebar of its
Markdown files. With neither, opens text piped to standard input as an
untitled document.

Options:
  -w, --wait     Stay in the foreground until the window closes (for $EDITOR)
  -h, --help     Show this help
  -V, --version  Show the version";

/// Makes `focal` run the app in the foreground with its request, instead of
/// handing it to a running instance (development and `mise run run`).
const FOREGROUND_ENV: &str = "FOCAL_FOREGROUND";

struct Args {
    path: Option<PathBuf>,
    wait: bool,
    /// Internal: run as the instance other `focal` calls forward to.
    serve: bool,
}

/// How this process runs the app.
enum Launch {
    /// Started by `focal`, which forwards its request once the socket is up.
    Serve,
    /// Started by Finder or the Dock; files arrive as open events.
    Finder,
    /// In the foreground with one request (`FOCAL_FOREGROUND`).
    Open(Request),
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
    if args.serve {
        run_app(Launch::Serve);
        return Ok(());
    }
    // Finder and the Dock start the app with no arguments, from launchd.
    if args.path.is_none() && !args.wait && std::os::unix::process::parent_id() == 1 {
        run_app(Launch::Finder);
        return Ok(());
    }
    let request = request(&args)?;
    if std::env::var_os(FOREGROUND_ENV).is_some() {
        run_app(Launch::Open(request));
        return Ok(());
    }
    send(&request)
}

fn request(args: &Args) -> Result<Request> {
    let path = match &args.path {
        Some(path) => Some(std::path::absolute(path).context("resolving the path")?),
        None => None,
    };
    let untitled = if path.is_none() && !std::io::stdin().is_terminal() {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .context("reading standard input")?;
        // Piped text may come in any encoding, as files do.
        Some(focal_core::encoding::decode(&bytes).0)
    } else {
        None
    };
    Ok(Request {
        path,
        untitled,
        wait: args.wait,
    })
}

fn parse_args(args: impl Iterator<Item = OsString>) -> Result<Option<Args>> {
    let mut parsed = Args {
        path: None,
        wait: false,
        serve: false,
    };
    for arg in args {
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
            Some("--serve") => parsed.serve = true,
            Some(flag) if flag.starts_with('-') && flag != "-" => {
                bail!("unknown option {flag}\n\n{USAGE}")
            }
            _ if parsed.path.is_some() => bail!("only one file can be opened at a time\n\n{USAGE}"),
            _ => parsed.path = Some(arg.into()),
        }
    }
    Ok(Some(parsed))
}

/// Hands the request to the running Focal, starting one if none runs.
fn send(request: &Request) -> Result<()> {
    let socket = instance::socket_path();
    let absent = |error: &std::io::Error| {
        matches!(
            error.kind(),
            ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::UnexpectedEof
        )
    };
    match instance::forward(&socket, request) {
        Ok(()) => return Ok(()),
        Err(error) if absent(&error) => {}
        Err(error) => bail!("{error}"),
    }
    start_instance()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        std::thread::sleep(Duration::from_millis(40));
        match instance::forward(&socket, request) {
            Ok(()) => return Ok(()),
            Err(error) if absent(&error) && Instant::now() < deadline => {}
            Err(error) => bail!("could not reach the Focal window: {error}"),
        }
    }
}

/// Starts the instance in the background, so the terminal is free again.
fn start_instance() -> Result<()> {
    Command::new(std::env::current_exe().context("locating focal")?)
        .arg("--serve")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("starting Focal")?;
    Ok(())
}

fn run_app(launch: Launch) {
    let app = gpui_kit::application();
    let (urls_tx, urls_rx) = async_channel::unbounded::<Vec<String>>();
    app.on_open_urls(move |urls| {
        let _ = urls_tx.try_send(urls);
    });
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);
        mac::force_appearance();
        load_fonts(cx);
        editor::bind_keys(cx);
        workspace::bind_keys(cx);
        switcher::bind_keys(cx);
        find_bar::bind_keys(cx);
        windows::bind_keys(cx);
        settings::init(cx);
        windows::init(cx);
        cli_install::init(cx);
        updates::init(cx);
        recent::init(cx);
        drafts::init(cx);
        fold_memory::init(cx);
        menus::init(cx);
        serve(cx);
        drafts::restore(cx);
        cx.spawn(async move |cx| {
            while let Ok(urls) = urls_rx.recv().await {
                let paths = urls.iter().filter_map(|url| instance::file_url_path(url));
                let requests: Vec<Request> = paths
                    .map(|path| Request {
                        path: Some(path),
                        ..Request::default()
                    })
                    .collect();
                cx.update(|cx| {
                    for request in requests {
                        windows::open(request, None, cx);
                    }
                });
            }
        })
        .detach();
        match launch {
            Launch::Open(request) => windows::open(request, None, cx),
            Launch::Serve | Launch::Finder => {
                // Started by `focal`, a request follows at once; started from
                // Finder, opened files follow. Without either, quit or open
                // an empty document.
                let finder = matches!(launch, Launch::Finder);
                let wait = Duration::from_millis(if finder { 600 } else { 10_000 });
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(wait).await;
                    cx.update(|cx| {
                        if cx.windows().is_empty() {
                            if finder {
                                windows::open(Request::default(), None, cx);
                            } else {
                                cx.quit();
                            }
                        }
                    });
                })
                .detach();
            }
        }
    });
}

/// Accepts requests from other `focal` calls for as long as the app runs.
fn serve(cx: &mut App) {
    let socket = instance::socket_path();
    let listener = match instance::listen(&socket) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("focal: not accepting other focal calls: {error}");
            return;
        }
    };
    let (requests_tx, requests_rx) = async_channel::unbounded();
    std::thread::spawn(move || {
        while let Ok(pair) = instance::accept(&listener) {
            if requests_tx.send_blocking(pair).is_err() {
                break;
            }
        }
    });
    cx.spawn(async move |cx| {
        while let Ok((request, responder)) = requests_rx.recv().await {
            cx.update(|cx| windows::open(request, Some(responder), cx));
        }
    })
    .detach();
    cx.on_app_quit(move |_| {
        let _ = std::fs::remove_file(&socket);
        async {}
    })
    .detach();
}

fn load_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = theme::FONTS
        .iter()
        .map(|font| Cow::Borrowed(font.data))
        .collect();
    if let Err(error) = cx.text_system().add_fonts(fonts) {
        eprintln!("focal: could not load the iA Writer fonts: {error:#}");
    }
}
