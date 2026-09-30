//! The bottom formatting bar: the file name, formatting buttons and the word
//! count. Its buttons dispatch the same actions as the keyboard and menus.

use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    Action, App, ClickEvent, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::editor::{
    BarState, Bold, CONTEXT, InlineCode, InsertCodeBlock, InsertLink, InsertMath, InsertTable,
    Italic, SetHeading, Strikethrough, ToggleBullets, ToggleFocusMode, ToggleNumbers, ToggleQuote,
    ToggleTask,
};
use crate::theme::{MONO_FONT, Theme};

/// The bar's height, and the strip at the bottom edge that shows it.
pub const BAR_HEIGHT: f32 = 44.;
pub const SHOW_ZONE: f32 = 40.;

/// A bar button with a tooltip that names it and, for an action with a
/// shortcut, shows the keys.
fn button(
    id: &'static str,
    label: impl IntoElement,
    tooltip: &'static str,
    theme: &Theme,
    action: Option<Box<dyn Action>>,
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
                Some(action) => tip.action(action.as_ref(), Some(CONTEXT)),
                None => tip,
            }
            .build(window, cx)
        })
        .px(px(7.))
        .py(px(3.))
        .rounded(px(5.))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(label)
        .on_click(on_click)
}

fn action_button(
    id: &'static str,
    label: impl IntoElement,
    tooltip: &'static str,
    theme: &Theme,
    action: impl Action + Clone,
) -> impl IntoElement {
    let shortcut = action.boxed_clone();
    button(
        id,
        label,
        tooltip,
        theme,
        Some(shortcut),
        move |_, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx);
        },
    )
}

fn heading_menu(event: &ClickEvent, window: &mut Window, cx: &mut App) {
    let mut menu = NativeMenu::new().menu("Paragraph", Box::new(SetHeading(0)));
    for level in 1..=6u8 {
        menu = menu.menu(format!("Heading {level}"), Box::new(SetHeading(level)));
    }
    menu.show(event.position(), window, cx);
}

/// The formatting buttons, headed by the heading level.
fn buttons(heading: String, theme: &Theme) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap(px(2.))
        .text_color(theme.text)
        .font_family(MONO_FONT)
        .child(button(
            "bar-heading",
            heading,
            "Heading level",
            theme,
            None,
            heading_menu,
        ))
        .child(action_button(
            "bar-bold",
            div().font_weight(FontWeight::BOLD).child("B"),
            "Bold",
            theme,
            Bold,
        ))
        .child(action_button(
            "bar-italic",
            div().italic().child("I"),
            "Italic",
            theme,
            Italic,
        ))
        .child(action_button(
            "bar-strike",
            div().line_through().child("S"),
            "Strikethrough",
            theme,
            Strikethrough,
        ))
        .child(action_button(
            "bar-code",
            "`",
            "Inline code",
            theme,
            InlineCode,
        ))
        .child(action_button("bar-link", "[]", "Link", theme, InsertLink))
        .child(action_button(
            "bar-bullets",
            "•",
            "Bulleted list",
            theme,
            ToggleBullets,
        ))
        .child(action_button(
            "bar-numbers",
            "1.",
            "Numbered list",
            theme,
            ToggleNumbers,
        ))
        .child(action_button("bar-task", "☐", "Task", theme, ToggleTask))
        .child(action_button("bar-quote", ">", "Quote", theme, ToggleQuote))
        .child(action_button("bar-table", "⊞", "Table", theme, InsertTable))
        .child(action_button(
            "bar-block",
            "{}",
            "Code block",
            theme,
            InsertCodeBlock,
        ))
        .child(action_button("bar-math", "∑", "Math", theme, InsertMath))
        .child(action_button(
            "bar-focus",
            "◎",
            "Focus mode",
            theme,
            ToggleFocusMode,
        ))
}

pub fn render(state: &BarState, theme: &Theme) -> impl IntoElement {
    let heading = if state.heading == 0 {
        "¶".to_owned()
    } else {
        format!("H{}", state.heading)
    };
    let words = if state.selected_words > 0 {
        format!("{} of {} words", state.selected_words, state.words)
    } else {
        format!(
            "{} words · {} min",
            state.words,
            state.minutes.max(usize::from(state.words > 0))
        )
    };
    div()
        .id("bottom-bar")
        .test_support()
        .occlude()
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(BAR_HEIGHT))
        .px(px(16.))
        .flex()
        .items_center()
        .gap(px(2.))
        .bg(theme.background)
        .border_t_1()
        .border_color(theme.rule)
        .text_size(px(13.))
        .text_color(theme.marker)
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(state.file_name.clone()),
        )
        .child(buttons(heading, theme))
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .whitespace_nowrap()
                .child(words),
        )
}
