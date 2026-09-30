//! The quick switcher (Cmd-P in folder mode): type part of a file name,
//! choose with the arrow keys and open with Return.

use std::path::PathBuf;

use focal_core::fuzzy::fuzzy_rank;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    Subscription, TestSupportExt as _, Window, actions, div, px,
};

use crate::theme::{PROSE_FONT, Theme};

actions!(
    focal,
    [
        QuickOpen,
        SwitcherUp,
        SwitcherDown,
        SwitcherConfirm,
        SwitcherDismiss
    ]
);

const CONTEXT: &str = "FocalSwitcher > Input";
/// Results shown at once.
const SHOWN: usize = 12;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-p", QuickOpen, None),
        KeyBinding::new("up", SwitcherUp, Some(CONTEXT)),
        KeyBinding::new("down", SwitcherDown, Some(CONTEXT)),
        KeyBinding::new("enter", SwitcherConfirm, Some(CONTEXT)),
        KeyBinding::new("escape", SwitcherDismiss, Some(CONTEXT)),
    ]);
}

pub enum SwitcherEvent {
    Open(PathBuf),
    Dismiss,
}

pub struct Switcher {
    input: Entity<InputState>,
    root: PathBuf,
    /// Relative to `root`, the most recently changed first.
    files: Vec<PathBuf>,
    /// Indices into `files`, best match first.
    results: Vec<usize>,
    /// The query `results` were ranked for.
    query: String,
    selected: usize,
    _input: Subscription,
}

impl EventEmitter<SwitcherEvent> for Switcher {}

impl Switcher {
    pub fn new(
        root: PathBuf,
        files: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut files = files.to_vec();
        files.sort_by_cached_key(|file| {
            std::cmp::Reverse(
                std::fs::metadata(root.join(file))
                    .and_then(|m| m.modified())
                    .ok(),
            )
        });
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Open a file…"));
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            match event {
                InputEvent::Change => this.filter(cx),
                // A click elsewhere closes the switcher.
                InputEvent::Blur => cx.emit(SwitcherEvent::Dismiss),
                _ => {}
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            input,
            root,
            results: (0..files.len()).collect(),
            query: String::new(),
            files,
            selected: 0,
            _input: subscription,
        }
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_string();
        if query == self.query {
            return;
        }
        let names: Vec<String> = self
            .files
            .iter()
            .map(|file| file.to_string_lossy().into_owned())
            .collect();
        self.results = if query.trim().is_empty() {
            (0..self.files.len()).collect()
        } else {
            fuzzy_rank(query.trim(), names.iter().map(String::as_str))
                .into_iter()
                .map(|(ix, _)| ix)
                .collect()
        };
        self.query = query;
        self.selected = 0;
        cx.notify();
    }

    fn up(&mut self, _: &SwitcherUp, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.selected.saturating_sub(1);
        cx.notify();
    }

    fn down(&mut self, _: &SwitcherDown, _: &mut Window, cx: &mut Context<Self>) {
        let last = self.results.len().min(SHOWN).saturating_sub(1);
        self.selected = (self.selected + 1).min(last);
        cx.notify();
    }

    fn confirm(&mut self, _: &SwitcherConfirm, _: &mut Window, cx: &mut Context<Self>) {
        // The input reports changes late; rank what it holds now.
        self.filter(cx);
        if let Some(&file) = self.results.get(self.selected) {
            cx.emit(SwitcherEvent::Open(self.root.join(&self.files[file])));
        }
    }
}

impl Render for Switcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let rows = self
            .results
            .iter()
            .take(SHOWN)
            .enumerate()
            .map(|(ix, &file)| {
                let path = &self.files[file];
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let parent = path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let target = self.root.join(path);
                div()
                    .id(("switch-result", ix))
                    .test_support()
                    .aria_label(name.clone())
                    .px(px(14.))
                    .py(px(6.))
                    .flex()
                    .gap(px(10.))
                    .items_baseline()
                    .cursor_pointer()
                    .when(ix == self.selected, |d| d.bg(theme.selection))
                    .child(name)
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.marker)
                            .child(parent),
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(SwitcherEvent::Open(target.clone()));
                    }))
            });
        div()
            .id("switcher")
            .test_support()
            .key_context("FocalSwitcher")
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::confirm))
            .on_action(cx.listener(|_, _: &SwitcherDismiss, _, cx| {
                cx.emit(SwitcherEvent::Dismiss);
            }))
            .occlude()
            .w(px(520.))
            .max_w_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .border_1()
            .border_color(theme.rule)
            .rounded(px(10.))
            .shadow_lg()
            .overflow_hidden()
            .font_family(PROSE_FONT)
            .text_size(px(14.))
            .text_color(theme.text)
            .child(
                div()
                    .p(px(8.))
                    .border_b_1()
                    .border_color(theme.rule)
                    .child(Input::new(&self.input).appearance(false)),
            )
            .child(
                div()
                    .py(px(4.))
                    .children(rows)
                    .when(self.results.is_empty(), |d| {
                        d.child(
                            div()
                                .px(px(14.))
                                .py(px(6.))
                                .text_color(theme.marker)
                                .child("No matching files"),
                        )
                    }),
            )
    }
}
