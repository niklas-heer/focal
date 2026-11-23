//! The image viewer: one of the document's images over the whole window, to
//! look at closely. A pinch, ⌘-scrolling or ⌘+ and ⌘− zoom around the
//! pointer; dragging or scrolling pans; a double click switches between
//! fitting the window and a closer look. The arrow keys step through the
//! document's images, Esc closes, and "Open in Preview" hands the file on.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, Context, CursorStyle, EventEmitter, FocusHandle, Focusable, InteractiveElement as _,
    IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement as _, PinchEvent, Pixels, Point, Render, Role, ScrollEvent, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _, TestSupportExt as _, Window,
    actions, div, img, point, px,
};

use gpui_kit::WindowControlArea;

use crate::bar::{button, separator};
use crate::icons::Icon;
use crate::theme::{MONO_FONT, PROSE_FONT, Theme};

actions!(
    focal,
    [
        CloseViewer,
        ViewerZoomIn,
        ViewerZoomOut,
        ViewerFit,
        ViewerActualSize,
        NextImage,
        PreviousImage,
        OpenInPreview
    ]
);

pub const CONTEXT: &str = "FocalImageViewer";

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("escape", CloseViewer, context),
        KeyBinding::new("space", CloseViewer, context),
        KeyBinding::new("cmd-y", CloseViewer, context),
        KeyBinding::new("cmd-w", CloseViewer, context),
        KeyBinding::new("cmd-=", ViewerZoomIn, context),
        KeyBinding::new("cmd-+", ViewerZoomIn, context),
        KeyBinding::new("=", ViewerZoomIn, context),
        KeyBinding::new("+", ViewerZoomIn, context),
        KeyBinding::new("cmd--", ViewerZoomOut, context),
        KeyBinding::new("-", ViewerZoomOut, context),
        KeyBinding::new("cmd-0", ViewerFit, context),
        KeyBinding::new("0", ViewerFit, context),
        KeyBinding::new("cmd-1", ViewerActualSize, context),
        KeyBinding::new("1", ViewerActualSize, context),
        KeyBinding::new("right", NextImage, context),
        KeyBinding::new("down", NextImage, context),
        KeyBinding::new("left", PreviousImage, context),
        KeyBinding::new("up", PreviousImage, context),
    ]);
}

/// Room kept free around a fitted image: for the title above and the
/// controls below.
const MARGIN_X: f32 = 48.;
const MARGIN_TOP: f32 = 56.;
const MARGIN_BOTTOM: f32 = 84.;
/// The closest look: sixteen times the actual size.
const MAX_SCALE: f32 = 16.;
/// One step of ⌘+ or ⌘−.
const STEP: f32 = 1.25;

/// An image to show, with what to call it.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewedImage {
    pub path: PathBuf,
    pub title: String,
}

pub enum ViewerEvent {
    Dismiss,
}

/// A drag that pans, or a press on the backdrop that closes once released
/// without moving.
struct Drag {
    start: Point<Pixels>,
    offset: Point<Pixels>,
    on_image: bool,
    moved: bool,
}

pub struct ImageViewer {
    images: Vec<ViewedImage>,
    index: usize,
    /// The image's size in points, as it shows at 100% (a Retina image's
    /// pixels halved, as in Preview).
    natural: Option<(f32, f32)>,
    /// Screen points per image point: 1 is the actual size.
    scale: f32,
    /// Whether the scale follows the window's size, as it does until zoomed.
    fitted: bool,
    /// How far the image's center is from the window's.
    offset: Point<Pixels>,
    drag: Option<Drag>,
    /// The window's size in the last frame.
    viewport: (f32, f32),
    focus_handle: FocusHandle,
}

impl EventEmitter<ViewerEvent> for ImageViewer {}

impl Focusable for ImageViewer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// The scale at which a `width` × `height` image fits in the window, room
/// for the title and controls left free.
fn fit_scale((width, height): (f32, f32), (view_width, view_height): (f32, f32)) -> f32 {
    let room_width = (view_width - 2. * MARGIN_X).max(1.);
    let room_height = (view_height - MARGIN_TOP - MARGIN_BOTTOM).max(1.);
    (room_width / width.max(1.)).min(room_height / height.max(1.))
}

/// The scale an image opens at: fitted to the window, never past its
/// natural size, where it is sharpest.
fn resting_scale(natural: (f32, f32), viewport: (f32, f32)) -> f32 {
    fit_scale(natural, viewport).min(1.)
}

/// The offset that keeps the image point under `anchor` (taken from the
/// window's center) in place while the scale goes from `old` to `new`.
fn zoom_offset(offset: Point<Pixels>, anchor: Point<Pixels>, old: f32, new: f32) -> Point<Pixels> {
    let ratio = new / old;
    point(
        anchor.x - (anchor.x - offset.x) * ratio,
        anchor.y - (anchor.y - offset.y) * ratio,
    )
}

/// Keeps a panned image over the window: centered along an axis it fits in,
/// otherwise with no more than a margin of backdrop past an edge.
fn clamp_offset(offset: f32, shown: f32, view: f32, margin: f32) -> f32 {
    if shown <= view {
        return 0.;
    }
    let limit = (shown - view) / 2. + margin;
    offset.clamp(-limit, limit)
}

impl ImageViewer {
    pub fn new(
        images: Vec<ViewedImage>,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let size = window.viewport_size();
        let mut viewer = Self {
            images,
            index: 0,
            natural: None,
            scale: 1.,
            fitted: true,
            offset: point(px(0.), px(0.)),
            drag: None,
            viewport: (f32::from(size.width), f32::from(size.height)),
            focus_handle: cx.focus_handle(),
        };
        viewer.show(index);
        viewer
    }

    /// Shows the image at `index`, fitted.
    fn show(&mut self, index: usize) {
        self.index = index.min(self.images.len().saturating_sub(1));
        self.natural = self
            .images
            .get(self.index)
            .and_then(|image| size_in_points(&image.path));
        self.fit();
    }

    fn fit(&mut self) {
        self.fitted = true;
        self.offset = point(px(0.), px(0.));
        if let Some(natural) = self.natural {
            self.scale = resting_scale(natural, self.viewport);
        }
    }

    /// The image being shown.
    #[cfg(test)]
    pub fn current(&self) -> Option<&ViewedImage> {
        self.images.get(self.index)
    }

    /// The zoom, as screen points per image pixel.
    #[cfg(test)]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// Zooms to `scale` around `anchor`, a window position.
    fn zoom_to(&mut self, scale: f32, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(natural) = self.natural else {
            return;
        };
        let least = resting_scale(natural, self.viewport) * 0.5;
        let scale = scale.clamp(least.min(1.), MAX_SCALE.max(least));
        let center = point(px(self.viewport.0 / 2.), px(self.viewport.1 / 2.));
        let from_center = point(anchor.x - center.x, anchor.y - center.y);
        self.offset = zoom_offset(self.offset, from_center, self.scale, scale);
        self.scale = scale;
        self.fitted = false;
        self.clamp();
        cx.notify();
    }

    /// Zooms by `factor` around the window's center.
    fn zoom_by(&mut self, factor: f32, cx: &mut Context<Self>) {
        let center = point(px(self.viewport.0 / 2.), px(self.viewport.1 / 2.));
        self.zoom_to(self.scale * factor, center, cx);
    }

    fn clamp(&mut self) {
        let Some((width, height)) = self.natural else {
            return;
        };
        let (view_width, view_height) = self.viewport;
        self.offset = point(
            px(clamp_offset(
                f32::from(self.offset.x),
                width * self.scale,
                view_width,
                MARGIN_X,
            )),
            px(clamp_offset(
                f32::from(self.offset.y),
                height * self.scale,
                view_height,
                MARGIN_BOTTOM,
            )),
        );
    }

    /// Shows the next image, or the previous one, going round.
    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = self.images.len();
        if count < 2 {
            return;
        }
        let next = if forward {
            (self.index + 1) % count
        } else {
            (self.index + count - 1) % count
        };
        self.show(next);
        cx.notify();
    }

    fn on_scroll(&mut self, event: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(px(16.));
        if event.modifiers.platform || event.modifiers.control {
            let factor = (f32::from(delta.y) * 0.01).exp();
            self.zoom_to(self.scale * factor, event.position, cx);
        } else {
            self.offset = point(self.offset.x + delta.x, self.offset.y + delta.y);
            self.clamp();
            cx.notify();
        }
    }

    fn on_pinch(&mut self, event: &PinchEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.zoom_to(self.scale * (1. + event.delta), event.position, cx);
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let on_image = self
            .image_bounds()
            .is_some_and(|bounds| bounds.contains(&event.position));
        if event.click_count == 2 && on_image {
            self.drag = None;
            self.toggle_zoom(event.position, cx);
            return;
        }
        self.drag = Some(Drag {
            start: event.position,
            offset: self.offset,
            on_image,
            moved: false,
        });
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        let moved = point(
            event.position.x - drag.start.x,
            event.position.y - drag.start.y,
        );
        if !drag.moved && f32::from(moved.x).hypot(f32::from(moved.y)) < 3. {
            return;
        }
        drag.moved = true;
        self.offset = point(drag.offset.x + moved.x, drag.offset.y + moved.y);
        self.clamp();
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(drag) = self.drag.take()
            && !drag.moved
            && !drag.on_image
        {
            cx.emit(ViewerEvent::Dismiss);
        }
    }

    /// Between the resting size and a closer look at `anchor`: actual size
    /// for an image shrunk to fit, twice that for one shown whole already.
    fn toggle_zoom(&mut self, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(natural) = self.natural else {
            return;
        };
        let resting = resting_scale(natural, self.viewport);
        if (self.scale - resting).abs() < 0.01 {
            let closer = if resting < 0.99 { 1. } else { 2. * resting };
            self.zoom_to(closer, anchor, cx);
        } else {
            self.fit();
            cx.notify();
        }
    }

    /// Where the image was drawn, in window coordinates.
    fn image_bounds(&self) -> Option<gpui_kit::Bounds<Pixels>> {
        let (width, height) = self.natural?;
        let (shown_width, shown_height) = (width * self.scale, height * self.scale);
        let left = (self.viewport.0 - shown_width) / 2. + f32::from(self.offset.x);
        let top = (self.viewport.1 - shown_height) / 2. + f32::from(self.offset.y);
        Some(gpui_kit::Bounds::new(
            point(px(left), px(top)),
            gpui_kit::size(px(shown_width), px(shown_height)),
        ))
    }

    fn open_in_preview(&mut self, _: &OpenInPreview, _: &mut Window, _: &mut Context<Self>) {
        if let Some(image) = self.images.get(self.index) {
            open_in_preview(image.path.clone());
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render_controls(&self, theme: &Theme, cx: &Context<Self>) -> impl IntoElement {
        let percent = format!("{:.0}%", self.scale * 100.);
        let several = self.images.len() > 1;
        let action =
            |id: &'static str, icon: Icon, tip: &'static str, act: Box<dyn gpui_kit::Action>| {
                let dispatch = act.boxed_clone();
                button(
                    id,
                    icon.element(16.).text_color(theme.text),
                    tip,
                    theme,
                    Some(act),
                    move |_, window, cx| window.dispatch_action(dispatch.boxed_clone(), cx),
                )
            };
        let _ = cx;
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(crate::bar::BAR_MARGIN))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("viewer-controls")
                    .test_support()
                    .occlude()
                    .h(px(crate::bar::BAR_HEIGHT))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(1.))
                    .rounded(px(12.))
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.rule)
                    .shadow_lg()
                    .text_size(px(12.))
                    .text_color(theme.marker)
                    .when(several, |d| {
                        d.child(action(
                            "viewer-previous",
                            Icon::ChevronLeft,
                            "Previous image",
                            Box::new(PreviousImage),
                        ))
                        .child(
                            div()
                                .id("viewer-position")
                                .test_support()
                                .px(px(4.))
                                .font_family(MONO_FONT)
                                .child(format!("{} of {}", self.index + 1, self.images.len())),
                        )
                        .child(action(
                            "viewer-next",
                            Icon::ChevronRight,
                            "Next image",
                            Box::new(NextImage),
                        ))
                        .child(separator(theme))
                    })
                    .child(action(
                        "viewer-zoom-out",
                        Icon::ZoomOut,
                        "Zoom out",
                        Box::new(ViewerZoomOut),
                    ))
                    .child(button(
                        "viewer-actual-size",
                        div()
                            .min_w(px(40.))
                            .flex()
                            .justify_center()
                            .font_family(MONO_FONT)
                            .text_color(theme.text)
                            .child(percent),
                        "Actual size",
                        theme,
                        Some(Box::new(ViewerActualSize)),
                        |_, window, cx| window.dispatch_action(Box::new(ViewerActualSize), cx),
                    ))
                    .child(action(
                        "viewer-zoom-in",
                        Icon::ZoomIn,
                        "Zoom in",
                        Box::new(ViewerZoomIn),
                    ))
                    .child(action(
                        "viewer-fit",
                        Icon::Scan,
                        "Fit to window",
                        Box::new(ViewerFit),
                    ))
                    .child(separator(theme))
                    .child(action(
                        "viewer-preview",
                        Icon::ExternalLink,
                        "Open in Preview",
                        Box::new(OpenInPreview),
                    ))
                    .child(action(
                        "viewer-close",
                        Icon::X,
                        "Close",
                        Box::new(CloseViewer),
                    )),
            )
    }
}

/// Opens `path` in Preview, or in its default app when Preview declines it.
pub fn open_in_preview(path: PathBuf) {
    std::thread::spawn(move || {
        let opened = |args: &[&std::ffi::OsStr]| {
            std::process::Command::new("open")
                .args(args)
                .status()
                .is_ok_and(|status| status.success())
        };
        let file = path.as_os_str();
        if !opened(&["-a".as_ref(), "Preview".as_ref(), file]) {
            opened(&[file]);
        }
    });
}

/// An image's size in screen points: its pixels, halved for a Retina
/// image (see [`focal_core::density`]).
pub fn size_in_points(path: &Path) -> Option<(f32, f32)> {
    use std::io::Read as _;
    let mut header = Vec::with_capacity(64 * 1024);
    std::fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut header)
        .ok()?;
    let size = imagesize::blob_size(&header).ok()?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let density = focal_core::density::density(&name, &header);
    Some((size.width as f32 / density, size.height as f32 / density))
}

/// What to call an image: its alt text, otherwise its file's name.
pub fn title(alt: &str, path: &Path) -> String {
    if alt.trim().is_empty() {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        alt.trim().to_owned()
    }
}

impl Render for ImageViewer {
    #[allow(clippy::too_many_lines)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let size = window.viewport_size();
        let viewport = (f32::from(size.width), f32::from(size.height));
        if viewport != self.viewport {
            self.viewport = viewport;
            if self.fitted {
                self.fit();
            } else {
                self.clamp();
            }
        }
        let image = self.images.get(self.index).cloned();
        let title: SharedString = image
            .as_ref()
            .map(|image| image.title.clone())
            .unwrap_or_default()
            .into();
        let bounds = self.image_bounds();
        let grabbing = self.drag.as_ref().is_some_and(|drag| drag.moved);
        let pannable = bounds.is_some_and(|b| {
            f32::from(b.size.width) > viewport.0 || f32::from(b.size.height) > viewport.1
        });
        div()
            .id("image-viewer")
            .test_support()
            .role(Role::Dialog)
            .aria_label(format!("Image: {title}"))
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .occlude()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(theme.background)
            .when(pannable, |d| {
                d.cursor(if grabbing {
                    CursorStyle::ClosedHand
                } else {
                    CursorStyle::OpenHand
                })
            })
            .on_action(cx.listener(|_, _: &CloseViewer, _, cx| cx.emit(ViewerEvent::Dismiss)))
            .on_action(cx.listener(|this, _: &ViewerZoomIn, _, cx| this.zoom_by(STEP, cx)))
            .on_action(cx.listener(|this, _: &ViewerZoomOut, _, cx| this.zoom_by(1. / STEP, cx)))
            .on_action(cx.listener(|this, _: &ViewerFit, _, cx| {
                this.fit();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ViewerActualSize, _, cx| {
                let center = point(px(this.viewport.0 / 2.), px(this.viewport.1 / 2.));
                this.zoom_to(1., center, cx);
            }))
            .on_action(cx.listener(|this, _: &NextImage, _, cx| this.step(true, cx)))
            .on_action(cx.listener(|this, _: &PreviousImage, _, cx| this.step(false, cx)))
            .on_action(cx.listener(Self::open_in_preview))
            .on_scroll(cx.listener(Self::on_scroll))
            .on_pinch(cx.listener(Self::on_pinch))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .when_some(image.zip(bounds), |d, (image, bounds)| {
                d.child(
                    img(image.path)
                        .id("viewer-image")
                        .absolute()
                        .left(bounds.origin.x)
                        .top(bounds.origin.y)
                        .w(bounds.size.width)
                        .h(bounds.size.height)
                        .object_fit(ObjectFit::Fill),
                )
            })
            .when(bounds.is_none(), |d| {
                d.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.marker)
                        .child("This image cannot be shown."),
                )
            })
            .child(
                // In the title bar, in place of the document's name.
                div()
                    .id("viewer-title")
                    .test_support()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .px(px(96.))
                    .h(px(36.))
                    // A bar of its own, as the editor's title bar is, so a
                    // zoomed image passes under it.
                    .bg(theme.background)
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_family(PROSE_FONT)
                    .text_size(px(12.))
                    .text_color(theme.marker)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .window_control_area(WindowControlArea::Drag)
                    .child(title),
            )
            .child(self.render_controls(&theme, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_image_fits_and_a_small_one_keeps_its_size() {
        let viewport = (1000., 800.);
        let scale = resting_scale((4000., 1000.), viewport);
        assert!((4000. * scale - (1000. - 2. * MARGIN_X)).abs() < 0.01);
        assert!((resting_scale((200., 100.), viewport) - 1.).abs() < f32::EPSILON);
    }

    #[test]
    fn zooming_keeps_the_point_under_the_pointer() {
        let anchor = point(px(100.), px(-50.));
        let offset = point(px(10.), px(20.));
        let zoomed = zoom_offset(offset, anchor, 1., 2.);
        // The image point under the anchor, in image pixels from its center.
        let before = (
            f32::from(anchor.x - offset.x),
            f32::from(anchor.y - offset.y),
        );
        let after = (
            f32::from(anchor.x - zoomed.x) / 2.,
            f32::from(anchor.y - zoomed.y) / 2.,
        );
        assert!((before.0 - after.0).abs() < 0.01 && (before.1 - after.1).abs() < 0.01);
    }

    #[test]
    fn a_panned_image_stays_over_the_window() {
        assert!(clamp_offset(300., 500., 800., 40.).abs() < f32::EPSILON);
        assert!((clamp_offset(900., 1200., 800., 40.) - 240.).abs() < f32::EPSILON);
        assert!((clamp_offset(-900., 1200., 800., 40.) + 240.).abs() < f32::EPSILON);
    }

    #[test]
    fn an_image_is_called_by_its_alt_text_or_file_name() {
        assert_eq!(title("A chart", Path::new("/a/b.png")), "A chart");
        assert_eq!(title(" ", Path::new("/a/b.png")), "b.png");
    }
}
