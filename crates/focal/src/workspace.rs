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
use crate::editor::{Editor, EditorEvent};
use crate::find_bar::{Find, FindAndReplace, FindBar, FindBarEvent, FindNext, FindPrevious};
use crate::folder;
use crate::instance::Request;
use crate::switcher::{QuickOpen, Switcher, SwitcherEvent};
use crate::theme::Theme;

actions!(focal, [ToggleSidebar, GoBack]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-cmd-s", ToggleSidebar, None),
        KeyBinding::new("cmd-[", GoBack, None),
    ]);
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
    /// Kept alive to receive the editor's events.
    editor_events: gpui_kit::Subscription,
    /// Where followed links and jumps came from, for "back": the file and
    /// the selection in it.
    history: Vec<(Option<PathBuf>, std::ops::Range<usize>)>,
    find_bar: Option<(Entity<FindBar>, gpui_kit::Subscription)>,
    /// The find bar's query when it closed, for ⌘G.
    last_query: Option<String>,
}

impl Workspace {
    pub fn new(
        document: Document,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| Editor::new(document, text, window, cx));
        crate::settings::follow_appearance(window, cx);
        let events = cx.subscribe_in(&editor, window, Self::editor_event);
        Self {
            editor,
            bar: Bar::Hidden,
            shows: 0,
            bar_timer: None,
            folder: None,
            switcher: None,
            editor_events: events,
            history: Vec::new(),
            find_bar: None,
            last_query: None,
        }
    }

    fn editor_event(
        &mut self,
        _: &Entity<Editor>,
        event: &EditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.editor.read(cx);
        let here = editor.path().map(PathBuf::from);
        match event {
            EditorEvent::Jumped(from) => self.history.push((here, from.clone())),
            EditorEvent::Open(path) => {
                let in_folder = self
                    .folder
                    .as_ref()
                    .is_some_and(|folder| path.starts_with(&folder.root));
                if in_folder {
                    self.history.push((here, editor.selection.clone()));
                    self.open_file(path.clone(), window, cx);
                } else {
                    // The window list reads every workspace, this one too.
                    let request = Request {
                        path: Some(path.clone()),
                        ..Request::default()
                    };
                    cx.defer(move |cx| crate::windows::open(request, None, cx));
                }
            }
        }
    }

    /// Returns to where the last followed link or jump came from.
    fn go_back(&mut self, _: &GoBack, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, selection)) = self.history.pop() else {
            return;
        };
        if let Some(path) = path
            && self.editor.read(cx).path() != Some(path.as_path())
        {
            self.open_file(path, window, cx);
        }
        self.editor
            .update(cx, |editor, cx| editor.select(selection, cx));
    }

    /// Tells the editor which files its wiki links can reach.
    fn share_files(&self, cx: &mut Context<Self>) {
        if let Some(folder) = &self.folder {
            let (root, files) = (folder.root.clone(), folder.files.clone().into());
            self.editor
                .update(cx, |editor, cx| editor.set_link_files(root, files, cx));
        }
    }

    /// Opens `root` in folder mode, with its most recently changed Markdown
    /// file, or a new `Untitled.md` in it.
    pub fn new_folder(root: PathBuf, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = folder::scan(&root, folder::SCAN_LIMIT);
        let untitled = root.join("Untitled.md");
        let newest = folder::newest(&root, &files).map(|file| root.join(file));
        // A file Focal cannot read (not UTF-8) gives way to a new one, with
        // the reason shown.
        let (opened, failure) = match newest.map(Document::open) {
            Some(Ok(opened)) => (Some(opened), None),
            Some(Err(error)) => (None, Some(format!("{error:#}"))),
            None => (None, None),
        };
        let (document, text) = opened
            .or_else(|| Document::open(untitled).ok())
            .unwrap_or_else(|| (Document::untitled(), String::new()));
        window.set_window_title(&document.title());
        let mut this = Self::new(document, text, window, cx);
        if let Some(failure) = failure {
            this.editor
                .update(cx, |editor, cx| editor.show_error(failure, cx));
        }
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
        this.share_files(cx);
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
                        this.share_files(cx);
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
        self.editor_events = cx.subscribe_in(&self.editor, window, Self::editor_event);
        self.share_files(cx);
        if let Some((bar, _)) = &self.find_bar {
            let editor = self.editor.clone();
            bar.update(cx, |bar, cx| bar.set_editor(editor, cx));
        }
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

    /// Opens the find bar, or focuses it, seeded with the selected text or
    /// the last query.
    fn open_find_bar(&mut self, replacing: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((bar, _)) = &self.find_bar {
            bar.update(cx, |bar, cx| bar.show(replacing, window, cx));
            return;
        }
        let seed = self
            .editor
            .read(cx)
            .selected_line_text()
            .or_else(|| self.last_query.clone());
        let editor = self.editor.clone();
        let bar = cx.new(|cx| FindBar::new(editor, seed, replacing, window, cx));
        let subscription = cx.subscribe_in(&bar, window, |this, bar, event, window, cx| {
            let FindBarEvent::Dismiss = event;
            this.last_query = Some(bar.read(cx).query(cx)).filter(|q| !q.is_empty());
            this.find_bar = None;
            this.editor
                .update(cx, |editor, cx| editor.set_query(None, cx));
            let handle = this.editor.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
            cx.notify();
        });
        self.find_bar = Some((bar, subscription));
        cx.notify();
    }

    fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find_bar(false, window, cx);
    }

    fn find_and_replace(
        &mut self,
        _: &FindAndReplace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_find_bar(true, window, cx);
    }

    /// The next or previous match, with or without the find bar.
    fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some((bar, _)) = &self.find_bar {
            bar.update(cx, |bar, cx| bar.step(forward, cx));
        } else if let Some(query) = self.last_query.clone() {
            self.editor
                .update(cx, |editor, cx| editor.find_again(query, forward, cx));
        }
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

    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    /// Whether this window shows `path`, as its file or its folder.
    pub fn shows(&self, path: &std::path::Path, cx: &gpui_kit::App) -> bool {
        self.editor.read(cx).path() == Some(path)
            || self
                .folder
                .as_ref()
                .is_some_and(|folder| folder.root == path)
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
            .on_action(cx.listener(Self::go_back))
            .on_action(cx.listener(Self::quick_open))
            .on_action(cx.listener(Self::find))
            .on_action(cx.listener(Self::find_and_replace))
            .on_action(cx.listener(|this, _: &FindNext, _, cx| this.find_step(true, cx)))
            .on_action(cx.listener(|this, _: &FindPrevious, _, cx| this.find_step(false, cx)))
            .children(sidebar)
            .child(main)
            .when_some(self.find_bar.as_ref(), |d, (bar, _)| {
                d.child(
                    div()
                        .absolute()
                        .top(px(40.))
                        .right(px(16.))
                        .max_w(px(380.))
                        .child(bar.clone()),
                )
            })
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
