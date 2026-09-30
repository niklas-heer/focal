//! Updates through Sparkle (decision "Update Focal with Sparkle through
//! objc2"). A release bundle carries `Sparkle.framework` and a feed in its
//! `Info.plist`; Focal loads the framework and starts Sparkle's standard
//! updater, whose prompt asks before downloading. Focal's setting turns the
//! automatic checks on or off.
//!
//! This is the one module allowed to use `unsafe`: Sparkle is reached by
//! Objective-C messages, which Rust cannot check.
#![allow(unsafe_code)]

use gpui_kit::{App, Global, actions};
use objc2::msg_send;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSBundle, NSString};

use crate::settings::Settings;

actions!(focal, [CheckForUpdates, OpenReleases]);

const RELEASES: &str = "https://github.com/niklas-heer/focal/releases";

/// Sparkle's `SPUStandardUpdaterController`, when Focal runs from a release
/// bundle.
struct Updater(Option<Retained<AnyObject>>);

impl Global for Updater {}

/// Starts the updater if the bundle has one, and keeps it in step with the
/// setting.
pub fn init(cx: &mut App) {
    cx.set_global(Updater(start()));
    apply_setting(cx);
    cx.observe_global::<Settings>(apply_setting).detach();
    cx.on_action(|_: &CheckForUpdates, cx| check(cx));
    cx.on_action(|_: &OpenReleases, cx| cx.open_url(RELEASES));
}

/// Whether this Focal can update itself.
pub fn available(cx: &App) -> bool {
    cx.global::<Updater>().0.is_some()
}

fn start() -> Option<Retained<AnyObject>> {
    let bundle = NSBundle::mainBundle();
    // Only release builds name a feed; development builds have no updater.
    bundle.objectForInfoDictionaryKey(&NSString::from_str("SUFeedURL"))?;
    let frameworks = bundle.privateFrameworksPath()?;
    let path = NSString::from_str(&format!("{frameworks}/Sparkle.framework"));
    let sparkle = NSBundle::bundleWithPath(&path)?;
    // SAFETY: loading runs the framework's initializers, which register
    // Sparkle's classes. The framework ships inside this bundle, signed with
    // it, and is built for the macOS versions Focal supports.
    if !unsafe { sparkle.load() } {
        return None;
    }
    let class = AnyClass::get(c"SPUStandardUpdaterController")?;
    // SAFETY: `alloc` on an `NSObject` subclass returns a new, uninitialized
    // instance that the following `init…` consumes.
    let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
    // SAFETY: Sparkle's designated initializer
    // `-initWithStartingUpdater:(BOOL)updaterDelegate:(id)userDriverDelegate:(id)`;
    // both delegates may be nil. It returns nil if the updater cannot start.
    unsafe {
        msg_send![
            allocated,
            initWithStartingUpdater: true,
            updaterDelegate: Option::<&AnyObject>::None,
            userDriverDelegate: Option::<&AnyObject>::None,
        ]
    }
}

fn apply_setting(cx: &mut App) {
    let enabled = cx.global::<Settings>().check_updates;
    let Some(controller) = &cx.global::<Updater>().0 else {
        return;
    };
    // SAFETY: `-updater` returns the controller's `SPUUpdater`, an object.
    let updater: Option<Retained<AnyObject>> = unsafe { msg_send![&**controller, updater] };
    if let Some(updater) = updater {
        // SAFETY: `-setAutomaticallyChecksForUpdates:(BOOL)` on `SPUUpdater`.
        let () = unsafe { msg_send![&*updater, setAutomaticallyChecksForUpdates: enabled] };
    }
}

/// Checks now, showing Sparkle's prompt; without an updater, opens the
/// releases page.
fn check(cx: &mut App) {
    match &cx.global::<Updater>().0 {
        // SAFETY: `-checkForUpdates:(id)sender` on the controller; the sender
        // may be nil.
        Some(controller) => {
            let () =
                unsafe { msg_send![&**controller, checkForUpdates: Option::<&AnyObject>::None] };
        }
        None => cx.open_url(RELEASES),
    }
}
