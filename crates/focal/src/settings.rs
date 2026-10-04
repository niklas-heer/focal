//! Focal's few settings, stored as JSON in Application Support and shared as a
//! GPUI global, and the Settings window that changes them.

use std::path::{Path, PathBuf};

use gpui_kit::component::switch::Switch;
use gpui_kit::component::theme::Theme;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, BorrowAppContext as _, Bounds, Context,
    FocusHandle, FontWeight, Global, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, Render, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, TitlebarOptions, Window, WindowBounds, WindowControlArea, WindowOptions,
    actions, div, point, px, size,
};
use serde::{Deserialize, Serialize};

use crate::icons::Icon;
use crate::switcher::{PickItem, Switcher, SwitcherEvent};
use crate::theme::{BOLD_PROSE_FONT, MONO_FONT, PROSE_FONT, Theme as Colors, Typography};

actions!(
    focal,
    [
        OpenSettings,
        CloseSettings,
        NextPane,
        PreviousPane,
        BiggerText,
        SmallerText,
        DefaultTextSize,
    ]
);

/// Shows the settings pane with this index.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct ShowPane(pub usize);

const SETTINGS_CONTEXT: &str = "FocalSettings";

/// Light or dark, or as the Mac is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// How keys edit: as in other Mac apps, or with Vim's or Helix's modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keyboard {
    #[default]
    Standard,
    Vim,
    Helix,
}

impl Keyboard {
    pub const ALL: [Self; 3] = [Self::Standard, Self::Vim, Self::Helix];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Vim => "Vim",
            Self::Helix => "Helix",
        }
    }
}

/// Chooses how keys edit.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct SetKeyboard(pub Keyboard);

/// What focus mode keeps bright.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FocusUnit {
    Sentence,
    #[default]
    Paragraph,
}

/// The typeface for prose. The iA Writer, Commit Mono and `JetBrains` Mono
/// faces come with Focal; the others come with every Mac, and any installed
/// family can be chosen as `Custom`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProseFont {
    #[default]
    Quattro,
    Duo,
    Mono,
    Charter,
    Georgia,
    Palatino,
    Avenir,
    Helvetica,
    System,
    Menlo,
    CommitMono,
    JetBrainsMono,
    /// An installed family, by name.
    Custom(String),
}

impl ProseFont {
    pub const ALL: [Self; 12] = [
        Self::Quattro,
        Self::Duo,
        Self::Mono,
        Self::Charter,
        Self::Georgia,
        Self::Palatino,
        Self::Avenir,
        Self::Helvetica,
        Self::System,
        Self::Menlo,
        Self::CommitMono,
        Self::JetBrainsMono,
    ];

    /// The name people know the typeface by.
    pub fn name(&self) -> &str {
        match self {
            Self::Quattro => "Quattro",
            Self::Duo => "Duo",
            Self::Mono => "Mono",
            Self::Charter => "Charter",
            Self::Georgia => "Georgia",
            Self::Palatino => "Palatino",
            Self::Avenir => "Avenir Next",
            Self::Helvetica => "Helvetica Neue",
            Self::System => "SF Pro",
            Self::Menlo => "Menlo",
            Self::CommitMono => "Commit Mono",
            Self::JetBrainsMono => "JetBrains Mono",
            Self::Custom(family) => family,
        }
    }

    /// The font family GPUI draws it with.
    pub fn family(&self) -> SharedString {
        match self {
            Self::Quattro => crate::theme::PROSE_FONT.into(),
            Self::Duo => crate::theme::BOLD_PROSE_FONT.into(),
            Self::Mono => crate::theme::MONO_FONT.into(),
            // The Mac's own San Francisco, which has no public family name.
            Self::System => ".SystemUIFont".into(),
            Self::CommitMono => "CommitMono".into(),
            Self::Custom(family) => family.clone().into(),
            other => SharedString::new(other.name()),
        }
    }
}

/// The typeface for code, math and other monospaced text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeFont {
    #[default]
    IaMono,
    CommitMono,
    JetBrainsMono,
    Menlo,
    Monaco,
    PtMono,
    /// An installed monospaced family, by name.
    Custom(String),
}

impl CodeFont {
    pub const ALL: [Self; 6] = [
        Self::IaMono,
        Self::CommitMono,
        Self::JetBrainsMono,
        Self::Menlo,
        Self::Monaco,
        Self::PtMono,
    ];

    pub fn name(&self) -> &str {
        match self {
            Self::IaMono => "iA Writer Mono",
            Self::CommitMono => "Commit Mono",
            Self::JetBrainsMono => "JetBrains Mono",
            Self::Menlo => "Menlo",
            Self::Monaco => "Monaco",
            Self::PtMono => "PT Mono",
            Self::Custom(family) => family,
        }
    }

    pub fn family(&self) -> SharedString {
        match self {
            Self::IaMono => crate::theme::MONO_FONT.into(),
            Self::CommitMono => "CommitMono".into(),
            Self::Custom(family) => family.clone().into(),
            other => SharedString::new(other.name()),
        }
    }
}

/// Which typeface "Other…" chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSlot {
    Prose,
    Code,
}

/// Lists the installed families to choose one for prose or code.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct ChooseFont(pub FontSlot);

/// The installed font families, without the Mac's hidden ones; for code,
/// only those whose letters are all as wide as each other.
pub(crate) fn installed_families(cx: &App, slot: FontSlot) -> Vec<String> {
    let text = cx.text_system();
    text.all_font_names()
        .into_iter()
        .filter(|family| !family.starts_with('.'))
        .filter(|family| {
            slot == FontSlot::Prose || {
                let id = text.resolve_font(&gpui_kit::font(family.clone()));
                let width = |c| text.advance(id, px(16.), c).map(|size| size.width).ok();
                width('i').is_some() && width('i') == width('M')
            }
        })
        .collect()
}

/// Chooses an installed family for `slot`, from a picker's list.
pub(crate) fn choose_installed(slot: FontSlot, family: String, cx: &mut App) {
    update(cx, |settings| match slot {
        FontSlot::Prose => settings.prose_font = ProseFont::Custom(family),
        FontSlot::Code => settings.code_font = CodeFont::Custom(family),
    });
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
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub appearance: Appearance,
    pub focus_unit: FocusUnit,
    /// Keep the caret's line vertically centered in focus mode.
    pub typewriter: bool,
    pub prose_font: ProseFont,
    pub code_font: CodeFont,
    pub text_size: TextSize,
    pub column_width: ColumnWidth,
    /// Let a release build look for updates by itself.
    pub check_updates: bool,
    /// Underline what Apple's grammar checker has a suggestion for.
    pub check_grammar: bool,
    /// Correct a misspelled word when it is finished, if macOS's own
    /// "Correct spelling automatically" is on too.
    pub correct_spelling: bool,
    /// Focal's own keys, or Vim's or Helix's modal editing.
    pub keyboard: Keyboard,
}

impl TextSize {
    const ALL: [Self; 4] = [Self::Small, Self::Medium, Self::Large, Self::Huge];

    /// The next size up or down, staying at the ends.
    fn step(self, up: bool) -> Self {
        let ix = Self::ALL.iter().position(|&size| size == self).unwrap_or(1);
        let ix = if up {
            (ix + 1).min(Self::ALL.len() - 1)
        } else {
            ix.saturating_sub(1)
        };
        Self::ALL[ix]
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            focus_unit: FocusUnit::Paragraph,
            typewriter: true,
            prose_font: ProseFont::default(),
            code_font: CodeFont::default(),
            text_size: TextSize::default(),
            column_width: ColumnWidth::default(),
            check_updates: true,
            check_grammar: true,
            correct_spelling: true,
            keyboard: Keyboard::Standard,
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
        // Tests change settings through the window too; never the user's.
        if cfg!(test) {
            return;
        }
        if let Some(path) = Self::path()
            && let Err(error) = self.save_to(&path)
        {
            eprintln!("focal: could not save settings: {error:#}");
        }
    }
}

/// Loads the settings, applies the appearance and registers the settings
/// window's actions.
pub fn init(cx: &mut App) {
    cx.set_global(Settings::path().map_or_else(Settings::default, |p| Settings::load_from(&p)));
    cx.set_global(SettingsWindow(None));
    crate::mac::set_appearance(cx.global::<Settings>().appearance);
    let mut appearance = cx.global::<Settings>().appearance;
    cx.observe_global::<Settings>(move |cx| {
        let chosen = cx.global::<Settings>().appearance;
        if chosen != appearance {
            appearance = chosen;
            crate::mac::set_appearance(chosen);
        }
    })
    .detach();
    let context = Some(SETTINGS_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-w", CloseSettings, context),
        KeyBinding::new("escape", CloseSettings, context),
        KeyBinding::new("cmd-1", ShowPane(0), context),
        KeyBinding::new("cmd-2", ShowPane(1), context),
        KeyBinding::new("cmd-3", ShowPane(2), context),
        KeyBinding::new("cmd-4", ShowPane(3), context),
        KeyBinding::new("cmd-5", ShowPane(4), context),
        KeyBinding::new("cmd-shift-]", NextPane, context),
        KeyBinding::new("cmd-shift-[", PreviousPane, context),
        KeyBinding::new("ctrl-tab", NextPane, context),
        KeyBinding::new("ctrl-shift-tab", PreviousPane, context),
    ]);
    cx.on_action(|_: &OpenSettings, cx| open_window(cx));
    cx.on_action(|action: &SetKeyboard, cx| {
        let keyboard = action.0;
        update(cx, |s| s.keyboard = keyboard);
    });
    cx.on_action(|_: &BiggerText, cx| update(cx, |s| s.text_size = s.text_size.step(true)));
    cx.on_action(|_: &SmallerText, cx| update(cx, |s| s.text_size = s.text_size.step(false)));
    cx.on_action(|_: &DefaultTextSize, cx| update(cx, |s| s.text_size = TextSize::default()));
    cx.bind_keys([
        KeyBinding::new("cmd-=", BiggerText, None),
        KeyBinding::new("cmd-+", BiggerText, None),
        KeyBinding::new("cmd--", SmallerText, None),
    ]);
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

const WINDOW_WIDTH: f32 = 600.;
const WINDOW_HEIGHT: f32 = 720.;
/// The toolbar of panes, under the window buttons.
const TOOLBAR_HEIGHT: f32 = 76.;

fn open_window(cx: &mut App) {
    if let Some(handle) = cx.global::<SettingsWindow>().0
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(Pane::ALL[0].title().into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px(12.))),
        }),
        is_resizable: false,
        is_minimizable: false,
        ..WindowOptions::default()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| SettingsView::new(window, cx))
    }) {
        Ok((handle, _)) => cx.set_global(SettingsWindow(Some(handle))),
        Err(error) => eprintln!("focal: could not open the settings: {error:#}"),
    }
}

/// The settings window's panes, in toolbar order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pane {
    Text,
    Writing,
    Focus,
    Diagrams,
    General,
}

impl Pane {
    pub const ALL: [Self; 5] = [
        Self::Text,
        Self::Writing,
        Self::Focus,
        Self::Diagrams,
        Self::General,
    ];

    pub const fn title(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Writing => "Writing",
            Self::Focus => "Focus",
            Self::Diagrams => "Diagrams",
            Self::General => "General",
        }
    }

    /// The toolbar's mark for the pane, drawn in the typefaces Focal has.
    const fn icon(self) -> Icon {
        match self {
            Self::Text => Icon::Type,
            Self::Writing => Icon::PenLine,
            Self::Focus => Icon::Focus,
            Self::Diagrams => Icon::Shapes,
            Self::General => Icon::Settings,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&pane| pane == self).unwrap_or(0)
    }
}

pub struct SettingsView {
    focus: FocusHandle,
    /// The list of installed typefaces "Other…" opens.
    picker: Option<(gpui_kit::Entity<Switcher>, gpui_kit::Subscription)>,
    pane: Pane,
    /// The diagram tools found when the window opened; looking them up
    /// reads the disk, so not on every frame.
    tools: Vec<crate::diagram::Tool>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Settings>(|_, cx| cx.notify()).detach();
        // The command line tool may change behind the window's back.
        cx.observe_window_activation(window, |_, _, cx| cx.notify())
            .detach();
        follow_appearance(window, cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            focus,
            picker: None,
            pane: Pane::Text,
            tools: crate::diagram::tools(),
        }
    }

    #[cfg(test)]
    pub const fn pane(&self) -> Pane {
        self.pane
    }

    fn choose_font(&mut self, action: &ChooseFont, window: &mut Window, cx: &mut Context<Self>) {
        let slot = action.0;
        let families = installed_families(cx, slot);
        let items = families
            .iter()
            .map(|family| PickItem {
                label: family.clone(),
                detail: String::new(),
                key: family.clone(),
                indent: 0,
            })
            .collect();
        let picker = cx.new(|cx| {
            Switcher::new(
                items,
                "Choose a typeface…",
                "No matching typefaces",
                window,
                cx,
            )
        });
        let subscription = cx.subscribe_in(&picker, window, move |this, _, event, window, cx| {
            if let SwitcherEvent::Pick(ix) = event
                && let Some(family) = families.get(*ix)
            {
                choose_installed(slot, family.clone(), cx);
            }
            this.picker = None;
            window.focus(&this.focus, cx);
            cx.notify();
        });
        self.picker = Some((picker, subscription));
        cx.notify();
    }

    fn show(&mut self, pane: Pane, window: &mut Window, cx: &mut Context<Self>) {
        if self.pane != pane {
            self.pane = pane;
            window.set_window_title(pane.title());
            cx.notify();
        }
    }

    fn step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = Pane::ALL.len().cast_signed();
        let next = (self.pane.index().cast_signed() + by).rem_euclid(count);
        self.show(Pane::ALL[next.cast_unsigned()], window, cx);
    }

    fn render_toolbar(&self, theme: &Colors, cx: &Context<Self>) -> impl IntoElement {
        let tabs = Pane::ALL.into_iter().map(|pane| {
            let selected = pane == self.pane;
            let hover = theme.code_background;
            div()
                .id(SharedString::from(format!("pane-{}", pane.title())))
                .test_support()
                .aria_label(pane.title())
                .w(px(68.))
                .py(px(5.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.))
                .rounded(px(7.))
                .cursor_pointer()
                .text_color(if selected { theme.caret } else { theme.marker })
                .when(selected, |d| d.bg(theme.code_background))
                .when(!selected, |d| d.hover(move |style| style.bg(hover)))
                .child(div().h(px(20.)).flex().items_center().child(
                    pane.icon().element(18.).text_color(if selected {
                        theme.caret
                    } else {
                        theme.marker
                    }),
                ))
                .child(
                    div()
                        .text_size(px(11.))
                        .when(selected, |d| d.text_color(theme.text))
                        .child(pane.title()),
                )
                .on_click(cx.listener(move |this, _, window, cx| this.show(pane, window, cx)))
        });
        div()
            .relative()
            .flex_none()
            .h(px(TOOLBAR_HEIGHT))
            .border_b_1()
            .border_color(theme.rule)
            // A handle to drag the window, behind the tabs.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(8.))
                    .flex()
                    .justify_center()
                    .gap(px(4.))
                    .children(tabs),
            )
    }

    fn render_pane(&self, settings: &Settings, theme: &Colors, cx: &App) -> AnyElement {
        match self.pane {
            Pane::Text => text_pane(settings, theme).into_any_element(),
            Pane::Writing => writing_pane(settings, theme).into_any_element(),
            Pane::Focus => focus_pane(settings, theme).into_any_element(),
            Pane::Diagrams => diagrams_pane(&self.tools, theme).into_any_element(),
            Pane::General => general_pane(settings, theme, cx).into_any_element(),
        }
    }
}

/// A titled group of rows on a card, as in System Settings.
fn group(
    title: &'static str,
    theme: &Colors,
    rows: impl IntoIterator<Item = AnyElement>,
) -> impl IntoElement {
    let rule = theme.rule;
    let rows = rows.into_iter().enumerate().map(move |(ix, row)| {
        div()
            .when(ix > 0, |d| d.border_t_1().border_color(rule))
            .child(row)
    });
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(
            div()
                .px(px(4.))
                .text_size(px(12.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.marker)
                .child(title),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .rounded(px(10.))
                .border_1()
                .border_color(theme.rule)
                .bg(theme.code_background)
                .children(rows),
        )
}

/// One setting: its name and an optional explanation on the left, its
/// control on the right.
fn row(
    label: impl IntoElement,
    hint: Option<SharedString>,
    control: impl IntoElement,
    theme: &Colors,
) -> AnyElement {
    div()
        .px(px(14.))
        .py(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_color(theme.text).child(label))
                .when_some(hint, |d, hint| {
                    d.child(
                        div()
                            .text_size(px(12.))
                            .text_color(theme.marker)
                            .child(hint),
                    )
                }),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// A segmented control choosing one of `options`, saved through `set`.
pub(crate) fn segmented<T: Copy + PartialEq + 'static>(
    id: &'static str,
    options: &'static [(T, &'static str)],
    current: T,
    set: fn(&mut Settings, T),
    theme: &Colors,
) -> impl IntoElement {
    let hover = theme.code_background;
    let segments = options.iter().map(move |&(value, label)| {
        let selected = value == current;
        div()
            .id(SharedString::from(format!("{id}-{label}")))
            .test_support()
            .aria_label(label)
            .px(px(10.))
            .py(px(3.))
            .rounded(px(5.))
            .cursor_pointer()
            .text_size(px(13.))
            .when(selected, |d| {
                d.bg(theme.background)
                    .text_color(theme.text)
                    .border_1()
                    .border_color(theme.rule)
            })
            .when(!selected, |d| {
                d.text_color(theme.marker)
                    .border_1()
                    .border_color(gpui_kit::transparent_black())
                    .hover(move |style| style.bg(hover))
            })
            .child(label)
            .on_click(move |_, _, cx| update(cx, |settings| set(settings, value)))
    });
    div()
        .id(id)
        .flex()
        .gap(px(2.))
        .p(px(2.))
        .rounded(px(7.))
        .bg(theme.code_background)
        .border_1()
        .border_color(theme.rule)
        .children(segments)
}

/// A switch saved through `set`.
pub(crate) fn toggle(
    id: &'static str,
    checked: bool,
    set: fn(&mut Settings, bool),
    theme: &Colors,
) -> Switch {
    Switch::new(id)
        .checked(checked)
        .color(theme.caret)
        .on_click(move |checked, _, cx| {
            let checked = *checked;
            update(cx, |settings| set(settings, checked));
        })
}

/// A small push button.
fn push_button(
    id: &'static str,
    label: &'static str,
    theme: &Colors,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let hover = theme.rule;
    div()
        .id(id)
        .test_support()
        .aria_label(label)
        .px(px(12.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.rule)
        .bg(theme.background)
        .text_size(px(13.))
        .text_color(theme.text)
        .cursor_pointer()
        .hover(move |style| style.bg(hover))
        .child(label)
        .on_click(move |_, window, cx| on_click(window, cx))
}

/// A few lines in the chosen typeface, size and colors.
fn preview(settings: &Settings, theme: &Colors) -> impl IntoElement {
    let typography = Typography::new(settings);
    let bold: SharedString = if typography.prose == PROSE_FONT {
        BOLD_PROSE_FONT.into()
    } else {
        typography.prose.clone()
    };
    let characters = match settings.column_width {
        ColumnWidth::Narrow => "about 60",
        ColumnWidth::Medium => "about 70",
        ColumnWidth::Wide => "about 85",
    };
    div()
        .id("settings-preview")
        .test_support()
        .px(px(20.))
        .py(px(16.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.rule)
        .bg(theme.background)
        .flex()
        .flex_col()
        .gap(px(6.))
        .text_color(theme.text)
        .font_family(typography.prose)
        .text_size(px(typography.size))
        .child(
            div()
                .font_family(bold)
                .font_weight(FontWeight::BOLD)
                .text_size(px(typography.size * 1.25))
                .child("A calm page"),
        )
        .child("Words look the way they will read, and the page stays quiet.")
        .child(
            div()
                .font_family(MONO_FONT)
                .text_size(px(typography.size * 0.9))
                .text_color(theme.code_text)
                .child("let code = \"always in Mono\";"),
        )
        .child(
            div()
                .pt(px(4.))
                .font_family(PROSE_FONT)
                .text_size(px(11.))
                .text_color(theme.marker)
                .child(format!(
                    "{} pt · {characters} characters per line",
                    typography.size
                )),
        )
}

/// A typeface tile: a sample in the face, and its name below.
#[allow(clippy::too_many_arguments)]
fn font_tile(
    id: String,
    name: SharedString,
    sample: &'static str,
    family: Option<SharedString>,
    selected: bool,
    tile: f32,
    theme: &Colors,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let hover = theme.code_background;
    div()
        .id(SharedString::from(id))
        .test_support()
        .aria_label(name.clone())
        .w(px(tile))
        .h(px(tile * 0.72))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(2.))
        .rounded(px(9.))
        .cursor_pointer()
        .bg(theme.surface)
        .map(|d| {
            if selected {
                d.border_2().border_color(theme.caret)
            } else {
                d.border_1()
                    .border_color(theme.rule)
                    .hover(move |style| style.bg(hover))
            }
        })
        .child(match family {
            Some(family) => div()
                .font_family(family)
                .text_size(px(tile * 0.24))
                .text_color(theme.text)
                .child(sample)
                .into_any_element(),
            None => Icon::Search
                .element(tile * 0.22)
                .text_color(theme.marker)
                .into_any_element(),
        })
        .child(
            div()
                .max_w(px(tile - 8.))
                .overflow_hidden()
                .text_ellipsis()
                .text_size(px(10.5))
                .text_color(if selected { theme.text } else { theme.marker })
                .whitespace_nowrap()
                .child(name),
        )
        .on_click(move |_, window, cx| on_click(window, cx))
}

/// "Other…": any installed family, chosen from a list.
fn other_tile(slot: FontSlot, prefix: &str, tile: f32, theme: &Colors) -> impl IntoElement {
    font_tile(
        format!("{prefix}-Other"),
        "Other…".into(),
        "",
        None,
        false,
        tile,
        theme,
        move |window, cx| window.dispatch_action(Box::new(ChooseFont(slot)), cx),
    )
}

/// The typefaces for prose as tiles, each showing "Aa" in its own face;
/// clicking one chooses it.
pub(crate) fn typeface_grid(settings: &Settings, theme: &Colors, tile: f32) -> impl IntoElement {
    let mut fonts = ProseFont::ALL.to_vec();
    if matches!(settings.prose_font, ProseFont::Custom(_)) {
        fonts.push(settings.prose_font.clone());
    }
    let tiles = fonts.into_iter().map(|font| {
        let chosen = font.clone();
        font_tile(
            format!("prose-font-{}", font.name()),
            SharedString::new(font.name()),
            "Aa",
            Some(font.family()),
            font == settings.prose_font,
            tile,
            theme,
            move |_, cx| {
                let chosen = chosen.clone();
                update(cx, |settings| settings.prose_font = chosen);
            },
        )
    });
    div()
        .flex()
        .flex_wrap()
        .gap(px(8.))
        .children(tiles)
        .child(other_tile(FontSlot::Prose, "prose-font", tile, theme))
}

/// The typefaces for code as tiles, each showing braces in its own face.
pub(crate) fn code_font_grid(settings: &Settings, theme: &Colors, tile: f32) -> impl IntoElement {
    let mut fonts = CodeFont::ALL.to_vec();
    if matches!(settings.code_font, CodeFont::Custom(_)) {
        fonts.push(settings.code_font.clone());
    }
    let tiles = fonts.into_iter().map(|font| {
        let chosen = font.clone();
        font_tile(
            format!("code-font-{}", font.name()),
            SharedString::new(font.name()),
            "{0}",
            Some(font.family()),
            font == settings.code_font,
            tile,
            theme,
            move |_, cx| {
                let chosen = chosen.clone();
                update(cx, |settings| settings.code_font = chosen);
            },
        )
    });
    div()
        .flex()
        .flex_wrap()
        .gap(px(8.))
        .children(tiles)
        .child(other_tile(FontSlot::Code, "code-font", tile, theme))
}

fn text_pane(settings: &Settings, theme: &Colors) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(preview(settings, theme))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .px(px(4.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.marker)
                        .child("Typeface"),
                )
                .child(typeface_grid(settings, theme, 100.)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .px(px(4.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.marker)
                        .child("Code"),
                )
                .child(code_font_grid(settings, theme, 100.)),
        )
        .child(group(
            "Text",
            theme,
            [
                row(
                    "Appearance",
                    Some("Light or dark for Focal alone, or as your Mac is set.".into()),
                    segmented(
                        "appearance",
                        &[
                            (Appearance::System, "System"),
                            (Appearance::Light, "Light"),
                            (Appearance::Dark, "Dark"),
                        ],
                        settings.appearance,
                        |s, v| s.appearance = v,
                        theme,
                    ),
                    theme,
                ),
                row(
                    "Size",
                    None,
                    segmented(
                        "text-size",
                        &[
                            (TextSize::Small, "S"),
                            (TextSize::Medium, "M"),
                            (TextSize::Large, "L"),
                            (TextSize::Huge, "XL"),
                        ],
                        settings.text_size,
                        |s, v| s.text_size = v,
                        theme,
                    ),
                    theme,
                ),
                row(
                    "Line length",
                    None,
                    segmented(
                        "column-width",
                        &[
                            (ColumnWidth::Narrow, "Narrow"),
                            (ColumnWidth::Medium, "Medium"),
                            (ColumnWidth::Wide, "Wide"),
                        ],
                        settings.column_width,
                        |s, v| s.column_width = v,
                        theme,
                    ),
                    theme,
                ),
            ],
        ))
}

/// What the chosen editing keys are like.
pub(crate) const fn keyboard_hint(keyboard: Keyboard) -> &'static str {
    match keyboard {
        Keyboard::Standard => "The keys of every Mac app. Vim and Helix add modal editing.",
        Keyboard::Vim => {
            "Vim's normal, insert and visual modes: operators, motions, text objects and \
             :w. Esc returns to normal mode."
        }
        Keyboard::Helix => {
            "Helix's selection first: w selects a word, x a line, then d, c or y act \
             on it. v extends; Esc returns to normal mode."
        }
    }
}

fn writing_pane(settings: &Settings, theme: &Colors) -> impl IntoElement {
    let correction_hint: SharedString = if crate::spell::system_corrects_spelling() {
        "Fixes a typo once you finish the word, as its own undo step.".into()
    } else {
        "Turned off for every app in System Settings › Keyboard › Text Input.".into()
    };
    div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(group(
            "Keyboard",
            theme,
            [row(
                "Editing keys",
                Some(keyboard_hint(settings.keyboard).into()),
                segmented(
                    "keyboard",
                    &[
                        (Keyboard::Standard, "Standard"),
                        (Keyboard::Vim, "Vim"),
                        (Keyboard::Helix, "Helix"),
                    ],
                    settings.keyboard,
                    |s, v| s.keyboard = v,
                    theme,
                ),
                theme,
            )],
        ))
        .child(group(
            "Spelling and grammar",
            theme,
            [
                row(
                    "Check grammar as you type",
                    Some("Underlines what Apple's grammar checker would change, in green.".into()),
                    toggle(
                        "check-grammar",
                        settings.check_grammar,
                        |s, v| {
                            s.check_grammar = v;
                        },
                        theme,
                    ),
                    theme,
                ),
                row(
                    "Correct spelling automatically",
                    Some(correction_hint),
                    toggle(
                        "correct-spelling",
                        settings.correct_spelling,
                        |s, v| {
                            s.correct_spelling = v;
                        },
                        theme,
                    ),
                    theme,
                ),
            ],
        ))
}

fn focus_pane(settings: &Settings, theme: &Colors) -> impl IntoElement {
    div().flex().flex_col().gap(px(18.)).child(group(
        "Focus mode",
        theme,
        [
            row(
                "Keep bright",
                Some("Everything else fades while you write. ⌘D turns focus mode on.".into()),
                segmented(
                    "focus-unit",
                    &[
                        (FocusUnit::Sentence, "Sentence"),
                        (FocusUnit::Paragraph, "Paragraph"),
                    ],
                    settings.focus_unit,
                    |s, v| s.focus_unit = v,
                    theme,
                ),
                theme,
            ),
            row(
                "Typewriter scrolling",
                Some("Keeps the line you write on in the middle of the window.".into()),
                toggle(
                    "typewriter",
                    settings.typewriter,
                    |s, v| s.typewriter = v,
                    theme,
                ),
                theme,
            ),
        ],
    ))
}

fn diagrams_pane(tools: &[crate::diagram::Tool], theme: &Colors) -> impl IntoElement {
    let rows = tools.iter().map(|tool| {
        let status = div()
            .id(SharedString::from(format!("diagram-tool-{}", tool.command)))
            .px(px(8.))
            .py(px(2.))
            .rounded(px(10.))
            .text_size(px(12.))
            .map(|d| {
                if tool.installed {
                    d.bg(theme.alert(focal_core::analysis::Alert::Tip).opacity(0.15))
                        .text_color(theme.alert(focal_core::analysis::Alert::Tip))
                        .child("Installed")
                } else {
                    d.font_family(MONO_FONT)
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.rule)
                        .text_color(theme.marker)
                        .child(tool.install)
                }
            });
        row(
            div().font_family(MONO_FONT).child(tool.command),
            Some(tool.draws.into()),
            status,
            theme,
        )
    });
    div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(group("Optional diagram tools", theme, rows))
        .child(
            div()
                .px(px(4.))
                .text_size(px(12.))
                .text_color(theme.marker)
                .child(
                    "Mermaid, Graphviz, Vega, maps and the rest are drawn by Focal itself. \
                     These tools add what it cannot draw alone; restart Focal after installing one.",
                ),
        )
}

fn general_pane(settings: &Settings, theme: &Colors, cx: &App) -> impl IntoElement {
    let (updates_hint, check_label): (SharedString, _) = if crate::updates::available(cx) {
        (
            "Focal asks before it downloads anything.".into(),
            "Check Now",
        )
    } else {
        (
            "This build of Focal does not update itself.".into(),
            "Releases…",
        )
    };
    let (command_hint, command_button): (SharedString, AnyElement) =
        match crate::cli_install::command_path() {
            Some(path) if !crate::cli_install::is_own_link(&path) => (
                format!("Installed at {}, by Homebrew or by hand.", path.display()).into(),
                div().into_any_element(),
            ),
            Some(path) => (
                format!("Installed at {}.", path.display()).into(),
                push_button("uninstall-command", "Uninstall", theme, |window, cx| {
                    window.dispatch_action(Box::new(crate::cli_install::UninstallCommand), cx);
                })
                .into_any_element(),
            ),
            None if crate::cli_install::bundled_binary().is_some() => (
                "Type focal notes.md or focal . in a terminal.".into(),
                push_button("install-command", "Install", theme, |window, cx| {
                    window.dispatch_action(Box::new(crate::cli_install::InstallCommand), cx);
                })
                .into_any_element(),
            ),
            None => (
                "Only Focal.app can install the command.".into(),
                div().into_any_element(),
            ),
        };
    div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .child(group(
            "Updates",
            theme,
            [row(
                "Check for updates automatically",
                Some(updates_hint),
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(push_button(
                        "check-now",
                        check_label,
                        theme,
                        |window, cx| {
                            let action: Box<dyn gpui_kit::Action> = if crate::updates::available(cx)
                            {
                                Box::new(crate::updates::CheckForUpdates)
                            } else {
                                Box::new(crate::updates::OpenReleases)
                            };
                            window.dispatch_action(action, cx);
                        },
                    ))
                    .child(toggle(
                        "check-updates",
                        settings.check_updates,
                        |s, v| {
                            s.check_updates = v;
                        },
                        theme,
                    )),
                theme,
            )],
        ))
        .child(group(
            "Command line",
            theme,
            [row(
                "The focal command",
                Some(command_hint),
                command_button,
                theme,
            )],
        ))
        .child(
            div()
                .px(px(4.))
                .text_size(px(12.))
                .text_color(theme.marker)
                .child(format!(
                    "Focal {} · Settings are kept in ~/Library/Application Support/Focal.",
                    env!("CARGO_PKG_VERSION")
                )),
        )
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Colors::for_appearance(window.appearance());
        let settings = cx.global::<Settings>().clone();
        div()
            .id("settings")
            .key_context(SETTINGS_CONTEXT)
            .track_focus(&self.focus)
            .on_action(|_: &CloseSettings, window, _| window.remove_window())
            .on_action(cx.listener(|this, action: &ShowPane, window, cx| {
                if let Some(&pane) = Pane::ALL.get(action.0) {
                    this.show(pane, window, cx);
                }
            }))
            .on_action(cx.listener(Self::choose_font))
            .on_action(cx.listener(|this, _: &NextPane, window, cx| this.step(1, window, cx)))
            .on_action(cx.listener(|this, _: &PreviousPane, window, cx| this.step(-1, window, cx)))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(PROSE_FONT)
            .text_size(px(14.))
            .child(self.render_toolbar(&theme, cx))
            .child(
                div()
                    .id("settings-pane")
                    .flex_1()
                    .overflow_y_scroll()
                    .px(px(28.))
                    .py(px(22.))
                    .child(self.render_pane(&settings, &theme, cx)),
            )
            .when_some(self.picker.as_ref(), |d, (picker, _)| {
                d.child(
                    div()
                        .absolute()
                        .top(px(TOOLBAR_HEIGHT + 12.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(picker.clone()),
                )
            })
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
            appearance: Appearance::Dark,
            focus_unit: FocusUnit::Sentence,
            typewriter: false,
            prose_font: ProseFont::Custom("MonoLisa".to_owned()),
            code_font: CodeFont::JetBrainsMono,
            text_size: TextSize::Large,
            column_width: ColumnWidth::Wide,
            check_updates: false,
            check_grammar: false,
            correct_spelling: false,
            keyboard: Keyboard::Helix,
        };
        settings.save_to(&path).unwrap();
        assert_eq!(Settings::load_from(&path), settings);
    }

    #[test]
    fn typefaces_read_by_name_or_as_an_installed_family() {
        let path = temp("typefaces");
        std::fs::write(
            &path,
            r#"{"prose_font": "charter", "code_font": {"custom": "MonoLisa"}}"#,
        )
        .unwrap();
        let settings = Settings::load_from(&path);
        assert_eq!(settings.prose_font, ProseFont::Charter);
        assert_eq!(settings.code_font, CodeFont::Custom("MonoLisa".to_owned()));
        assert_eq!(settings.code_font.family().as_ref(), "MonoLisa");
        assert_eq!(
            CodeFont::default().family().as_ref(),
            crate::theme::MONO_FONT
        );
    }

    #[test]
    fn text_size_steps_stop_at_the_ends() {
        assert_eq!(TextSize::Medium.step(true), TextSize::Large);
        assert_eq!(TextSize::Huge.step(true), TextSize::Huge);
        assert_eq!(TextSize::Small.step(false), TextSize::Small);
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
