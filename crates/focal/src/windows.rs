//! Focal's windows: one per request, with the window that already shows a
//! file or folder brought forward instead of a second one, and waiting
//! `focal --wait` calls answered when their window closes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, DisplayId, Global, KeyBinding, PathPromptOptions,
    PlatformDisplay, TitlebarOptions, WeakEntity, WindowBounds, WindowId, WindowOptions, actions,
    point, px,
};

use crate::document::Document;
use crate::instance::{Request, Responder};
use crate::window_frame::{self, Frame, Place, Saved, Screen};
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
    /// Where the session's first window on the screen in use opened, and how
    /// many windows were open then; later windows there cascade from it.
    anchor: Option<(Place, usize)>,
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
    // A new window opens on the screen the user works on. Each new window
    // there steps down and right from the first, as on macOS, and every
    // frame is fitted to the screen it opens on.
    let (screens, active) = screens(cx);
    let anchor = cx.global::<Windows>().anchor;
    let open = cx.global::<Windows>().open.len();
    let step = 24.
        * open
            .saturating_sub(anchor.map_or(0, |(_, opened)| opened))
            .min(10) as f32;
    let remembered = cx
        .global::<window_frame::Memory>()
        .frame
        .as_ref()
        .map(|saved| (remembered_display(saved, cx), saved.frame));
    let place = window_frame::placement(
        remembered,
        anchor.map(|(place, _)| place),
        step,
        &screens,
        active,
    );
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(window_frame::to_bounds(place.frame))),
        display_id: Some(DisplayId::new(place.display)),
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
                    let display = window
                        .display(cx)
                        .and_then(|display| display.uuid().ok())
                        .map(|uuid| uuid.to_string());
                    let frame = window_frame::from_bounds(bounds);
                    window_frame::remember(Saved { frame, display }, cx);
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
    // A window on another screen than the anchor's starts a new cascade.
    if anchor.is_none_or(|(anchor, _)| anchor.display != place.display) {
        windows.anchor = Some((place, open));
    }
    windows.open.push((handle, workspace.downgrade()));
    Ok((handle, workspace))
}

/// The connected screens, and the one the user works on: the one with
/// Focal's key window, else the one under the pointer, else the primary one.
fn screens(cx: &App) -> (Vec<Screen>, Screen) {
    let screen = |display: &dyn PlatformDisplay| Screen {
        display: display.id().into(),
        area: window_frame::from_bounds(display.visible_bounds()),
    };
    let screens: Vec<Screen> = cx
        .displays()
        .iter()
        .map(|display| screen(display.as_ref()))
        .collect();
    let primary = cx.primary_display().map_or_else(
        || {
            screens.first().copied().unwrap_or(Screen {
                display: 0,
                area: FALLBACK_SCREEN,
            })
        },
        |display| screen(display.as_ref()),
    );
    let active = window_frame::active_screen(
        &screens,
        primary,
        crate::mac::key_window_display(),
        crate::mac::pointer_display(),
    );
    (screens, active)
}

/// The display a window without a place of its own (Settings, About) opens
/// on: the one the user works on.
pub fn active_display(cx: &App) -> Option<DisplayId> {
    let (screens, active) = screens(cx);
    (!screens.is_empty()).then(|| DisplayId::new(active.display))
}

/// The id of the connected display a saved frame was on, or `None` when it
/// is no longer connected. A frame saved without its display was reopened on
/// the primary one, so it stays there.
fn remembered_display(saved: &Saved, cx: &App) -> Option<u64> {
    match &saved.display {
        None => cx.primary_display().map(|display| display.id().into()),
        Some(uuid) => cx
            .displays()
            .iter()
            .find(|display| {
                display
                    .uuid()
                    .is_ok_and(|candidate| candidate.to_string().eq_ignore_ascii_case(uuid))
            })
            .map(|display| display.id().into()),
    }
}

/// Used only when the platform reports no display at all.
const FALLBACK_SCREEN: Frame = Frame {
    x: 0.,
    y: 0.,
    width: 1440.,
    height: 900.,
};
