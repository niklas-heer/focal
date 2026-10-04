//! The About window: the icon, the version, where to find Focal, and what it
//! is made with.

use std::sync::Arc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Context, FocusHandle, FontWeight, Global, Image,
    ImageFormat, InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    TitlebarOptions, Window, WindowBounds, WindowControlArea, WindowOptions, actions, div, img,
    point, px, size,
};

use crate::theme::{BOLD_PROSE_FONT, MONO_FONT, PROSE_FONT, Theme};

actions!(focal, [CloseAbout]);

const CONTEXT: &str = "FocalAbout";
const ICON: &[u8] = include_bytes!("../../../docs/images/icon.png");
const REPOSITORY: &str = "https://github.com/niklas-heer/focal";

/// The open About window, so a second "About Focal" brings it forward.
struct AboutWindow(Option<AnyWindowHandle>);

impl Global for AboutWindow {}

pub fn init(cx: &mut App) {
    cx.set_global(AboutWindow(None));
    cx.bind_keys([
        KeyBinding::new("cmd-w", CloseAbout, Some(CONTEXT)),
        KeyBinding::new("escape", CloseAbout, Some(CONTEXT)),
    ]);
}

pub fn open_window(cx: &mut App) {
    if let Some(handle) = cx.global::<AboutWindow>().0
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let bounds = Bounds::centered(None, size(px(380.), px(540.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("About Focal".into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.), px(12.))),
        }),
        is_resizable: false,
        is_minimizable: false,
        ..WindowOptions::default()
    };
    match gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| AboutView::new(window, cx))
    }) {
        Ok((handle, _)) => cx.set_global(AboutWindow(Some(handle))),
        Err(error) => eprintln!("focal: could not open the About window: {error:#}"),
    }
}

/// "Version 0.4.0", with the bundle's build number when there is one.
pub fn version_line() -> String {
    let version = env!("CARGO_PKG_VERSION");
    match crate::mac::bundle_build() {
        Some(build) => format!("Version {version} ({build})"),
        None => format!("Version {version}"),
    }
}

pub struct AboutView {
    focus: FocusHandle,
    icon: Arc<Image>,
}

impl AboutView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::settings::follow_appearance(window, cx);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        Self {
            focus,
            icon: Arc::new(Image::from_bytes(ImageFormat::Png, ICON.to_vec())),
        }
    }
}

/// A link drawn as quiet text that underlines under the pointer.
fn link(id: &'static str, label: &'static str, url: String, theme: &Theme) -> impl IntoElement {
    div()
        .id(id)
        .test_support()
        .aria_label(label)
        .px(px(8.))
        .py(px(3.))
        .rounded(px(6.))
        .cursor_pointer()
        .text_color(theme.link)
        .hover(|style| style.underline())
        .child(label)
        .on_click(move |_, _, cx| cx.open_url(&url))
}

/// A line of credits.
fn credit(text: impl Into<SharedString>) -> impl IntoElement {
    div().text_center().child(text.into())
}

/// Where to read about Focal, its changes, and where to report a problem.
fn links(theme: &Theme) -> impl IntoElement {
    let version = env!("CARGO_PKG_VERSION");
    div()
        .pt(px(14.))
        .flex()
        .gap(px(4.))
        .child(link(
            "about-website",
            "Website",
            REPOSITORY.to_owned(),
            theme,
        ))
        .child(link(
            "about-notes",
            "Release Notes",
            format!("{REPOSITORY}/releases/tag/v{version}"),
            theme,
        ))
        .child(link(
            "about-issue",
            "Report an Issue",
            format!("{REPOSITORY}/issues/new"),
            theme,
        ))
}

/// Who made Focal and what it is made with.
fn credits(theme: &Theme) -> impl IntoElement {
    div()
        .w_full()
        .px(px(28.))
        .pt(px(14.))
        .pb(px(20.))
        .border_t_1()
        .border_color(theme.rule)
        .flex()
        .flex_col()
        .items_center()
        .gap(px(4.))
        .text_size(px(11.))
        .text_color(theme.marker)
        .child(credit("Made by Niklas Heer, in Rust with GPUI."))
        .child(credit(
            "Set in iA Writer Quattro, Duo and Mono by Information Architects, \
             under the SIL Open Font License 1.1.",
        ))
        .child(credit(
            "Markdown by pulldown-cmark, math by MathJax, diagrams by merman.",
        ))
        .child(div().pt(px(6.)).child("© 2026 Niklas Heer · MIT License"))
}

impl Render for AboutView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let hover = theme.rule;
        let updates = crate::updates::available(cx);
        div()
            .id("about")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(|_: &CloseAbout, window, _| window.remove_window())
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(PROSE_FONT)
            .text_size(px(13.))
            // A handle to drag the window, under the window buttons.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(40.))
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(
                img(self.icon.clone())
                    .mt(px(44.))
                    .size(px(112.))
                    .flex_none(),
            )
            .child(
                div()
                    .id("about-name")
                    .test_support()
                    .pt(px(12.))
                    .font_family(BOLD_PROSE_FONT)
                    .font_weight(FontWeight::BOLD)
                    .text_size(px(30.))
                    .child("Focal"),
            )
            .child(
                div()
                    .id("about-version")
                    .test_support()
                    .font_family(MONO_FONT)
                    .text_size(px(12.))
                    .text_color(theme.marker)
                    .child(version_line()),
            )
            .child(
                div()
                    .pt(px(16.))
                    .px(px(36.))
                    .text_center()
                    .text_size(px(15.))
                    .child("A calm, native Markdown editor for the Mac."),
            )
            .child(links(&theme))
            // Builds that cannot update themselves have Release Notes.
            .when(updates, |d| {
                d.child(
                    div()
                        .id("about-updates")
                        .test_support()
                        .mt(px(14.))
                        .px(px(14.))
                        .py(px(5.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(theme.rule)
                        .cursor_pointer()
                        .hover(move |style| style.bg(hover))
                        .child("Check for Updates…")
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(crate::updates::CheckForUpdates), cx);
                        }),
                )
            })
            .child(div().flex_1())
            .child(credits(&theme))
    }
}
