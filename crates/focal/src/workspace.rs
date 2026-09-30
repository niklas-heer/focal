//! The window's root: the editor, the bottom bar and, in folder mode, the
//! sidebar of files.

use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, App, AppContext as _, Context, Entity,
    Focusable as _, InteractiveElement as _, IntoElement, KeyBinding, KeyDownEvent, MouseMoveEvent,
    ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _, Task,
    TestSupportExt as _, Window, WindowControlArea, actions, div, px,
};

use crate::bar;
use crate::document::Document;
use crate::editor::Editor;
use crate::folder;
use crate::switcher::{QuickOpen, Switcher, SwitcherEvent};
use crate::theme::Theme;

actions!(focal, [ToggleSidebar]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("ctrl-cmd-s", ToggleSidebar, None)]);
}

const SIDEBAR_WIDTH: f32 = 240.;
/// How long the file list waits for a burst of changes to settle.
const RESCAN_SETTLE: Duration = Duration::from_millis(200);

/// Folder mode: the folder, its Markdown files and the sidebar listing them.
struct Folder {
    root: PathBuf,
    /// Relative to `root`.
    files: Vec<PathBuf>,
    sidebar: bool,
    _watch: Option<(notify::RecommendedWatcher, Task<()>)>,
}

/// How long the bar stays after the pointer leaves it.
const BAR_LINGER: Duration = Duration::from_secs(1);
const BAR_FADE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bar {
    Hidden,
    /// Shown for the n-th time, which names its fade-in animation.
    Shown(u32),
    Leaving(u32),
}

pub struct Workspace {
    editor: Entity<Editor>,
    bar: Bar,
    shows: u32,
    bar_timer: Option<Task<()>>,
    folder: Option<Folder>,
    switcher: Option<(Entity<Switcher>, gpui_kit::Subscription)>,
}

impl Workspace {
    pub fn new(
        document: Document,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| Editor::new(document, text, window, cx));
        Self {
            editor,
            bar: Bar::Hidden,
            shows: 0,
            bar_timer: None,
            folder: None,
            switcher: None,
        }
    }

    /// Opens `root` in folder mode, with its most recently changed Markdown
    /// file, or a new `Untitled.md` in it.
    pub fn new_folder(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = folder::scan(&root, folder::SCAN_LIMIT);
        let path = folder::newest(&root, &files)
            .map_or_else(|| root.join("Untitled.md"), |file| root.join(file));
        let (document, text) = Document::open(path).unwrap_or_else(|error| {
            eprintln!("focal: {error:#}");
            (Document::untitled(), String::new())
        });
        let mut this = Self::new(document, text, window, cx);
        let watch = match folder::watch(&root) {
            Ok((watcher, events)) => Some((watcher, Self::rescan_on(events, root.clone(), cx))),
            Err(error) => {
                eprintln!(
                    "focal: not watching {} for changes: {error:#}",
                    root.display()
                );
                None
            }
        };
        this.folder = Some(Folder {
            root,
            files,
            sidebar: false,
            _watch: watch,
        });
        this
    }

    /// Rescans the folder after each settled burst of changes.
    fn rescan_on(
        events: async_channel::Receiver<()>,
        root: PathBuf,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            while events.recv().await.is_ok() {
                cx.background_executor().timer(RESCAN_SETTLE).await;
                while events.try_recv().is_ok() {}
                let scan_root = root.clone();
                let files = cx
                    .background_executor()
                    .spawn(async move { folder::scan(&scan_root, folder::SCAN_LIMIT) })
                    .await;
                let updated = this.update(cx, |this, cx| {
                    if let Some(folder) = &mut this.folder {
                        folder.files = files;
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    break;
                }
            }
        })
    }

    /// Saves the current file, then opens `path` in its place.
    pub fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.read(cx).path() == Some(path.as_path()) {
            return;
        }
        self.editor
            .update(cx, |editor, cx| editor.flush(window, cx));
        let (document, text) = match Document::open(path) {
            Ok(opened) => opened,
            Err(error) => {
                eprintln!("focal: {error:#}");
                return;
            }
        };
        let title = document.title();
        self.editor = cx.new(|cx| Editor::new(document, text, window, cx));
        window.set_window_title(&title);
        let handle = self.editor.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        cx.notify();
    }

    fn quick_open(&mut self, _: &QuickOpen, window: &mut Window, cx: &mut Context<Self>) {
        let Some(folder) = &self.folder else { return };
        let (root, files) = (folder.root.clone(), folder.files.clone());
        let switcher = cx.new(|cx| Switcher::new(root, &files, window, cx));
        let subscription = cx.subscribe_in(&switcher, window, |this, _, event, window, cx| {
            this.switcher = None;
            if let SwitcherEvent::Open(path) = event {
                this.open_file(path.clone(), window, cx);
            }
            let handle = this.editor.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
            cx.notify();
        });
        self.switcher = Some((switcher, subscription));
        cx.notify();
    }

    fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(folder) = &mut self.folder {
            folder.sidebar = !folder.sidebar;
            cx.notify();
        }
    }

    fn render_sidebar(&self, theme: &Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let folder = self.folder.as_ref().filter(|f| f.sidebar)?;
        let current = self.editor.read(cx).path().map(PathBuf::from);
        let files = folder.files.iter().enumerate().map(|(ix, file)| {
            let path = folder.root.join(file);
            let selected = current.as_ref() == Some(&path);
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let parent = file
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let hover = theme.code_background;
            div()
                .id(("file", ix))
                .test_support()
                .px(px(16.))
                .py(px(5.))
                .cursor_pointer()
                .when(selected, |d| d.bg(theme.selection))
                .hover(move |style| style.bg(hover))
                .child(name)
                .when(!parent.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(11.))
                            .text_color(theme.marker)
                            .child(parent),
                    )
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_file(path.clone(), window, cx);
                }))
        });
        Some(
            div()
                .id("sidebar")
                .test_support()
                .flex_none()
                .w(px(SIDEBAR_WIDTH))
                .h_full()
                .flex()
                .flex_col()
                .bg(theme.code_background)
                .border_r_1()
                .border_color(theme.rule)
                .text_size(px(13.))
                .text_color(theme.text)
                // Room for the window buttons, and a handle to drag the window.
                .child(
                    div()
                        .h(px(40.))
                        .flex_none()
                        .window_control_area(WindowControlArea::Drag),
                )
                .child(
                    div()
                        .id("sidebar-files")
                        .flex_1()
                        .overflow_y_scroll()
                        .children(files),
                )
                .into_any_element(),
        )
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by tests until folder mode")
    )]
    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    /// Gives the editor keyboard focus.
    pub fn focus_editor(this: &Entity<Self>, window: &mut Window, cx: &mut gpui_kit::App) {
        let handle = this.read(cx).editor.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }

    fn hide_bar(&mut self, cx: &mut Context<Self>) {
        self.bar_timer = None;
        if self.bar != Bar::Hidden {
            self.bar = Bar::Hidden;
            cx.notify();
        }
    }

    fn pointer_moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from_bottom = window.viewport_size().height - event.position.y;
        let reach = if matches!(self.bar, Bar::Shown(_)) {
            bar::BAR_HEIGHT
        } else {
            bar::SHOW_ZONE
        };
        if from_bottom <= px(reach) {
            self.bar_timer = None;
            if !matches!(self.bar, Bar::Shown(_)) {
                self.shows += 1;
                self.bar = Bar::Shown(self.shows);
                cx.notify();
            }
        } else if matches!(self.bar, Bar::Shown(_)) && self.bar_timer.is_none() {
            self.bar_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(BAR_LINGER).await;
                this.update(cx, |this, cx| {
                    if let Bar::Shown(n) = this.bar {
                        this.bar = Bar::Leaving(n);
                        cx.notify();
                    }
                })
                .ok();
                cx.background_executor().timer(BAR_FADE).await;
                this.update(cx, |this, cx| this.hide_bar(cx)).ok();
            }));
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let editor = self.editor.read(cx);
        let focus_mode = editor.focus_mode();
        // The word count reads the whole text; only count while the bar shows.
        let bar = (!focus_mode && self.bar != Bar::Hidden).then(|| (self.bar, editor.bar_state()));
        let sidebar = if focus_mode {
            None
        } else {
            self.render_sidebar(&theme, cx)
        };
        let main = div()
            .relative()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .child(self.editor.clone())
            // Where the pointer shows the bar; tests hover it.
            .child(
                div()
                    .id("bar-zone")
                    .test_support()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(bar::SHOW_ZONE)),
            )
            .when_some(bar, |d, (state, bar_state)| match state {
                Bar::Hidden => d,
                Bar::Shown(n) => d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(bar::render(&bar_state, &theme))
                        .with_animation(("bar-in", n), Animation::new(BAR_FADE), |el, t| {
                            el.opacity(t)
                        }),
                ),
                Bar::Leaving(n) => d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(bar::render(&bar_state, &theme))
                        .with_animation(("bar-out", n), Animation::new(BAR_FADE), |el, t| {
                            el.opacity(1. - t)
                        }),
                ),
            });
        div()
            .id("workspace")
            .size_full()
            .flex()
            .bg(theme.background)
            .on_mouse_move(cx.listener(Self::pointer_moved))
            // The bar never shows while you type.
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, cx| this.hide_bar(cx)))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::quick_open))
            .children(sidebar)
            .child(main)
            .when_some(self.switcher.as_ref(), |d, (switcher, _)| {
                d.child(
                    div()
                        .absolute()
                        .top(px(72.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(switcher.clone()),
                )
            })
    }
}
