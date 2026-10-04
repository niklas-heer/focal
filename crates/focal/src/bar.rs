//! The bottom formatting bar: a floating strip of formatting buttons and the
//! word count. Its buttons dispatch the same actions as the keyboard and menus.

use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    Action, App, ClickEvent, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::editor::{
    BarState, Bold, CONTEXT, Highlight, InlineCode, InsertCodeBlock, InsertLink, InsertMath,
    InsertTable, Italic, SetHeading, Strikethrough, ToggleBullets, ToggleFocusMode, ToggleNumbers,
    ToggleQuote, ToggleTask,
};
use crate::icons::Icon;
use crate::theme::{MONO_FONT, Theme};

/// The floating bar's height and its distance from the window's bottom edge,
/// and the strip at the bottom edge that shows it.
pub const BAR_HEIGHT: f32 = 40.;
pub const BAR_MARGIN: f32 = 12.;
pub const SHOW_ZONE: f32 = 40.;
/// How far from the bottom the pointer keeps a shown bar.
pub const BAR_REACH: f32 = BAR_HEIGHT + 2. * BAR_MARGIN;

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
        .h(px(30.))
        .min_w(px(30.))
        .px(px(7.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(2.))
        .rounded(px(8.))
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(label)
        .on_click(on_click)
}

fn action_button(
    id: &'static str,
    icon: Icon,
    tooltip: &'static str,
    theme: &Theme,
    action: Box<dyn Action>,
) -> impl IntoElement {
    let shortcut = action.boxed_clone();
    button(
        id,
        icon.element(16.).text_color(theme.text),
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

fn separator(theme: &Theme) -> impl IntoElement {
    div().w(px(1.)).h(px(18.)).mx(px(4.)).bg(theme.rule)
}

/// A button's element id, icon, name and action.
type ButtonSpec = (&'static str, Icon, &'static str, Box<dyn Action>);

/// The buttons after the heading level, in groups: inline styles, blocks,
/// inserts and focus mode.
fn groups() -> [Vec<ButtonSpec>; 4] {
    [
        vec![
            ("bar-bold", Icon::Bold, "Bold", Box::new(Bold)),
            ("bar-italic", Icon::Italic, "Italic", Box::new(Italic)),
            (
                "bar-strike",
                Icon::Strikethrough,
                "Strikethrough",
                Box::new(Strikethrough),
            ),
            (
                "bar-highlight",
                Icon::Highlighter,
                "Highlight",
                Box::new(Highlight),
            ),
            ("bar-code", Icon::Code, "Inline code", Box::new(InlineCode)),
            ("bar-link", Icon::Link, "Link", Box::new(InsertLink)),
        ],
        vec![
            (
                "bar-bullets",
                Icon::List,
                "Bulleted list",
                Box::new(ToggleBullets),
            ),
            (
                "bar-numbers",
                Icon::ListOrdered,
                "Numbered list",
                Box::new(ToggleNumbers),
            ),
            ("bar-task", Icon::ListChecks, "Task", Box::new(ToggleTask)),
            ("bar-quote", Icon::TextQuote, "Quote", Box::new(ToggleQuote)),
        ],
        vec![
            ("bar-table", Icon::Table, "Table", Box::new(InsertTable)),
            (
                "bar-block",
                Icon::SquareCode,
                "Code block",
                Box::new(InsertCodeBlock),
            ),
            ("bar-math", Icon::Sigma, "Math", Box::new(InsertMath)),
        ],
        vec![(
            "bar-focus",
            Icon::Focus,
            "Focus mode",
            Box::new(ToggleFocusMode),
        )],
    ]
}

/// The formatting buttons, headed by the heading level.
fn buttons(heading: String, theme: &Theme) -> impl IntoElement {
    let mut row = div()
        .flex()
        .items_center()
        .gap(px(1.))
        .text_color(theme.text)
        .child(button(
            "bar-heading",
            div()
                .flex()
                .items_center()
                .gap(px(2.))
                .font_family(MONO_FONT)
                .text_size(px(12.))
                .child(heading)
                .child(Icon::ChevronDown.element(12.).text_color(theme.marker)),
            "Heading level",
            theme,
            None,
            heading_menu,
        ));
    for group in groups() {
        row = row.child(separator(theme));
        for (id, icon, tooltip, action) in group {
            row = row.child(action_button(id, icon, tooltip, theme, action));
        }
    }
    row
}

/// The bar: a floating strip of formatting buttons centered at the bottom,
/// with the word count at its end.
pub fn render(state: &BarState, goal: Option<usize>, theme: &Theme) -> impl IntoElement {
    let heading = if state.heading == 0 {
        "¶".to_owned()
    } else {
        format!("H{}", state.heading)
    };
    let mut words = if state.selected_words > 0 {
        format!("{} of {} words", state.selected_words, state.words)
    } else if let Some(goal) = goal {
        // A word goal: how far along, and a check once it is reached.
        let reached = if state.words >= goal { "✓ " } else { "" };
        format!("{reached}{} / {goal} words", state.words)
    } else {
        format!(
            "{} words · {} min",
            state.words,
            state.minutes.max(usize::from(state.words > 0))
        )
    };
    // A file that is not plain UTF-8 says how it is stored.
    if let Some(storage) = &state.storage {
        words = format!("{storage} · {words}");
    }
    div()
        .absolute()
        .left_0()
        .right_0()
        .bottom(px(BAR_MARGIN))
        .flex()
        .justify_center()
        .child(
            div()
                .id("bottom-bar")
                .test_support()
                .occlude()
                .h(px(BAR_HEIGHT))
                .px(px(6.))
                .flex()
                .items_center()
                .rounded(px(12.))
                .bg(theme.surface)
                .border_1()
                .border_color(theme.rule)
                .shadow_lg()
                .text_size(px(12.))
                .text_color(theme.marker)
                .child(buttons(heading, theme))
                .child(separator(theme))
                .child(
                    div()
                        .id("bar-words")
                        .test_support()
                        .px(px(6.))
                        .whitespace_nowrap()
                        .child(words),
                ),
        )
}
