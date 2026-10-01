//! Focal's few settings, stored as JSON in Application Support and shared as a
//! GPUI global.

use std::path::{Path, PathBuf};

use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::Theme;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, BorrowAppContext as _, Bounds, Context, FocusHandle,
    Global, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render,
    SharedString, Styled as _, TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div,
    px, size,
};
use serde::{Deserialize, Serialize};

use crate::theme::{MONO_FONT, PROSE_FONT, Theme as Colors};

actions!(focal, [OpenSettings, CloseSettings]);

/// What focus mode keeps bright.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusUnit {
    Sentence,
    #[default]
    Paragraph,
}

/// The typeface for prose; code is always in iA Writer Mono.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProseFont {
    #[default]
    Quattro,
    Duo,
    Mono,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSize {
    Small,
    #[default]
    Medium,
    Large,
    Huge,
}

/// The text column's width, about 60, 70 or 85 characters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnWidth {
    Narrow,
    #[default]
    Medium,
    Wide,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub focus_unit: FocusUnit,
    /// Keep the caret's line vertically centered in focus mode.
    pub typewriter: bool,
    pub prose_font: ProseFont,
    pub text_size: TextSize,
    pub column_width: ColumnWidth,
    /// Let a release build look for updates by itself.
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            focus_unit: FocusUnit::Paragraph,
            typewriter: true,
            prose_font: ProseFont::default(),
            text_size: TextSize::default(),
            column_width: ColumnWidth::default(),
            check_updates: true,
        }
    }
}

impl Global for Settings {}

impl Settings {
    /// Reads settings from `path`; a missing or unreadable file gives the
    /// defaults, and unknown fields are ignored.
    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    /// `~/Library/Application Support/Focal/settings.json`.
    fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/Application Support/Focal/settings.json"))
    }

    fn save(&self) {
        if let Some(path) = Self::path()
            && let Err(error) = self.save_to(&path)
        {
            eprintln!("focal: could not save settings: {error:#}");
        }
    }
}

/// Loads the settings and registers the settings window's action.
pub fn init(cx: &mut App) {
    cx.set_global(Settings::path().map_or_else(Settings::default, |p| Settings::load_from(&p)));
    cx.set_global(SettingsWindow(None));
    cx.bind_keys([
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-w", CloseSettings, Some("FocalSettings")),
        KeyBinding::new("escape", CloseSettings, Some("FocalSettings")),
    ]);
    cx.on_action(|_: &OpenSettings, cx| open_window(cx));
}

/// Keeps GPUI Kit's controls and tooltips in the system appearance, like
/// Focal's own colors, for the life of the window's view.
pub fn follow_appearance<V: 'static>(window: &mut Window, cx: &mut Context<V>) {
    Theme::sync_system_appearance(Some(window), cx);
    cx.observe_window_appearance(window, |_, window, cx| {
        Theme::sync_system_appearance(Some(window), cx);
    })
    .detach();
}

/// Changes the settings, saves them and redraws every window.
pub fn update(cx: &mut App, change: impl FnOnce(&mut Settings)) {
    cx.update_global::<Settings, _>(|settings, _| change(settings));
    cx.global::<Settings>().save();
    cx.refresh_windows();
}

/// The open settings window, so a second ⌘, brings it forward.
struct SettingsWindow(Option<AnyWindowHandle>);

impl Global for SettingsWindow {}

fn open_window(cx: &mut App) {
    if let Some(handle) = cx.global::<SettingsWindow>().0
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(460.), px(660.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("Settings".into()),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        is_resizable: false,
        ..WindowOptions::default()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| SettingsView::new(window, cx))
    }) {
        Ok((handle, _)) => cx.set_global(SettingsWindow(Some(handle))),
        Err(error) => eprintln!("focal: could not open the settings: {error:#}"),
    }
}

struct SettingsView {
    focus: FocusHandle,
}

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Settings>(|_, cx| cx.notify()).detach();
        follow_appearance(window, cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self { focus }
    }
}

/// A row of radio buttons choosing one of `options`, saved through `set`.
fn choice<T: Copy + PartialEq + 'static>(
    id: &'static str,
    options: &'static [(T, &'static str)],
    current: T,
    set: fn(&mut Settings, T),
) -> RadioGroup {
    RadioGroup::horizontal(id)
        .children(options.iter().map(|&(_, label)| {
            Radio::new(SharedString::from(format!("{id}-{label}"))).label(label)
        }))
        .selected_index(options.iter().position(|&(value, _)| value == current))
        .on_click(move |index, _, cx| {
            if let Some(&(value, _)) = options.get(*index) {
                update(cx, |settings| set(settings, value));
            }
        })
}

/// A diagram tool, whether it is installed, and how to install it.
fn tool_row(tool: &crate::diagram::Tool, theme: &Colors) -> impl IntoElement + use<> {
    let status = if tool.installed {
        "installed".to_owned()
    } else {
        format!("not installed: {}", tool.install)
    };
    div()
        .id(SharedString::from(format!("diagram-tool-{}", tool.command)))
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .gap(px(6.))
                .child(div().font_family(MONO_FONT).child(tool.command))
                .child(
                    div()
                        .text_color(if tool.installed {
                            theme.text
                        } else {
                            theme.marker
                        })
                        .child(status),
                ),
        )
        .child(
            div()
                .text_size(px(12.))
                .text_color(theme.marker)
                .child(tool.draws),
        )
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Colors::for_appearance(window.appearance());
        let settings = cx.global::<Settings>().clone();
        let heading = |text: &'static str| {
            div()
                .pt(px(6.))
                .text_size(px(11.))
                .text_color(theme.marker)
                .child(text)
        };
        div()
            .key_context("FocalSettings")
            .track_focus(&self.focus)
            .on_action(|_: &CloseSettings, window, _| window.remove_window())
            .size_full()
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(PROSE_FONT)
            .text_size(px(14.))
            .child(heading("TYPEFACE"))
            .child(choice(
                "prose-font",
                &[
                    (ProseFont::Quattro, "Quattro"),
                    (ProseFont::Duo, "Duo"),
                    (ProseFont::Mono, "Mono"),
                ],
                settings.prose_font,
                |s, v| s.prose_font = v,
            ))
            .child(heading("TEXT SIZE"))
            .child(choice(
                "text-size",
                &[
                    (TextSize::Small, "Small"),
                    (TextSize::Medium, "Medium"),
                    (TextSize::Large, "Large"),
                    (TextSize::Huge, "Huge"),
                ],
                settings.text_size,
                |s, v| s.text_size = v,
            ))
            .child(heading("COLUMN WIDTH"))
            .child(choice(
                "column-width",
                &[
                    (ColumnWidth::Narrow, "Narrow"),
                    (ColumnWidth::Medium, "Medium"),
                    (ColumnWidth::Wide, "Wide"),
                ],
                settings.column_width,
                |s, v| s.column_width = v,
            ))
            .child(heading("FOCUS MODE KEEPS BRIGHT"))
            .child(choice(
                "focus-unit",
                &[
                    (FocusUnit::Sentence, "The sentence"),
                    (FocusUnit::Paragraph, "The paragraph"),
                ],
                settings.focus_unit,
                |s, v| s.focus_unit = v,
            ))
            .child(heading("UPDATES"))
            .child(
                Switch::new("check-updates")
                    .label("Check for updates automatically")
                    .checked(settings.check_updates)
                    .on_click(|checked, _, cx| {
                        let checked = *checked;
                        update(cx, |settings| settings.check_updates = checked);
                    }),
            )
            .child(heading("DIAGRAM TOOLS"))
            .children(
                crate::diagram::tools()
                    .into_iter()
                    .map(|tool| tool_row(&tool, &theme)),
            )
            .child(heading("TYPEWRITER SCROLLING"))
            .child(
                Switch::new("typewriter")
                    .label("Keep the current line centered in focus mode")
                    .checked(settings.typewriter)
                    .on_click(|checked, _, cx| {
                        let checked = *checked;
                        update(cx, |settings| settings.typewriter = checked);
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("focal-settings-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn settings_round_trip() {
        let path = temp("round-trip");
        let settings = Settings {
            focus_unit: FocusUnit::Sentence,
            typewriter: false,
            prose_font: ProseFont::Duo,
            text_size: TextSize::Large,
            column_width: ColumnWidth::Wide,
            check_updates: false,
        };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
    }

    #[test]
    fn updates_are_checked_unless_turned_off() {
        assert!(Settings::default().check_updates);
        let path = temp("updates");
        std::fs::write(&path, r#"{"focus_unit": "sentence"}"#).unwrap();
        assert!(
            Settings::load_from(&path).check_updates,
            "older files keep checking"
        );
    }

    #[test]
    fn a_missing_file_or_unknown_fields_give_defaults() {
        assert_eq!(
            Settings::load_from(&temp("missing").join("nope")),
            Settings::default()
        );
        let path = temp("unknown");
        std::fs::write(&path, r#"{"typewriter": false, "later": 1}"#).unwrap();
        assert_eq!(
            Settings::load_from(&path),
            Settings {
                typewriter: false,
                ..Settings::default()
            }
        );
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Settings::load_from(&path), Settings::default());
    }
}
