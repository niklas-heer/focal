//! Draws a line's prefix (quote bars, list markers, checkboxes) as elements in
//! columns beside the content, so wrapped text hangs under its first word.

use focal_core::{LinePrefix, ListMarker, PrefixLevel};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels,
    Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::theme::Theme;

/// Width of a list level's marker column.
pub const MARKER_COLUMN: f32 = 28.;

/// Wraps `content` in its prefix, innermost level closest to the content.
pub fn wrap(
    content: AnyElement,
    prefix: &LinePrefix,
    line: usize,
    theme: &Theme,
    line_height: Pixels,
    on_toggle: impl Fn(usize, &mut Window, &mut App) + Clone + 'static,
) -> AnyElement {
    let depth_of = |index: usize| {
        prefix.levels[..index]
            .iter()
            .filter(|l| matches!(l, PrefixLevel::List(_)))
            .count()
    };
    let mut element = content;
    for (index, level) in prefix.levels.iter().enumerate().rev() {
        element = match level {
            PrefixLevel::Quote(alert) | PrefixLevel::Block(alert) => div()
                .border_l(px(3.))
                .border_color(alert.map_or(theme.quote_bar, |alert| theme.alert(alert)))
                .pl(px(14.))
                .child(element)
                .into_any_element(),
            PrefixLevel::List(marker) => div()
                .flex()
                .items_start()
                .child(
                    div()
                        .w(px(MARKER_COLUMN))
                        .h(line_height)
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_end()
                        .pr(px(8.))
                        .children(marker.as_ref().map(|marker| {
                            marker_element(marker, depth_of(index), line, theme, on_toggle.clone())
                        })),
                )
                .child(div().flex_1().min_w(px(0.)).child(element))
                .into_any_element(),
        };
    }
    element
}

fn marker_element(
    marker: &ListMarker,
    depth: usize,
    line: usize,
    theme: &Theme,
    on_toggle: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    match marker {
        ListMarker::Bullet => {
            let dot = div().size(px(6.)).rounded_full();
            match depth {
                0 => dot.bg(theme.text),
                1 => dot.border_1().border_color(theme.text),
                _ => dot.rounded(px(1.)).size(px(5.)).bg(theme.marker),
            }
            .into_any_element()
        }
        ListMarker::Ordered(label) => div()
            .text_color(theme.marker)
            .child(label.clone())
            .into_any_element(),
        ListMarker::Task { checked } => div()
            .id(("task", line))
            .test_support()
            .size(px(14.))
            .rounded(px(3.))
            .border_1()
            .border_color(if *checked {
                theme.checkbox
            } else {
                theme.marker
            })
            .when(*checked, |d| {
                d.bg(theme.checkbox)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .text_color(theme.background)
                    .child("✓")
            })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                on_toggle(line, window, cx);
            })
            .into_any_element(),
    }
}

/// What the prefix says, for the accessibility tree: "• ", "1. ", "☐ " or "☑ ".
pub fn marker_text(prefix: &LinePrefix) -> String {
    prefix
        .levels
        .iter()
        .find_map(|level| match level {
            PrefixLevel::List(Some(ListMarker::Bullet)) => Some("• ".to_owned()),
            PrefixLevel::List(Some(ListMarker::Ordered(label))) => Some(format!("{label} ")),
            PrefixLevel::List(Some(ListMarker::Task { checked })) => {
                Some(if *checked { "☑ " } else { "☐ " }.to_owned())
            }
            _ => None,
        })
        .unwrap_or_default()
}
