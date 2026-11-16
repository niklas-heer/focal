//! Focal's windows: one per request, with the window that already shows a
//! file or folder brought forward instead of a second one, and waiting
//! `focal --wait` calls answered when their window closes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Global, KeyBinding, PathPromptOptions, TitlebarOptions,
    WeakEntity, WindowBounds, WindowId, WindowOptions, actions, point, px,
};

use crate::document::Document;
use crate::instance::{Request, Responder};
use crate::window_frame::{self, Frame};
use crate::workspace::Workspace;

actions!(focal, [NewWindow, OpenFiles]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-n", NewWindow, None),
        KeyBinding::new("cmd-o", OpenFiles, None),
    ]);
}

#[derive(Default)]
struct Windows {
    open: Vec<(AnyWindowHandle, WeakEntity<Workspace>)>,
    waiting: HashMap<WindowId, Vec<Responder>>,
    /// Where the session's first window opened; later windows cascade from it.
    anchor: Option<(f32, f32)>,
}

impl Global for Windows {}

/// Tracks windows and quits when the last one closes.
pub fn init(cx: &mut App) {
    cx.set_global(Windows::default());
    cx.set_global(window_frame::Memory {
        frame: window_frame::load(),
        generation: 0,
    });
    cx.on_window_closed(|cx, id| {
        let windows = cx.global_mut::<Windows>();
        windows.open.retain(|(handle, _)| handle.window_id() != id);
        for responder in windows.waiting.remove(&id).unwrap_or_default() {
            responder.closed();
        }
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    cx.on_action(|_: &NewWindow, cx| open(Request::default(), None, cx));
    cx.on_action(|_: &OpenFiles, cx| open_panel(cx));
    cx.on_action(|_: &crate::editor::Quit, cx| quit(cx));
}

/// Asks for files or folders to open, each in its window.
fn open_panel(cx: &mut App) {
    let chosen = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: true,
        multiple: true,
        prompt: Some("Open".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = chosen.await {
            cx.update(|cx| {
                for path in paths {
                    let request = Request {
                        path: Some(path),
                        ..Request::default()
                    };
                    open(request, None, cx);
                }
            });
        }
    })
    .detach();
}

/// Saves every document and quits, first asking when untitled documents
/// have unsaved text.
pub fn quit(cx: &mut App) {
    let mut unsaved = 0;
    for workspace in workspaces(cx) {
        let editor = workspace.read(cx).editor().clone();
        editor.update(cx, |editor, cx| {
            editor.save_before_quit(cx);
            unsaved += usize::from(editor.asks_before_closing());
        });
    }
    if unsaved == 0 {
        cx.quit();
        return;
    }
    cx.spawn(async move |cx| {
        if crate::mac::confirm_discard(unsaved) {
            cx.update(|cx| {
                for workspace in workspaces(cx) {
                    let editor = workspace.read(cx).editor().clone();
                    editor.update(cx, |editor, _| editor.discard_draft());
                }
                cx.quit();
            });
        }
    })
    .detach();
}

/// The open windows' workspaces.
pub fn workspaces(cx: &App) -> Vec<gpui_kit::Entity<Workspace>> {
    cx.global::<Windows>()
        .open
        .iter()
        .filter_map(|(_, workspace)| workspace.upgrade())
        .collect()
}

/// Opens the draft at `path`, whose text is `text`, as an unsaved untitled
/// document that keeps writing to that draft.
pub fn open_draft(path: PathBuf, text: String, cx: &mut App) {
    match open_window(None, Untitled::Text(Some(text)), cx) {
        Ok((_, workspace)) => {
            let editor = workspace.read(cx).editor().clone();
            editor.update(cx, |editor, _| editor.restore_draft(path));
        }
        Err(error) => eprintln!("focal: reopening a draft: {error:#}"),
    }
}

/// Opens what `request` asks for, or brings forward the window showing it.
pub fn open(request: Request, mut responder: Option<Responder>, cx: &mut App) {
    let Request {
        path,
        untitled,
        wait,
        welcome,
        fragment,
    } = request;
    let untitled = if welcome {
        Untitled::Welcome
    } else {
        Untitled::Text(untitled)
    };
    let shown = path.as_deref().and_then(|path| showing(path, cx));
    let opened = match shown {
        Some(opened) => Ok(opened),
        None => open_window(path.clone(), untitled, cx),
    };
    let (handle, workspace) = match opened {
        Ok(opened) => opened,
        Err(error) => {
            if let Some(responder) = responder {
                responder.failed(&format!("{error:#}"));
            }
            return;
        }
    };
    if let Some(fragment) = &fragment {
        workspace.update(cx, |workspace, cx| workspace.reveal(fragment, cx));
    }
    handle
        .update(cx, |_, window, _| window.activate_window())
        .ok();
    cx.activate(true);
    if let Some(responder) = &mut responder {
        responder.opened();
    }
    if let Some(path) = &path {
        crate::recent::note(path, cx);
    }
    if wait && let Some(responder) = responder {
        let windows = cx.global_mut::<Windows>();
        windows
            .waiting
            .entry(handle.window_id())
            .or_default()
            .push(responder);
    }
}

/// The open window whose file or folder is `path`, and its workspace.
fn showing(path: &Path, cx: &App) -> Option<(AnyWindowHandle, gpui_kit::Entity<Workspace>)> {
    cx.global::<Windows>()
        .open
        .iter()
        .find_map(|(handle, workspace)| {
            let workspace = workspace.upgrade()?;
            workspace
                .read(cx)
                .shows(path, cx)
                .then_some((*handle, workspace))
        })
}

/// What a window without a file starts with.
enum Untitled {
    /// Text that is in no file yet, such as piped standard input.
    Text(Option<String>),
    /// The Welcome document, which needs no saving until it is edited.
    Welcome,
}

fn open_window(
    path: Option<PathBuf>,
    untitled: Untitled,
    cx: &mut App,
) -> anyhow::Result<(AnyWindowHandle, gpui_kit::Entity<Workspace>)> {
    enum Content {
        Folder(PathBuf),
        File(Document, String),
    }
    let content = match path {
        Some(path) if path.is_dir() => Content::Folder(path),
        Some(path) => {
            let (document, text) = Document::open(path)?;
            Content::File(document, text)
        }
        None => match untitled {
            Untitled::Welcome => {
                Content::File(Document::untitled(), crate::welcome::TEXT.to_owned())
            }
            // Piped text is in no file yet: it is unsaved from the start.
            Untitled::Text(Some(text)) if !text.is_empty() => {
                Content::File(Document::unsaved(), text)
            }
            Untitled::Text(_) => Content::File(Document::untitled(), String::new()),
        },
    };
    let title = match &content {
        Content::Folder(root) => root.file_name().map_or_else(
            || root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
        Content::File(document, _) => document.title(),
    };
    // Each new window steps down and right from the first, as on macOS, and
    // every frame is fitted to the screen it opens on.
    let step = 24. * cx.global::<Windows>().open.len().min(10) as f32;
    let screens: Vec<Frame> = cx
        .displays()
        .iter()
        .map(|display| window_frame::from_bounds(display.visible_bounds()))
        .collect();
    let primary = cx.primary_display().map_or_else(
        || screens.first().copied().unwrap_or(FALLBACK_SCREEN),
        |display| window_frame::from_bounds(display.visible_bounds()),
    );
    let remembered = cx.global::<window_frame::Memory>().frame;
    let anchor = cx.global::<Windows>().anchor;
    let frame = window_frame::placement(remembered, anchor, step, &screens, primary);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(window_frame::to_bounds(frame))),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px(12.))),
        }),
        ..WindowOptions::default()
    };
    let (handle, workspace) = gpui_kit::open_window(options, cx, |window, cx| {
        let workspace = cx.new(|cx| {
            // A resize or move is remembered for the next window (and written
            // after a pause), so the window's size and place outlive it.
            cx.observe_window_bounds(window, |_, window, cx| {
                if let WindowBounds::Windowed(bounds) = window.window_bounds() {
                    window_frame::remember(window_frame::from_bounds(bounds), cx);
                }
            })
            .detach();
            match content {
                Content::Folder(root) => Workspace::new_folder(root, window, cx),
                Content::File(document, text) => Workspace::new(document, text, window, cx),
            }
        });
        Workspace::focus_editor(&workspace, window, cx);
        workspace
    })?;
    let windows = cx.global_mut::<Windows>();
    windows.anchor.get_or_insert((frame.x, frame.y));
    windows.open.push((handle, workspace.downgrade()));
    Ok((handle, workspace))
}

/// Used only when the platform reports no display at all.
const FALLBACK_SCREEN: Frame = Frame {
    x: 0.,
    y: 0.,
    width: 1440.,
    height: 900.,
};
