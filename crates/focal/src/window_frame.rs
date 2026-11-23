//! The frame (position and size) of Focal's last resized window, kept as JSON
//! beside the settings so new windows open where the last one was, the
//! choice of screen a new window opens on, and the fitting that keeps any
//! frame on that screen.
//!
//! GPUI places a window relative to the top left of its display, so a frame
//! means nothing without its display: frames from different displays share
//! coordinates. The arithmetic works on plain numbers in points so it can be
//! tested without a window; the GPUI conversions sit at the bottom.

use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{Bounds, Pixels, point, px, size};
use serde::{Deserialize, Serialize};

/// A window's frame in points, relative to the top left of its display, as
/// GPUI reports it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// The saved frame and the display it was on, by the display's UUID (which,
/// unlike its id, survives reconnecting it). Frames saved before the display
/// was recorded have none and count as being on the primary display, where
/// they were reopened then.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    #[serde(flatten)]
    pub frame: Frame,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

/// A connected display, by its id, and its visible area (less the menu bar
/// and the Dock), relative to its own top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub display: u64,
    pub area: Frame,
}

/// Where a window is: the display and the frame on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub display: u64,
    pub frame: Frame,
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
pub fn parse(json: &str) -> Option<Saved> {
    let saved: Saved = serde_json::from_str(json).ok()?;
    (saved.frame.width > 0. && saved.frame.height > 0.).then_some(saved)
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

/// The screen the user is working on: the one showing Focal's key window,
/// else the one under the pointer (where the terminal that ran `focal`
/// almost always is), else the primary one. A display that is not among
/// `screens` (no longer connected) is passed over.
pub fn active_screen(
    screens: &[Screen],
    primary: Screen,
    key_window: Option<u64>,
    pointer: Option<u64>,
) -> Screen {
    [key_window, pointer]
        .into_iter()
        .flatten()
        .find_map(|display| screens.iter().find(|screen| screen.display == display))
        .copied()
        .unwrap_or(primary)
}

/// Where a new window opens, already fitted to its screen.
///
/// - While the session's first window on the active screen (`anchor`) is
///   there, later windows step down and right from it by `step`, as on
///   macOS.
/// - The session's first window opens at the remembered frame, on its
///   display, when that display is still connected. `remembered` carries the
///   id of that display, or `None` when it is gone.
/// - Otherwise the window is centred on the active screen, the one the user
///   is working on.
///
/// The size is always the remembered one, or the default.
pub fn placement(
    remembered: Option<(Option<u64>, Frame)>,
    anchor: Option<Place>,
    step: f32,
    screens: &[Screen],
    active: Screen,
) -> Place {
    let (width, height) = remembered.map_or((DEFAULT_WIDTH, DEFAULT_HEIGHT), |(_, frame)| {
        (frame.width, frame.height)
    });
    let screen = |display: u64| screens.iter().copied().find(|s| s.display == display);
    let (screen, origin) = match anchor {
        Some(anchor) if anchor.display == active.display => {
            (active, Some((anchor.frame.x + step, anchor.frame.y + step)))
        }
        Some(_) => (active, None),
        None => match remembered.and_then(|(display, frame)| Some((screen(display?)?, frame))) {
            Some((screen, frame)) => (screen, Some(frame.origin())),
            None => (active, None),
        },
    };
    let (x, y) = origin.unwrap_or((
        screen.area.x + (screen.area.width - width) / 2.,
        screen.area.y + (screen.area.height - height) / 2.,
    ));
    Place {
        display: screen.display,
        frame: fit(
            Frame {
                x,
                y,
                width,
                height,
            },
            screen.area,
        ),
    }
}

/// Reads a saved frame from `path`; a missing or unreadable file gives `None`.
pub fn load_from(path: &Path) -> Option<Saved> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|json| parse(&json))
}

pub fn save_to(saved: &Saved, path: &Path) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(saved)? + "\n";
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
pub fn load() -> Option<Saved> {
    load_from(&path()?)
}

/// Writes `frame` to the saved location, reporting a failure without
/// stopping the app.
pub fn save(saved: &Saved) {
    if let Some(path) = path()
        && let Err(error) = save_to(saved, &path)
    {
        eprintln!("focal: could not save the window frame: {error:#}");
    }
}

/// Remembers `saved` as the frame new windows open at, and writes it after a
/// pause, so a drag writes once and never blocks the UI thread.
pub fn remember(saved: Saved, cx: &mut gpui_kit::App) {
    let memory = cx.global_mut::<Memory>();
    if memory.frame.as_ref() == Some(&saved) {
        return;
    }
    memory.frame = Some(saved);
    memory.generation += 1;
    let generation = memory.generation;
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SAVE_DELAY).await;
        let latest = cx.update(|cx| {
            let memory = cx.global::<Memory>();
            (memory.generation == generation)
                .then(|| memory.frame.clone())
                .flatten()
        });
        if let Some(saved) = latest {
            cx.background_executor()
                .spawn(async move { save(&saved) })
                .await;
        }
    })
    .detach();
}

/// The last frame and the write that is waiting for it, held as a global.
#[derive(Default)]
pub struct Memory {
    pub frame: Option<Saved>,
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

    /// The LG ultrawide, the primary display with the menu bar, and the
    /// laptop's built-in screen beside it. GPUI gives each area relative to
    /// its own display, so both start near 0, 0.
    const PRIMARY: Screen = Screen {
        display: 1,
        area: Frame {
            x: 0.,
            y: 25.,
            width: 3440.,
            height: 1415.,
        },
    };
    const BUILT_IN: Screen = Screen {
        display: 2,
        area: Frame {
            x: 0.,
            y: 0.,
            width: 1512.,
            height: 949.,
        },
    };
    const BOTH: [Screen; 2] = [PRIMARY, BUILT_IN];

    fn centred(frame: Frame, screen: Screen) -> bool {
        let area = screen.area;
        close(
            frame.x - area.x,
            area.x + area.width - frame.x - frame.width,
        )
    }

    #[test]
    fn the_pointer_picks_the_screen_without_a_key_window() {
        assert_eq!(active_screen(&BOTH, PRIMARY, None, Some(2)), BUILT_IN);
        assert_eq!(active_screen(&BOTH, PRIMARY, None, Some(1)), PRIMARY);
    }

    #[test]
    fn the_key_window_picks_the_screen_before_the_pointer() {
        assert_eq!(active_screen(&BOTH, PRIMARY, Some(2), Some(1)), BUILT_IN);
        assert_eq!(active_screen(&BOTH, PRIMARY, Some(1), Some(2)), PRIMARY);
    }

    #[test]
    fn an_unknown_display_falls_back_to_the_next_clue_then_the_primary() {
        assert_eq!(active_screen(&BOTH, PRIMARY, Some(9), Some(2)), BUILT_IN);
        assert_eq!(active_screen(&BOTH, PRIMARY, Some(9), None), PRIMARY);
        assert_eq!(active_screen(&BOTH, PRIMARY, None, None), PRIMARY);
    }

    #[test]
    fn without_a_saved_frame_the_window_opens_centred_on_the_active_screen() {
        // `focal file.md` from a terminal on the built-in screen, the pointer
        // there, while the LG is the primary display.
        let active = active_screen(&BOTH, PRIMARY, None, Some(BUILT_IN.display));
        let place = placement(None, None, 0., &BOTH, active);
        assert_eq!(place.display, BUILT_IN.display);
        assert!(on_screen(place.frame, BUILT_IN.area));
        assert!(centred(place.frame, BUILT_IN));
        assert!(close(place.frame.width, DEFAULT_WIDTH));
    }

    #[test]
    fn a_single_screen_still_centres_the_first_window() {
        let screen = Screen {
            display: 7,
            area: LARGE,
        };
        let active = active_screen(&[screen], screen, None, None);
        let place = placement(None, None, 0., &[screen], active);
        assert_eq!(place.display, 7);
        assert_eq!(
            (place.frame.width, place.frame.height),
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        );
        assert!(close(place.frame.x, (1920. - DEFAULT_WIDTH) / 2.));
        assert!(on_screen(place.frame, LARGE));
    }

    #[test]
    fn a_saved_frame_on_a_connected_display_wins_over_the_active_screen() {
        let saved = Frame {
            x: 100.,
            y: 120.,
            width: 900.,
            height: 700.,
        };
        let place = placement(
            Some((Some(PRIMARY.display), saved)),
            None,
            0.,
            &BOTH,
            BUILT_IN,
        );
        assert_eq!(
            place,
            Place {
                display: PRIMARY.display,
                frame: saved,
            }
        );
    }

    #[test]
    fn a_saved_frame_on_a_disconnected_display_opens_centred_on_the_active_screen() {
        let saved = Frame {
            x: 3000.,
            y: 200.,
            width: 900.,
            height: 700.,
        };
        // The display it was on is gone; the user works on the built-in one.
        let place = placement(Some((None, saved)), None, 0., &BOTH, BUILT_IN);
        assert_eq!(place.display, BUILT_IN.display);
        assert_eq!((place.frame.width, place.frame.height), (900., 700.));
        assert!(on_screen(place.frame, BUILT_IN.area));
        assert!(centred(place.frame, BUILT_IN));
    }

    #[test]
    fn later_windows_cascade_from_the_first_one_at_the_remembered_size() {
        let screen = Screen {
            display: 1,
            area: LARGE,
        };
        let remembered = Frame {
            x: 400.,
            y: 300.,
            width: 1000.,
            height: 700.,
        };
        let first = placement(Some((Some(1), remembered)), None, 0., &[screen], screen);
        assert_eq!(first.frame.origin(), (400., 300.));
        let second = placement(
            Some((Some(1), remembered)),
            Some(first),
            24.,
            &[screen],
            screen,
        );
        assert_eq!(second.frame.origin(), (424., 324.));
        assert_eq!((second.frame.width, second.frame.height), (1000., 700.));
    }

    #[test]
    fn a_new_window_follows_the_key_window_to_another_screen() {
        // The session's first window is on the LG; the user now works in a
        // window on the built-in screen and opens another.
        let first = placement(None, None, 0., &BOTH, PRIMARY);
        let active = active_screen(&BOTH, PRIMARY, Some(BUILT_IN.display), None);
        let next = placement(None, Some(first), 24., &BOTH, active);
        assert_eq!(next.display, BUILT_IN.display);
        assert!(on_screen(next.frame, BUILT_IN.area));
        assert!(centred(next.frame, BUILT_IN));
    }

    #[test]
    fn a_cascade_past_the_edge_is_pulled_back_on_screen() {
        let screen = Screen {
            display: 1,
            area: LARGE,
        };
        let edge = Place {
            display: 1,
            frame: Frame {
                x: 1000.,
                y: 600.,
                width: 800.,
                height: 400.,
            },
        };
        let place = placement(
            Some((Some(1), edge.frame)),
            Some(edge),
            240.,
            &[screen],
            screen,
        );
        assert!(on_screen(place.frame, LARGE));
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
        let saved = Saved {
            frame,
            display: Some("0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9".to_owned()),
        };
        save_to(&saved, &path).unwrap();
        assert_eq!(load_from(&path), Some(saved));
    }

    #[test]
    fn a_frame_saved_before_displays_were_recorded_still_reads() {
        let saved = parse(r#"{"x": 1, "y": 2, "width": 300, "height": 400}"#).unwrap();
        assert_eq!(saved.display, None);
        assert!(close(saved.frame.width, 300.));
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
}
