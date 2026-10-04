//! After Bear: the buttons in the window's top-right corner, and the panel
//! they open at its right edge, with the document's statistics and details,
//! quick settings, and its outline.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::switch::Switch;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, Window, actions, div, px,
};

use super::Workspace;
use crate::editor::{CopyHtml, ExportHtml, ExportPdf, Print, ShowInFinder, ToggleFocusMode};
use crate::icons::Icon;
use crate::settings::{
    Appearance, BiggerText, ColumnWidth, Keyboard, OpenSettings, Settings, SmallerText,
};
use crate::switcher::GoToHeading;
use crate::theme::{DIMMED, MONO_FONT, Theme};

actions!(
    focal,
    [ToggleInfo, ToggleOutline, ToggleRecent, ToggleDarkMode]
);

pub const PANEL_WIDTH: f32 = 300.;

/// What the panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelTab {
    Info,
    Outline,
    Recent,
}

impl Workspace {
    /// Opens the panel on `tab`, or closes it when it already shows it.
    pub(super) fn toggle_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        self.panel = if self.panel == Some(tab) {
            None
        } else {
            Some(tab)
        };
        cx.notify();
    }

    /// Light when Focal looks dark, dark when it looks light.
    pub(super) fn toggle_dark_mode(window: &Window, cx: &mut App) {
        let dark = matches!(
            window.appearance(),
            gpui_kit::WindowAppearance::Dark | gpui_kit::WindowAppearance::VibrantDark
        );
        crate::settings::update(cx, |settings| {
            settings.appearance = if dark {
                Appearance::Light
            } else {
                Appearance::Dark
            };
        });
    }

    /// The outline, info and more buttons in the top-right corner. Focus
    /// mode fades them like the title.
    pub(super) fn render_corner(
        &self,
        theme: &Theme,
        focus_mode: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let hover = theme.code_background;
        let button = |id: &'static str, icon: Icon, label: &'static str, active: bool| {
            div()
                .id(id)
                .test_support()
                .aria_label(label)
                .size(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .cursor_pointer()
                .when(active, |d| d.bg(hover))
                .hover(move |style| style.bg(hover))
                .child(
                    icon.element(15.)
                        .text_color(if active { theme.text } else { theme.marker }),
                )
        };
        div()
            .id("corner")
            .absolute()
            .top(px(5.))
            .right(px(10.))
            .flex()
            .items_center()
            .gap(px(2.))
            .p(px(2.))
            .rounded(px(9.))
            .when(focus_mode, |d| d.opacity(DIMMED))
            .child(
                button(
                    "corner-outline",
                    Icon::ListTree,
                    "Outline",
                    self.panel == Some(PanelTab::Outline),
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_panel(PanelTab::Outline, cx))),
            )
            .child(
                button(
                    "corner-info",
                    Icon::Info,
                    "Info",
                    self.panel == Some(PanelTab::Info),
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_panel(PanelTab::Info, cx))),
            )
            .child(button("corner-more", Icon::Ellipsis, "More", false).on_click(more_menu))
            .into_any_element()
    }

    /// The panel at the right edge, when it is open and focus mode is off.
    pub(super) fn render_panel(&self, theme: &Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let tab = self.panel?;
        if self.editor.read(cx).focus_mode() {
            return None;
        }
        let body = match tab {
            PanelTab::Info => self.render_info(theme, cx),
            PanelTab::Outline => self.render_outline(theme, cx),
            PanelTab::Recent => render_recent(self.editor.read(cx).path(), theme, cx),
        };
        let tab_button = |id: &'static str, label: &'static str, which: PanelTab| {
            let selected = tab == which;
            div()
                .id(id)
                .test_support()
                .aria_label(label)
                .flex_1()
                .py(px(3.))
                .flex()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(12.))
                .when(selected, |d| d.bg(theme.background).text_color(theme.text))
                .when(!selected, |d| d.text_color(theme.marker))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.panel = Some(which);
                    cx.notify();
                }))
        };
        Some(
            div()
                .id("panel")
                .test_support()
                .flex_none()
                .w(px(PANEL_WIDTH))
                .h_full()
                .flex()
                .flex_col()
                .bg(theme.surface)
                .border_l_1()
                .border_color(theme.rule)
                .text_size(px(13.))
                .text_color(theme.text)
                .child(
                    div()
                        .flex_none()
                        .h(px(40.))
                        .px(px(12.))
                        .pt(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .p(px(2.))
                                .rounded(px(8.))
                                .bg(theme.code_background)
                                .child(tab_button("panel-tab-info", "Info", PanelTab::Info))
                                .child(tab_button(
                                    "panel-tab-outline",
                                    "Outline",
                                    PanelTab::Outline,
                                ))
                                .child(tab_button("panel-tab-recent", "Recent", PanelTab::Recent)),
                        )
                        .child(
                            div()
                                .id("panel-close")
                                .test_support()
                                .aria_label("Close")
                                .size(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .cursor_pointer()
                                .hover(|style| style.opacity(0.7))
                                .child(Icon::X.element(14.).text_color(theme.marker))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.panel = None;
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .id("panel-body")
                        .flex_1()
                        .overflow_y_scroll()
                        .px(px(14.))
                        .pt(px(8.))
                        .pb(px(24.))
                        .child(body),
                )
                .into_any_element(),
        )
    }

    fn render_info(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let editor = self.editor.read(cx);
        let settings = cx.global::<Settings>();
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(statistics_section(editor.text(), &editor.selection, theme))
            .child(document_section(editor, theme))
            .child(appearance_section(settings, theme))
            .child(writing_section(settings, editor.focus_mode(), theme))
            .child(
                div()
                    .id("panel-settings")
                    .test_support()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(theme.link)
                    .hover(|style| style.underline())
                    .child(Icon::Settings.element(13.).text_color(theme.link))
                    .child("All Settings…")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenSettings), cx)),
            )
            .into_any_element()
    }

    fn render_outline(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let editor = self.editor.read(cx);
        let headings = focal_core::outline::outline(editor.text());
        if headings.is_empty() {
            return div()
                .pt(px(8.))
                .text_color(theme.marker)
                .child("No headings yet. Start a line with # to add one.")
                .into_any_element();
        }
        let head = editor.selection.start;
        // The heading the caret is under.
        let text = editor.text();
        let current = headings.iter().rposition(|heading| {
            let line = text[..heading.offset].rfind('\n').map_or(0, |ix| ix + 1);
            line <= head
        });
        let hover = theme.code_background;
        let rows = headings.into_iter().enumerate().map(|(ix, heading)| {
            let selected = current == Some(ix);
            let at = heading.offset;
            div()
                .id(("outline", ix))
                .test_support()
                .pl(px(8. + 12. * f32::from(heading.level.saturating_sub(1))))
                .pr(px(8.))
                .py(px(4.))
                .rounded(px(6.))
                .cursor_pointer()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .when(heading.level == 1, |d| d.font_weight(FontWeight::SEMIBOLD))
                .when(selected, |d| d.bg(theme.selection))
                .when(!selected, |d| d.hover(move |style| style.bg(hover)))
                .text_color(if heading.level <= 2 {
                    theme.text
                } else {
                    theme.marker
                })
                .child(heading.title)
                .on_click(cx.listener(move |this, _, window, cx| this.jump_to(at, window, cx)))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .children(rows)
            .into_any_element()
    }
}

/// Counts for the document, and for the selection when there is one.
fn statistics_section(
    text: &str,
    selection: &std::ops::Range<usize>,
    theme: &Theme,
) -> impl IntoElement {
    let stats = focal_core::text_stats::statistics(text);
    let selected = (!selection.is_empty())
        .then(|| focal_core::text_stats::statistics(&text[selection.clone()]));
    let tiles = [
        ("Words", stats.words.to_string()),
        ("Characters", stats.characters.to_string()),
        ("Reading time", minutes(stats.reading_minutes)),
        ("Paragraphs", stats.paragraphs.to_string()),
        ("Sentences", stats.sentences.to_string()),
        (
            "Without spaces",
            stats.characters_without_spaces.to_string(),
        ),
    ];
    section("Statistics", theme)
        .child(
            div().flex().flex_wrap().gap(px(6.)).children(
                tiles
                    .into_iter()
                    .map(|(label, value)| stat_tile(label, value, theme)),
            ),
        )
        .when_some(selected, |d, selected| {
            d.child(
                div()
                    .text_size(px(12.))
                    .text_color(theme.marker)
                    .child(format!(
                        "Selected: {} words, {} characters",
                        selected.words, selected.characters
                    )),
            )
        })
}

/// The file: its name, its folder (which shows it in Finder), when it last
/// changed, its size and how it is stored.
fn document_section(editor: &crate::editor::Editor, theme: &Theme) -> impl IntoElement {
    let mut details = div().flex().flex_col().gap(px(6.));
    let Some(path) = editor.path().map(PathBuf::from) else {
        return section("Document", theme).child(details.child(detail(
            Icon::FileText,
            "Untitled, not saved yet".to_owned(),
            theme,
        )));
    };
    let folder = path
        .parent()
        .map(|folder| folder.to_string_lossy().replace(&home(), "~"))
        .unwrap_or_default();
    details = details
        .child(detail(Icon::FileText, editor.title(), theme))
        .child(
            detail(Icon::Folder, folder, theme)
                .id("panel-folder")
                .cursor_pointer()
                .hover(|style| style.opacity(0.7))
                .on_click(|_: &ClickEvent, window, cx| {
                    window.dispatch_action(Box::new(ShowInFinder), cx);
                }),
        );
    if let Ok(metadata) = std::fs::metadata(&path) {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| SystemTime::now().duration_since(time).ok())
            .map_or_else(String::new, |age| format!("Changed {}", ago(age)));
        let storage = editor.storage_note().unwrap_or_else(|| "UTF-8".to_owned());
        details = details
            .child(detail(Icon::Clock, modified, theme))
            .child(detail(
                Icon::Hash,
                format!("{} · {storage}", size(metadata.len())),
                theme,
            ));
    }
    section("Document", theme).child(details)
}

/// Light, dark or automatic, the typeface, the size and the line length.
fn appearance_section(settings: &Settings, theme: &Theme) -> impl IntoElement {
    let appearance = [
        (Appearance::Light, Icon::Sun, "Light"),
        (Appearance::Dark, Icon::Moon, "Dark"),
        (Appearance::System, Icon::SunMoon, "Auto"),
    ]
    .into_iter()
    .map(|(value, icon, label)| {
        let selected = settings.appearance == value;
        div()
            .id(SharedString::from(format!("panel-appearance-{label}")))
            .test_support()
            .aria_label(label)
            .flex_1()
            .py(px(5.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .rounded(px(7.))
            .cursor_pointer()
            .text_size(px(11.))
            .when(selected, |d| d.bg(theme.background).text_color(theme.text))
            .when(!selected, |d| d.text_color(theme.marker))
            .child(
                icon.element(15.)
                    .text_color(if selected { theme.caret } else { theme.marker }),
            )
            .child(label)
            .on_click(move |_, _, cx| crate::settings::update(cx, |s| s.appearance = value))
    });
    let size = crate::theme::Typography::new(settings).size;
    section("Appearance", theme)
        .child(
            div()
                .flex()
                .p(px(2.))
                .rounded(px(9.))
                .bg(theme.code_background)
                .children(appearance),
        )
        .child(crate::settings::typeface_grid(settings, theme, 84.))
        .child(
            div()
                .pt(px(4.))
                .text_size(px(11.))
                .text_color(theme.marker)
                .child("Code"),
        )
        .child(crate::settings::code_font_grid(settings, theme, 84.))
        .child(
            quick_row("Size", theme).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(step_button("panel-smaller", "A−", theme, || {
                        Box::new(SmallerText)
                    }))
                    .child(
                        div()
                            .w(px(44.))
                            .flex()
                            .justify_center()
                            .font_family(MONO_FONT)
                            .text_size(px(12.))
                            .child(format!("{size} pt")),
                    )
                    .child(step_button("panel-bigger", "A+", theme, || {
                        Box::new(BiggerText)
                    })),
            ),
        )
        .child(
            quick_row("Line length", theme).child(crate::settings::segmented(
                "panel-column",
                &[
                    (ColumnWidth::Narrow, "S"),
                    (ColumnWidth::Medium, "M"),
                    (ColumnWidth::Wide, "L"),
                ],
                settings.column_width,
                |s, v| s.column_width = v,
                theme,
            )),
        )
}

/// Focus mode, typewriter scrolling and the editing keys.
fn writing_section(settings: &Settings, focus_mode: bool, theme: &Theme) -> impl IntoElement {
    section("Writing", theme)
        .child(
            quick_row("Focus mode", theme).child(
                Switch::new("panel-focus")
                    .checked(focus_mode)
                    .color(theme.caret)
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(ToggleFocusMode), cx);
                    }),
            ),
        )
        .child(
            quick_row("Typewriter scrolling", theme).child(crate::settings::toggle(
                "panel-typewriter",
                settings.typewriter,
                |s, v| s.typewriter = v,
                theme,
            )),
        )
        .child(quick_row("Keys", theme).child(crate::settings::segmented(
            "panel-keys",
            &[
                (Keyboard::Standard, "Mac"),
                (Keyboard::Vim, "Vim"),
                (Keyboard::Helix, "Helix"),
            ],
            settings.keyboard,
            |s, v| s.keyboard = v,
            theme,
        )))
}

/// The files and folders opened lately, the current one marked; a click
/// opens one, or brings its window forward.
fn render_recent(current: Option<&std::path::Path>, theme: &Theme, cx: &App) -> AnyElement {
    let paths = cx
        .try_global::<crate::recent::Recent>()
        .map(crate::recent::Recent::existing)
        .unwrap_or_default();
    if paths.is_empty() {
        return div()
            .pt(px(8.))
            .text_color(theme.marker)
            .child("Files you open show here.")
            .into_any_element();
    }
    let labels = crate::recent::labels(&paths);
    let hover = theme.code_background;
    let rows = paths
        .into_iter()
        .zip(labels)
        .enumerate()
        .map(|(ix, (path, label))| {
            let selected = current == Some(path.as_path());
            let icon = if path.is_dir() {
                Icon::Folder
            } else {
                Icon::FileText
            };
            let folder = path
                .parent()
                .map(|folder| folder.to_string_lossy().replace(&home(), "~"))
                .unwrap_or_default();
            div()
                .id(("recent", ix))
                .test_support()
                .px(px(8.))
                .py(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(8.))
                .when(selected, |d| d.bg(theme.selection))
                .when(!selected, |d| d.hover(move |style| style.bg(hover)))
                .child(icon.element(14.).text_color(theme.marker))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(label),
                        )
                        .child(
                            div()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_size(px(11.))
                                .text_color(theme.marker)
                                .child(folder),
                        ),
                )
                .on_click(move |_, window, cx| {
                    window.dispatch_action(Box::new(crate::recent::OpenRecent(path.clone())), cx);
                })
        });
    div()
        .flex()
        .flex_col()
        .gap(px(1.))
        .children(rows)
        .into_any_element()
}

/// The corner's "more" menu: what the File and View menus hold for this
/// document.
fn more_menu(event: &ClickEvent, window: &mut Window, cx: &mut App) {
    NativeMenu::new()
        .menu("Go to Heading…", Box::new(GoToHeading))
        .menu("Focus Mode", Box::new(ToggleFocusMode))
        .menu("Toggle Dark Mode", Box::new(ToggleDarkMode))
        .separator()
        .menu("Copy as HTML", Box::new(CopyHtml))
        .menu("Export as HTML…", Box::new(ExportHtml))
        .menu("Export as PDF…", Box::new(ExportPdf))
        .menu("Print…", Box::new(Print))
        .separator()
        .menu("Show in Finder", Box::new(ShowInFinder))
        .menu("Settings…", Box::new(OpenSettings))
        .show(event.position(), window, cx);
}

fn section(title: &'static str, theme: &Theme) -> gpui_kit::Div {
    div().flex().flex_col().gap(px(8.)).child(
        div()
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.marker)
            .child(title),
    )
}

fn stat_tile(label: &'static str, value: String, theme: &Theme) -> impl IntoElement {
    div()
        .w(px(84.))
        .px(px(8.))
        .py(px(6.))
        .rounded(px(8.))
        .bg(theme.code_background)
        .flex()
        .flex_col()
        .child(
            div()
                .font_family(MONO_FONT)
                .text_size(px(15.))
                .text_color(theme.text)
                .child(value),
        )
        .child(
            div()
                .text_size(px(10.5))
                .text_color(theme.marker)
                .whitespace_nowrap()
                .child(label),
        )
}

fn detail(icon: Icon, text: String, theme: &Theme) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .min_w(px(0.))
        .child(icon.element(14.).text_color(theme.marker))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .child(text),
        )
}

fn quick_row(label: &'static str, theme: &Theme) -> gpui_kit::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(div().text_color(theme.text).child(label))
}

fn step_button(
    id: &'static str,
    label: &'static str,
    theme: &Theme,
    action: fn() -> Box<dyn gpui_kit::Action>,
) -> impl IntoElement {
    let hover = theme.rule;
    div()
        .id(id)
        .test_support()
        .aria_label(label)
        .px(px(8.))
        .py(px(2.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.rule)
        .bg(theme.background)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(label)
        .on_click(move |_, window, cx| window.dispatch_action(action(), cx))
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

fn minutes(minutes: usize) -> String {
    if minutes == 0 {
        "–".to_owned()
    } else {
        format!("{minutes} min")
    }
}

/// A file size in bytes, KB or MB.
fn size(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let kb = bytes as f64 / 1000.;
    if bytes < 1000 {
        format!("{bytes} bytes")
    } else if kb < 1000. {
        format!("{kb:.1} KB")
    } else {
        format!("{:.1} MB", kb / 1000.)
    }
}

/// How long ago, roughly: "just now", "5 minutes ago", "3 days ago".
fn ago(age: Duration) -> String {
    let seconds = age.as_secs();
    let (amount, unit) = match seconds {
        0..60 => return "just now".to_owned(),
        60..3_600 => (seconds / 60, "minute"),
        3_600..86_400 => (seconds / 3_600, "hour"),
        86_400..2_592_000 => (seconds / 86_400, "day"),
        2_592_000..31_536_000 => (seconds / 2_592_000, "month"),
        _ => (seconds / 31_536_000, "year"),
    };
    let plural = if amount == 1 { "" } else { "s" };
    format!("{amount} {unit}{plural} ago")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_naturally() {
        assert_eq!(ago(Duration::from_secs(5)), "just now");
        assert_eq!(ago(Duration::from_mins(1)), "1 minute ago");
        assert_eq!(ago(Duration::from_hours(2)), "2 hours ago");
        assert_eq!(ago(Duration::from_hours(72)), "3 days ago");
    }

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(size(512), "512 bytes");
        assert_eq!(size(12_400), "12.4 KB");
        assert_eq!(size(3_200_000), "3.2 MB");
    }
}
