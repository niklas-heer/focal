//! Focal's few settings, stored as JSON in Application Support and shared as a
//! GPUI global.

use std::path::{Path, PathBuf};

use gpui_kit::component::radio::{Radio, RadioGroup};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::Theme;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, BorrowAppContext as _, Bounds, Context, Global,
    IntoElement, KeyBinding, ParentElement as _, Render, Styled as _, TitlebarOptions, Window,
    WindowBounds, WindowOptions, actions, div, px, size,
};
use serde::{Deserialize, Serialize};

use crate::theme::{PROSE_FONT, Theme as Colors};

actions!(focal, [OpenSettings]);

/// What focus mode keeps bright.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusUnit {
    Sentence,
    #[default]
    Paragraph,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub focus_unit: FocusUnit,
    /// Keep the caret's line vertically centered in focus mode.
    pub typewriter: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            focus_unit: FocusUnit::Paragraph,
            typewriter: true,
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
    cx.bind_keys([KeyBinding::new("cmd-,", OpenSettings, None)]);
    cx.on_action(|_: &OpenSettings, cx| open_window(cx));
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
    let bounds = Bounds::centered(None, size(px(420.), px(260.)), cx);
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

struct SettingsView;

impl SettingsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Settings>(|_, cx| cx.notify()).detach();
        // GPUI Kit's controls follow the system appearance, like Focal.
        Theme::sync_system_appearance(Some(window), cx);
        cx.observe_window_appearance(window, |_, window, cx| {
            Theme::sync_system_appearance(Some(window), cx);
        })
        .detach();
        Self
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Colors::for_appearance(window.appearance());
        let settings = cx.global::<Settings>().clone();
        let heading = |text: &'static str| {
            div()
                .text_size(px(12.))
                .text_color(theme.marker)
                .child(text)
        };
        div()
            .size_full()
            .p(px(24.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(PROSE_FONT)
            .text_size(px(14.))
            .child(heading("FOCUS MODE KEEPS BRIGHT"))
            .child(
                RadioGroup::vertical("focus-unit")
                    .child(Radio::new("sentence").label("The sentence"))
                    .child(Radio::new("paragraph").label("The paragraph"))
                    .selected_index(Some(match settings.focus_unit {
                        FocusUnit::Sentence => 0,
                        FocusUnit::Paragraph => 1,
                    }))
                    .on_click(|index, _, cx| {
                        let unit = if *index == 0 {
                            FocusUnit::Sentence
                        } else {
                            FocusUnit::Paragraph
                        };
                        update(cx, |settings| settings.focus_unit = unit);
                    }),
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
        };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
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
