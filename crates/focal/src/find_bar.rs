//! The find bar (⌘F, or ⌥⌘F with replacing): a query field, the match count,
//! previous and next, and a replacement field. It drives the editor, which
//! keeps the matches and highlights them.

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, App, AppContext as _, ClickEvent, Context, Entity, EventEmitter,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, Window,
    actions, div, px,
};

use crate::editor::Editor;
use crate::theme::{PROSE_FONT, Theme};

actions!(
    focal,
    [Find, FindAndReplace, FindNext, FindPrevious, FindBarDismiss]
);

const CONTEXT: &str = "FocalFind";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-f", Find, None),
        // GPUI Kit's inputs take ⌘F for their own search; the find bar's
        // fields open the find bar instead.
        KeyBinding::new("cmd-f", Find, Some("FocalFind > Input")),
        KeyBinding::new("cmd-alt-f", FindAndReplace, None),
        KeyBinding::new("cmd-g", FindNext, None),
        KeyBinding::new("cmd-shift-g", FindPrevious, None),
        KeyBinding::new("escape", FindBarDismiss, Some("FocalFind > Input")),
    ]);
}

pub enum FindBarEvent {
    Dismiss,
}

pub struct FindBar {
    editor: Entity<Editor>,
    query: Entity<InputState>,
    replacement: Entity<InputState>,
    replacing: bool,
    editor_changes: Subscription,
    _inputs: [Subscription; 2],
}

impl EventEmitter<FindBarEvent> for FindBar {}

impl FindBar {
    /// A find bar searching `editor`, starting with `seed`.
    pub fn new(
        editor: Entity<Editor>,
        seed: Option<String>,
        replacing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
        let inputs = [
            cx.subscribe_in(
                &query,
                window,
                |this, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => this.search(cx),
                    InputEvent::PressEnter { shift, .. } => this.step(!shift, cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &replacement,
                window,
                |this, _, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.replace(cx);
                    }
                },
            ),
        ];
        let mut this = Self {
            editor_changes: cx.observe(&editor, |_, _, cx| cx.notify()),
            editor,
            query,
            replacement,
            replacing,
            _inputs: inputs,
        };
        if let Some(seed) = seed {
            this.query
                .update(cx, |input, cx| input.set_value(seed, window, cx));
        }
        this.show(replacing, window, cx);
        this.search(cx);
        this
    }

    /// Focuses the query, selected, and shows the replacement field too
    /// when `replacing`.
    pub fn show(&mut self, replacing: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.replacing |= replacing;
        self.query.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
    }

    /// Searches `editor` instead, as when folder mode opens another file.
    pub fn set_editor(&mut self, editor: Entity<Editor>, cx: &mut Context<Self>) {
        self.editor_changes = cx.observe(&editor, |_, _, cx| cx.notify());
        self.editor = editor;
        self.search(cx);
    }

    pub fn query(&self, cx: &App) -> String {
        self.query.read(cx).value().to_string()
    }

    fn search(&self, cx: &mut Context<Self>) {
        let query = Some(self.query(cx));
        self.editor
            .update(cx, |editor, cx| editor.set_query(query, cx));
    }

    /// Selects the next match, or the previous one.
    pub fn step(&self, forward: bool, cx: &mut Context<Self>) {
        // The input reports changes late; search for what it holds now.
        self.search(cx);
        self.editor
            .update(cx, |editor, cx| editor.find_step(forward, cx));
    }

    fn replace(&self, cx: &mut Context<Self>) {
        self.search(cx);
        let replacement = self.replacement.read(cx).value().to_string();
        self.editor
            .update(cx, |editor, cx| editor.replace_current(&replacement, cx));
    }

    fn replace_all(&self, cx: &mut Context<Self>) {
        self.search(cx);
        let replacement = self.replacement.read(cx).value().to_string();
        self.editor
            .update(cx, |editor, cx| editor.replace_all(&replacement, cx));
    }

    /// "3 of 12", "12 matches" or "No matches".
    fn status(&self, cx: &App) -> String {
        match self.editor.read(cx).find_status() {
            None => String::new(),
            Some((_, 0)) => "No matches".into(),
            Some((Some(ix), count)) => format!("{} of {count}", ix + 1),
            Some((None, 1)) => "1 match".into(),
            Some((None, count)) => format!("{count} matches"),
        }
    }
}

fn button(
    id: &'static str,
    label: &'static str,
    tooltip: &'static str,
    action: Option<Box<dyn Action>>,
    theme: &Theme,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let hover = theme.code_background;
    div()
        .id(id)
        .test_support()
        .aria_label(tooltip)
        .tooltip(move |window, cx| {
            let tip = Tooltip::new(tooltip);
            match &action {
                Some(action) => tip.action(action.as_ref(), None),
                None => tip,
            }
            .build(window, cx)
        })
        .flex_none()
        .px(px(7.))
        .py(px(3.))
        .rounded(px(5.))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(label)
        .on_click(on_click)
}

impl Render for FindBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let previous = cx.listener(|this, _: &ClickEvent, _, cx| this.step(false, cx));
        let next = cx.listener(|this, _: &ClickEvent, _, cx| this.step(true, cx));
        let replace = cx.listener(|this, _: &ClickEvent, _, cx| this.replace(cx));
        let replace_all = cx.listener(|this, _: &ClickEvent, _, cx| this.replace_all(cx));
        let field = |id: &'static str, input: &Entity<InputState>| {
            div()
                .id(id)
                .test_support()
                .flex_1()
                .min_w(px(0.))
                .child(Input::new(input).appearance(false))
        };
        div()
            .id("find-bar")
            .test_support()
            .key_context(CONTEXT)
            .on_action(cx.listener(|_, _: &FindBarDismiss, _, cx| {
                cx.emit(FindBarEvent::Dismiss);
            }))
            .occlude()
            .w(px(380.))
            .max_w_full()
            .p(px(6.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .bg(theme.background)
            .border_1()
            .border_color(theme.rule)
            .rounded(px(10.))
            .shadow_lg()
            .font_family(PROSE_FONT)
            .text_size(px(13.))
            .text_color(theme.text)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(field("find-field", &self.query))
                    .child(
                        div()
                            .id("find-status")
                            .test_support()
                            .flex_none()
                            .text_color(theme.marker)
                            .child(self.status(cx)),
                    )
                    .child(button(
                        "find-previous",
                        "‹",
                        "Previous match",
                        Some(Box::new(FindPrevious)),
                        &theme,
                        previous,
                    ))
                    .child(button(
                        "find-next",
                        "›",
                        "Next match",
                        Some(Box::new(FindNext)),
                        &theme,
                        next,
                    )),
            )
            .when(self.replacing, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .border_t_1()
                        .border_color(theme.rule)
                        .pt(px(4.))
                        .child(field("replace-field", &self.replacement))
                        .child(button(
                            "replace",
                            "Replace",
                            "Replace this match and find the next",
                            None,
                            &theme,
                            replace,
                        ))
                        .child(button(
                            "replace-all",
                            "All",
                            "Replace all matches",
                            None,
                            &theme,
                            replace_all,
                        )),
                )
            })
    }
}
