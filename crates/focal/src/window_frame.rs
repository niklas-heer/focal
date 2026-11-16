//! The frame (position and size) of Focal's last resized window, kept as JSON
//! beside the settings so new windows open where the last one was, and the
//! fitting that keeps any frame on the screen it opens on.
//!
//! The arithmetic works on plain numbers in points so it can be tested
//! without a window; the GPUI conversions sit at the bottom.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{Bounds, Pixels, point, px, size};
use serde::{Deserialize, Serialize};

/// A window's frame in points, with the origin at the top left of the
/// main display's visible area, as GPUI reports it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The size a window opens at when nothing is remembered.
pub const DEFAULT_WIDTH: f32 = 860.;
pub const DEFAULT_HEIGHT: f32 = 920.;
/// The smallest a window is kept at, unless the screen is smaller still.
pub const MIN_WIDTH: f32 = 480.;
pub const MIN_HEIGHT: f32 = 360.;
/// Space kept between a window and the edges of the screen.
const MARGIN: f32 = 16.;
/// How long after the last resize the frame is written, so a drag writes once.
const SAVE_DELAY: Duration = Duration::from_millis(500);

impl Frame {
    fn origin(self) -> (f32, f32) {
        (self.x, self.y)
    }

    /// The screen's visible area less the margin, as a frame.
    fn usable_area(self) -> Self {
        Self {
            x: self.x + MARGIN,
            y: self.y + MARGIN,
            width: (self.width - 2. * MARGIN).max(0.),
            height: (self.height - 2. * MARGIN).max(0.),
        }
    }
}

/// Parses a saved frame. Unreadable JSON, a missing field or a frame with
/// no area gives `None`, so the window falls back to the default placement.
pub fn parse(json: &str) -> Option<Frame> {
    let frame: Frame = serde_json::from_str(json).ok()?;
    (frame.width > 0. && frame.height > 0.).then_some(frame)
}

/// Fits `frame` onto `screen`'s visible area: the size is capped at the
/// usable area (and kept at least at the minimum where the screen allows),
/// and the origin is moved until the whole window is on screen. A frame
/// saved for a larger, since-disconnected screen is therefore brought in.
pub fn fit(frame: Frame, screen: Frame) -> Frame {
    let area = screen.usable_area();
    let width = frame.width.min(area.width).max(MIN_WIDTH.min(area.width));
    let height = frame
        .height
        .min(area.height)
        .max(MIN_HEIGHT.min(area.height));
    let x = frame.x.min(area.x + area.width - width).max(area.x);
    let y = frame.y.min(area.y + area.height - height).max(area.y);
    Frame {
        x,
        y,
        width,
        height,
    }
}

/// The screen containing `point`, or `primary` when none does (a display
/// that is no longer connected, say).
pub fn screen_at(point: (f32, f32), screens: &[Frame], primary: Frame) -> Frame {
    screens
        .iter()
        .copied()
        .find(|screen| {
            let (x, y) = point;
            x >= screen.x
                && x < screen.x + screen.width
                && y >= screen.y
                && y < screen.y + screen.height
        })
        .unwrap_or(primary)
}

/// Where a new window opens, already fitted to its screen.
///
/// The first window of a session opens at the remembered origin (or centred
/// on the primary screen when nothing is remembered). Later windows keep
/// the cascade: `anchor`, the first window's origin, moved by `step`. The
/// size is always the remembered one, or the default.
pub fn placement(
    remembered: Option<Frame>,
    anchor: Option<(f32, f32)>,
    step: f32,
    screens: &[Frame],
    primary: Frame,
) -> Frame {
    let (width, height) = remembered.map_or((DEFAULT_WIDTH, DEFAULT_HEIGHT), |frame| {
        (frame.width, frame.height)
    });
    let origin = match (anchor, remembered) {
        (Some((x, y)), _) => Some((x + step, y + step)),
        (None, Some(frame)) => Some(frame.origin()),
        (None, None) => None,
    };
    let screen = origin.map_or(primary, |origin| screen_at(origin, screens, primary));
    let (x, y) = origin.unwrap_or((
        screen.x + (screen.width - width) / 2.,
        screen.y + (screen.height - height) / 2.,
    ));
    fit(
        Frame {
            x,
            y,
            width,
            height,
        },
        screen,
    )
}

/// Reads a saved frame from `path`; a missing or unreadable file gives `None`.
pub fn load_from(path: &Path) -> Option<Frame> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|json| parse(&json))
}

pub fn save_to(frame: &Frame, path: &Path) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(frame)? + "\n";
    crate::document::write_replacing(path, json.as_bytes())?;
    Ok(())
}

/// `~/Library/Application Support/Focal/window.json`.
fn path() -> Option<PathBuf> {
    // Tests never touch the user's frame, as the settings tests don't.
    if cfg!(test) {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/Focal/window.json"))
}

/// The frame saved by the last session, if any.
pub fn load() -> Option<Frame> {
    load_from(&path()?)
}

/// Writes `frame` to the saved location, reporting a failure without
/// stopping the app.
pub fn save(frame: &Frame) {
    if let Some(path) = path()
        && let Err(error) = save_to(frame, &path)
    {
        eprintln!("focal: could not save the window frame: {error:#}");
    }
}

/// Remembers `frame` as the one new windows open at, and writes it after a
/// pause, so a drag writes once and never blocks the UI thread.
pub fn remember(frame: Frame, cx: &mut gpui_kit::App) {
    let memory = cx.global_mut::<Memory>();
    if memory.frame == Some(frame) {
        return;
    }
    memory.frame = Some(frame);
    memory.generation += 1;
    let generation = memory.generation;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SAVE_DELAY).await;
        let latest = cx.update(|cx| {
            let memory = cx.global::<Memory>();
            (memory.generation == generation)
                .then_some(memory.frame)
                .flatten()
        });
        if let Some(frame) = latest {
            cx.background_executor()
                .spawn(async move { save(&frame) })
                .await;
        }
    })
    .detach();
}

/// The last frame and the write that is waiting for it, held as a global.
#[derive(Default)]
pub struct Memory {
    pub frame: Option<Frame>,
    pub generation: u64,
}

impl gpui_kit::Global for Memory {}

pub fn to_bounds(frame: Frame) -> Bounds<Pixels> {
    Bounds {
        origin: point(px(frame.x), px(frame.y)),
        size: size(px(frame.width), px(frame.height)),
    }
}

pub fn from_bounds(bounds: Bounds<Pixels>) -> Frame {
    Frame {
        x: bounds.origin.x.into(),
        y: bounds.origin.y.into(),
        width: bounds.size.width.into(),
        height: bounds.size.height.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: Frame = Frame {
        x: 0.,
        y: 25.,
        width: 1280.,
        height: 800.,
    };
    const LARGE: Frame = Frame {
        x: 0.,
        y: 0.,
        width: 1920.,
        height: 1080.,
    };

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("focal-window-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("window.json")
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn on_screen(frame: Frame, screen: Frame) -> bool {
        frame.x >= screen.x
            && frame.y >= screen.y
            && frame.x + frame.width <= screen.x + screen.width
            && frame.y + frame.height <= screen.y + screen.height
    }

    #[test]
    fn the_default_window_fits_a_13_inch_screen() {
        // 1440 x 900 minus the menu bar and a Dock along the bottom.
        let screen = Frame {
            x: 0.,
            y: 25.,
            width: 1440.,
            height: 800.,
        };
        let frame = fit(
            Frame {
                x: 290.,
                y: 25.,
                width: DEFAULT_WIDTH,
                height: DEFAULT_HEIGHT,
            },
            screen,
        );
        assert!(close(frame.width, DEFAULT_WIDTH));
        assert!(close(frame.height, 800. - 2. * MARGIN));
        assert!(on_screen(frame, screen));
    }

    #[test]
    fn a_valid_saved_frame_is_kept_as_it_is() {
        let frame = Frame {
            x: 100.,
            y: 120.,
            width: 900.,
            height: 700.,
        };
        assert_eq!(fit(frame, LARGE), frame);
    }

    #[test]
    fn a_frame_from_a_disconnected_larger_screen_is_clamped_in() {
        let saved = Frame {
            x: 2600.,
            y: 300.,
            width: 1800.,
            height: 1400.,
        };
        let frame = fit(saved, SMALL);
        assert!(on_screen(frame, SMALL));
        assert!(frame.width <= SMALL.width - 2. * MARGIN);
        assert!(frame.height <= SMALL.height - 2. * MARGIN);
    }

    #[test]
    fn the_size_keeps_a_minimum_on_a_screen_that_allows_it() {
        let frame = fit(
            Frame {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 100.,
            },
            LARGE,
        );
        assert_eq!((frame.width, frame.height), (MIN_WIDTH, MIN_HEIGHT));
    }

    #[test]
    fn a_placement_on_a_disconnected_screen_moves_to_the_primary_one() {
        let primary = SMALL;
        let remembered = Frame {
            x: 3000.,
            y: 200.,
            width: 900.,
            height: 700.,
        };
        // Only the primary screen is connected: the saved origin is off it.
        let frame = placement(Some(remembered), None, 0., &[primary], primary);
        assert_eq!((frame.width, frame.height), (900., 700.));
        assert!(on_screen(frame, primary));
    }

    #[test]
    fn without_a_saved_frame_the_window_is_centred_on_the_primary_screen() {
        let frame = placement(None, None, 0., &[LARGE], LARGE);
        assert_eq!((frame.width, frame.height), (DEFAULT_WIDTH, DEFAULT_HEIGHT));
        assert!(close(frame.x, (1920. - DEFAULT_WIDTH) / 2.));
        assert!(on_screen(frame, LARGE));
    }

    #[test]
    fn later_windows_cascade_from_the_first_one_at_the_remembered_size() {
        let remembered = Frame {
            x: 400.,
            y: 300.,
            width: 1000.,
            height: 700.,
        };
        let first = placement(Some(remembered), None, 0., &[LARGE], LARGE);
        assert_eq!(first.origin(), (400., 300.));
        let anchor = Some(first.origin());
        let second = placement(Some(remembered), anchor, 24., &[LARGE], LARGE);
        assert_eq!(second.origin(), (424., 324.));
        assert_eq!((second.width, second.height), (1000., 700.));
    }

    #[test]
    fn a_cascade_past_the_edge_is_pulled_back_on_screen() {
        let edge = Frame {
            x: 1000.,
            y: 600.,
            width: 800.,
            height: 400.,
        };
        let frame = placement(Some(edge), Some((1000., 600.)), 240., &[LARGE], LARGE);
        assert!(on_screen(frame, LARGE));
    }

    #[test]
    fn a_frame_is_read_back_from_json_it_was_written_as() {
        let path = temp("round-trip");
        let frame = Frame {
            x: 12.5,
            y: 40.,
            width: 900.,
            height: 720.,
        };
        save_to(&frame, &path).unwrap();
        assert_eq!(load_from(&path), Some(frame));
    }

    #[test]
    fn unreadable_json_gives_no_frame() {
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"x": 1, "y": 2, "width": 3"#), None);
        assert_eq!(parse(r#"{"x": 1, "y": 2, "width": 3}"#), None);
        assert_eq!(parse(r#"{"x": 1, "y": 2, "width": 0, "height": 5}"#), None);
        assert_eq!(parse(""), None);
        let path = temp("unreadable");
        std::fs::write(&path, "{ broken").unwrap();
        assert_eq!(load_from(&path), None);
        assert_eq!(load_from(&path.with_file_name("missing.json")), None);
    }

    #[test]
    fn a_point_picks_the_screen_it_is_on() {
        let right = Frame {
            x: 1280.,
            y: 0.,
            width: 1920.,
            height: 1080.,
        };
        assert_eq!(screen_at((1500., 100.), &[SMALL, right], SMALL), right);
        assert_eq!(screen_at((-50., 100.), &[SMALL, right], SMALL), SMALL);
    }
}
