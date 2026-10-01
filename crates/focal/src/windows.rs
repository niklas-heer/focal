//! Focal's windows: one per request, with the window that already shows a
//! file or folder brought forward instead of a second one, and waiting
//! `focal --wait` calls answered when their window closes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Global, KeyBinding, PathPromptOptions,
    TitlebarOptions, WeakEntity, WindowBounds, WindowId, WindowOptions, actions, point, px, size,
};

use crate::document::Document;
use crate::instance::{Request, Responder};
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
}

impl Global for Windows {}

/// Tracks windows and quits when the last one closes.
pub fn init(cx: &mut App) {
    cx.set_global(Windows::default());
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
    match open_window(None, Some(text), cx) {
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
    } = request;
    let shown = path.as_deref().and_then(|path| showing(path, cx));
    let handle = match shown {
        Some(handle) => Ok(handle),
        None => open_window(path.clone(), untitled, cx).map(|(handle, _)| handle),
    };
    let handle = match handle {
        Ok(handle) => handle,
        Err(error) => {
            if let Some(responder) = responder {
                responder.failed(&format!("{error:#}"));
            }
            return;
        }
    };
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

/// The open window whose file or folder is `path`.
fn showing(path: &Path, cx: &App) -> Option<AnyWindowHandle> {
    cx.global::<Windows>()
        .open
        .iter()
        .find(|(_, workspace)| {
            workspace
                .upgrade()
                .is_some_and(|workspace| workspace.read(cx).shows(path, cx))
        })
        .map(|(handle, _)| *handle)
}

fn open_window(
    path: Option<PathBuf>,
    untitled: Option<String>,
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
        None => Content::File(Document::untitled(), untitled.unwrap_or_default()),
    };
    let title = match &content {
        Content::Folder(root) => root.file_name().map_or_else(
            || root.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ),
        Content::File(document, _) => document.title(),
    };
    // Each new window steps down and right from the last, as on macOS.
    let step = px(24.) * cx.global::<Windows>().open.len().min(10) as f32;
    let mut bounds = Bounds::centered(None, size(px(860.), px(920.)), cx);
    bounds.origin += point(step, step);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px(12.))),
        }),
        ..WindowOptions::default()
    };
    let (handle, workspace) = gpui_kit::open_window(options, cx, |window, cx| {
        let workspace = cx.new(|cx| match content {
            Content::Folder(root) => Workspace::new_folder(root, window, cx),
            Content::File(document, text) => Workspace::new(document, text, window, cx),
        });
        Workspace::focus_editor(&workspace, window, cx);
        workspace
    })?;
    cx.global_mut::<Windows>()
        .open
        .push((handle, workspace.downgrade()));
    Ok((handle, workspace))
}
