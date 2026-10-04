//! The key reference: every key of the editing mode in use, grouped, over
//! the page. Help ▸ Editing Keys, `space ?` in Helix or `\?` in Vim.

use gpui_kit::{
    AnyElement, App, Context, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, actions, div, px,
};

use gpui_kit::Focusable as _;

use super::Workspace;
use crate::icons::Icon;
use crate::theme::{MONO_FONT, Theme};

actions!(focal, [ShowEditingKeys, CloseEditingKeys]);

const CONTEXT: &str = "FocalKeys";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", CloseEditingKeys, Some(CONTEXT)),
        KeyBinding::new("cmd-w", CloseEditingKeys, Some(CONTEXT)),
    ]);
}

/// Focal's own shortcuts, for the standard keys.
const STANDARD: &[focal_core::keys::Group] = &[
    (
        "Writing",
        &[
            ("⌘B ⌘I", "bold, italic"),
            ("⇧⌘X ⇧⌘H", "strikethrough, highlight"),
            ("⌘E ⌘K", "inline code, link"),
            ("⌘1 … ⌘6", "heading level; ⌘0 paragraph"),
            ("Tab ⇧Tab", "nest, unnest a list item"),
        ],
    ),
    (
        "Moving",
        &[
            ("⌥← ⌥→", "by words"),
            ("⌘← ⌘→", "line start, end"),
            ("⌘↑ ⌘↓", "document start, end"),
            ("⇧⌘O", "go to a heading"),
            ("⌘↩ ⌘[", "follow a link, come back"),
        ],
    ),
    (
        "Finding",
        &[
            ("⌘F ⌥⌘F", "find, find and replace"),
            ("⌘G ⇧⌘G", "next, previous match"),
            ("⌘P", "open a file"),
        ],
    ),
    (
        "Viewing",
        &[
            ("⌘D", "focus mode"),
            ("⌥⌘I ⌥⌘O", "info panel, outline"),
            ("⌃⌘S", "sidebar"),
            ("⇧⌘L", "light or dark"),
            ("⌘= ⌘−", "bigger, smaller text"),
        ],
    ),
    (
        "Files",
        &[
            ("⌘N ⌘O", "new, open"),
            ("⇧⌘E ⌥⌘P", "export as HTML, print"),
            ("⌥⌘R", "show in Finder"),
            ("⌘,", "settings"),
        ],
    ),
];

impl Workspace {
    pub(super) fn show_editing_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        self.key_reference = Some(focus);
        cx.notify();
    }

    pub(super) fn close_editing_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.key_reference = None;
        let handle = self.editor.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
        cx.notify();
    }

    /// The reference over the page, while it is open.
    pub(super) fn render_key_reference(
        &self,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let focus: &FocusHandle = self.key_reference.as_ref()?;
        let (title, groups) = self
            .editor
            .read(cx)
            .key_reference()
            .unwrap_or_else(|| ("Keyboard shortcuts", STANDARD.to_vec()));
        Some(
            div()
                .id("key-reference")
                .test_support()
                .key_context(CONTEXT)
                .track_focus(focus)
                .on_action(cx.listener(|this, _: &CloseEditingKeys, window, cx| {
                    this.close_editing_keys(window, cx);
                }))
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.background.opacity(0.6))
                .on_click(cx.listener(|this, _, window, cx| this.close_editing_keys(window, cx)))
                .child(
                    div()
                        .id("key-reference-card")
                        .max_w(px(700.))
                        .max_h(gpui_kit::relative(0.85))
                        .overflow_y_scroll()
                        .p(px(22.))
                        .rounded(px(14.))
                        .bg(theme.surface)
                        .border_1()
                        .border_color(theme.rule)
                        .shadow_lg()
                        .text_size(px(13.))
                        // Clicks inside the card keep it open.
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .pb(px(14.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(17.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text)
                                        .child(title),
                                )
                                .child(
                                    div()
                                        .id("key-reference-close")
                                        .test_support()
                                        .aria_label("Close")
                                        .cursor_pointer()
                                        .child(Icon::X.element(16.).text_color(theme.marker))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.close_editing_keys(window, cx);
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap_x(px(28.))
                                .gap_y(px(20.))
                                .children(
                                    groups
                                        .into_iter()
                                        .map(|group| reference_group(group, theme)),
                                ),
                        ),
                )
                .into_any_element(),
        )
    }
}

/// A titled group of keys in the reference.
fn reference_group((name, hints): focal_core::keys::Group, theme: &Theme) -> impl IntoElement {
    div()
        .w(px(300.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .pb(px(2.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.marker)
                .child(name),
        )
        .children(hints.iter().map(|(keys, does)| {
            div()
                .flex()
                .gap(px(12.))
                .child(
                    div()
                        .w(px(96.))
                        .flex_none()
                        .font_family(MONO_FONT)
                        .text_size(px(12.))
                        .text_color(theme.caret)
                        .child(*keys),
                )
                .child(div().text_color(theme.text).child(*does))
        }))
}
