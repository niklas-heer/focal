//! The bottom formatting bar: a floating strip of formatting buttons and the
//! word count. Its buttons dispatch the same actions as the keyboard and menus.

use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::{
    Action, App, ClickEvent, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, Window, div, px,
};

use crate::editor::{
    BarState, Bold, CONTEXT, Highlight, InlineCode, InsertCodeBlock, InsertLink, InsertMath,
    InsertTable, Italic, SetHeading, Strikethrough, ToggleBullets, ToggleFocusMode, ToggleNumbers,
    ToggleQuote, ToggleTask,
};
use crate::icons::Icon;
use crate::theme::{MONO_FONT, Theme};
use focal_core::analysis::InlineStyle;
use gpui_kit::prelude::FluentBuilder as _;

/// The floating bar's height and its distance from the window's bottom edge,
/// and the strip at the bottom edge that shows it.
pub const BAR_HEIGHT: f32 = 40.;
pub const BAR_MARGIN: f32 = 12.;
pub const SHOW_ZONE: f32 = 40.;
/// How far from the bottom the pointer keeps a shown bar.
pub const BAR_REACH: f32 = BAR_HEIGHT + 2. * BAR_MARGIN;

/// A bar button with a tooltip that names it and, for an action with a
/// shortcut, shows the keys.
pub(crate) fn button(
    id: &'static str,
    label: impl IntoElement,
    tooltip: &'static str,
    theme: &Theme,
    action: Option<Box<dyn Action>>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl gpui_kit::StatefulInteractiveElement + Styled + IntoElement {
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
    active: Option<bool>,
) -> impl IntoElement {
    let shortcut = action.boxed_clone();
    let fill = theme.code_background;
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
    .when_some(active, |d, active| {
        d.aria_toggled(if active {
            gpui_kit::accesskit::Toggled::True
        } else {
            gpui_kit::accesskit::Toggled::False
        })
    })
    .when(active == Some(true), move |d| d.bg(fill))
}

fn heading_menu(event: &ClickEvent, window: &mut Window, cx: &mut App) {
    let mut menu = NativeMenu::new().menu("Paragraph", Box::new(SetHeading(0)));
    for level in 1..=6u8 {
        menu = menu.menu(format!("Heading {level}"), Box::new(SetHeading(level)));
    }
    menu.show(event.position(), window, cx);
}

pub(crate) fn separator(theme: &Theme) -> impl IntoElement {
    div().w(px(1.)).h(px(18.)).mx(px(4.)).bg(theme.rule)
}

/// A button's element id, icon, name and action, and for a button that
/// toggles, whether what it applies is in effect at the caret.
type ButtonSpec = (
    &'static str,
    Icon,
    &'static str,
    Box<dyn Action>,
    fn(&Active) -> Option<bool>,
);

/// What is in effect at the caret, which the buttons show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Active {
    pub styles: InlineStyle,
    /// The kind of list item the caret is in, if any.
    pub list: Option<ListKind>,
    pub quote: bool,
    pub focus: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Bullets,
    Numbers,
    Task,
}

/// The buttons after the heading level, in groups: inline styles, blocks,
/// inserts and focus mode.
fn groups() -> [Vec<ButtonSpec>; 4] {
    [
        vec![
            ("bar-bold", Icon::Bold, "Bold", Box::new(Bold), |a| {
                Some(a.styles.contains(InlineStyle::STRONG))
            }),
            (
                "bar-italic",
                Icon::Italic,
                "Italic",
                Box::new(Italic),
                |a| Some(a.styles.contains(InlineStyle::EMPHASIS)),
            ),
            (
                "bar-strike",
                Icon::Strikethrough,
                "Strikethrough",
                Box::new(Strikethrough),
                |a| Some(a.styles.contains(InlineStyle::STRIKETHROUGH)),
            ),
            (
                "bar-highlight",
                Icon::Highlighter,
                "Highlight",
                Box::new(Highlight),
                |a| Some(a.styles.contains(InlineStyle::HIGHLIGHT)),
            ),
            (
                "bar-code",
                Icon::Code,
                "Inline code",
                Box::new(InlineCode),
                |a| Some(a.styles.contains(InlineStyle::CODE)),
            ),
            ("bar-link", Icon::Link, "Link", Box::new(InsertLink), |a| {
                Some(a.styles.contains(InlineStyle::LINK))
            }),
        ],
        vec![
            (
                "bar-bullets",
                Icon::List,
                "Bulleted list",
                Box::new(ToggleBullets),
                |a| Some(a.list == Some(ListKind::Bullets)),
            ),
            (
                "bar-numbers",
                Icon::ListOrdered,
                "Numbered list",
                Box::new(ToggleNumbers),
                |a| Some(a.list == Some(ListKind::Numbers)),
            ),
            (
                "bar-task",
                Icon::ListChecks,
                "Task",
                Box::new(ToggleTask),
                |a| Some(a.list == Some(ListKind::Task)),
            ),
            (
                "bar-quote",
                Icon::TextQuote,
                "Quote",
                Box::new(ToggleQuote),
                |a| Some(a.quote),
            ),
        ],
        vec![
            (
                "bar-table",
                Icon::Table,
                "Table",
                Box::new(InsertTable),
                |_| None,
            ),
            (
                "bar-block",
                Icon::SquareCode,
                "Code block",
                Box::new(InsertCodeBlock),
                |_| None,
            ),
            (
                "bar-math",
                Icon::Sigma,
                "Math",
                Box::new(InsertMath),
                |_| None,
            ),
        ],
        vec![(
            "bar-focus",
            Icon::Focus,
            "Focus mode",
            Box::new(ToggleFocusMode),
            |a| Some(a.focus),
        )],
    ]
}

/// How much of the bar fits: everything, or with the word count left out
/// (the Info panel has it), and then the inserts, and the blocks too,
/// folded into a menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    Full,
    NoWords,
    FoldInserts,
    FoldBlocks,
}

/// Widths of the bar's parts, in points, to choose a [`Fit`] before
/// drawing it.
const BUTTON_WIDTH: f32 = 31.;
const SEPARATOR_WIDTH: f32 = 9.;
const HEADING_WIDTH: f32 = 52.;
const BAR_PADDING: f32 = 14.;
/// About one character of the word count, at 12 points.
const WORD_CHAR_WIDTH: f32 = 7.;

/// The widest fit of the bar, with a word count `words` characters long,
/// in `room` points.
pub fn fit(words: usize, room: f32) -> Fit {
    let groups = groups();
    let width = |folded: usize| {
        let mut width = BAR_PADDING + HEADING_WIDTH;
        for (ix, group) in groups.iter().enumerate() {
            let folds = (1..=2).contains(&ix) && 3 - ix <= folded;
            if folds {
                continue;
            }
            width += SEPARATOR_WIDTH + BUTTON_WIDTH * group.len() as f32;
        }
        if folded > 0 {
            width += SEPARATOR_WIDTH + BUTTON_WIDTH;
        }
        width
    };
    let words = SEPARATOR_WIDTH + 12. + WORD_CHAR_WIDTH * words as f32;
    if width(0) + words <= room {
        Fit::Full
    } else if width(0) <= room {
        Fit::NoWords
    } else if width(1) <= room {
        Fit::FoldInserts
    } else {
        Fit::FoldBlocks
    }
}

/// A button that shows the folded groups' commands as a menu.
fn more_button(folded: Vec<ButtonSpec>, theme: &Theme) -> impl IntoElement {
    let items: Vec<(&'static str, Box<dyn Action>)> = folded
        .into_iter()
        .map(|(_, _, name, action, _)| (name, action))
        .collect();
    button(
        "bar-more",
        Icon::Ellipsis.element(16.).text_color(theme.text),
        "More",
        theme,
        None,
        move |event, window, cx| {
            let mut menu = NativeMenu::new();
            for (name, action) in &items {
                menu = menu.menu(*name, action.boxed_clone());
            }
            menu.show(event.position(), window, cx);
        },
    )
}

/// The formatting buttons, headed by the heading level, with the groups
/// `fit` leaves out folded into a menu.
fn buttons(heading: String, active: &Active, fit: Fit, theme: &Theme) -> impl IntoElement {
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
    let mut folded = Vec::new();
    for (ix, group) in groups().into_iter().enumerate() {
        let folds = (ix == 2 && fit >= Fit::FoldInserts) || (ix == 1 && fit >= Fit::FoldBlocks);
        if folds {
            folded.extend(group);
            continue;
        }
        // The menu stands where the folded groups were, before focus mode.
        if ix == 3 && !folded.is_empty() {
            row = row
                .child(separator(theme))
                .child(more_button(std::mem::take(&mut folded), theme));
        }
        row = row.child(separator(theme));
        for (id, icon, tooltip, action, is_active) in group {
            row = row.child(action_button(
                id,
                icon,
                tooltip,
                theme,
                action,
                is_active(active),
            ));
        }
    }
    row
}

/// The bar: a floating strip of formatting buttons centered at the bottom,
/// with the word count at its end.
pub fn render(state: &BarState, goal: Option<usize>, room: f32, theme: &Theme) -> impl IntoElement {
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
    let fit = fit(words.chars().count(), room - 2. * BAR_MARGIN);
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
                .child(buttons(heading, &state.active, fit, theme))
                .when(fit == Fit::Full, |d| {
                    d.child(separator(theme)).child(
                        div()
                            .id("bar-words")
                            .test_support()
                            .px(px(6.))
                            .whitespace_nowrap()
                            .child(words),
                    )
                }),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_folds_what_does_not_fit() {
        assert_eq!(fit(15, 900.), Fit::Full);
        assert_eq!(fit(15, 580.), Fit::NoWords);
        assert_eq!(fit(15, 480.), Fit::FoldInserts);
        assert_eq!(fit(15, 300.), Fit::FoldBlocks);
    }
}
