//! A picker: type part of a name, choose with the arrow keys, confirm with
//! Return. It serves the quick switcher (⌘P in folder mode) and Go to
//! Heading (⇧⌘O).

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
        GoToHeading,
        SwitcherUp,
        SwitcherDown,
        SwitcherConfirm,
        SwitcherDismiss
    ]
);

const CONTEXT: &str = "FocalSwitcher > Input";
/// Results shown at once; the arrow keys move this window over the rest.
const SHOWN: usize = 12;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-p", QuickOpen, None),
        KeyBinding::new("cmd-shift-o", GoToHeading, None),
        KeyBinding::new("up", SwitcherUp, Some(CONTEXT)),
        KeyBinding::new("down", SwitcherDown, Some(CONTEXT)),
        KeyBinding::new("enter", SwitcherConfirm, Some(CONTEXT)),
        KeyBinding::new("escape", SwitcherDismiss, Some(CONTEXT)),
    ]);
}

/// One choice: what it is called, a quieter detail beside it, the text a
/// query is matched against, and how far it is indented.
pub struct PickItem {
    pub label: String,
    pub detail: String,
    pub key: String,
    pub indent: u8,
}

pub enum SwitcherEvent {
    /// The item with this index was chosen.
    Pick(usize),
    Dismiss,
}

pub struct Switcher {
    input: Entity<InputState>,
    items: Vec<PickItem>,
    /// Indices into `items`, best match first.
    results: Vec<usize>,
    /// The query `results` were ranked for.
    query: String,
    selected: usize,
    /// Shown when nothing matches.
    empty: &'static str,
    _input: Subscription,
}

impl EventEmitter<SwitcherEvent> for Switcher {}

impl Switcher {
    /// A picker over `items`, in the order given until a query ranks them.
    pub fn new(
        items: Vec<PickItem>,
        placeholder: &'static str,
        empty: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            match event {
                InputEvent::Change => this.filter(cx),
                // A click elsewhere closes the picker.
                InputEvent::Blur => cx.emit(SwitcherEvent::Dismiss),
                _ => {}
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        Self {
            input,
            results: (0..items.len()).collect(),
            items,
            query: String::new(),
            selected: 0,
            empty,
            _input: subscription,
        }
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_string();
        if query == self.query {
            return;
        }
        self.results = if query.trim().is_empty() {
            (0..self.items.len()).collect()
        } else {
            fuzzy_rank(
                query.trim(),
                self.items.iter().map(|item| item.key.as_str()),
            )
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
        let last = self.results.len().saturating_sub(1);
        self.selected = (self.selected + 1).min(last);
        cx.notify();
    }

    fn confirm(&mut self, _: &SwitcherConfirm, _: &mut Window, cx: &mut Context<Self>) {
        // The input reports changes late; rank what it holds now.
        self.filter(cx);
        if let Some(&item) = self.results.get(self.selected) {
            cx.emit(SwitcherEvent::Pick(item));
        }
    }
}

impl Render for Switcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        // The shown results slide along with the selection.
        let first = self.selected.saturating_sub(SHOWN - 1);
        let rows = self
            .results
            .iter()
            .enumerate()
            .skip(first)
            .take(SHOWN)
            .map(|(ix, &item)| {
                let PickItem {
                    label,
                    detail,
                    indent,
                    ..
                } = &self.items[item];
                div()
                    .id(("switch-result", ix))
                    .test_support()
                    .aria_label(label.clone())
                    .pl(px(14. + f32::from(*indent) * 16.))
                    .pr(px(14.))
                    .py(px(6.))
                    .flex()
                    .gap(px(10.))
                    .items_baseline()
                    .cursor_pointer()
                    .when(ix == self.selected, |d| d.bg(theme.selection))
                    .child(label.clone())
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.marker)
                            .child(detail.clone()),
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(SwitcherEvent::Pick(item));
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
                                .child(self.empty),
                        )
                    }),
            )
    }
}
