//! Small AppKit calls GPUI has no API for.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSAlert, NSAlertStyle, NSAppearance, NSApplication};
use objc2_foundation::NSString;

/// `FOCAL_APPEARANCE=light` or `dark` overrides the system appearance for
/// Focal alone, to check both palettes without switching the whole Mac.
pub fn force_appearance() {
    let name = match std::env::var("FOCAL_APPEARANCE").as_deref() {
        Ok("light") => "NSAppearanceNameAqua",
        Ok("dark") => "NSAppearanceNameDarkAqua",
        _ => return,
    };
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let appearance = NSAppearance::appearanceNamed(&NSString::from_str(name));
    NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
}

/// Shows an app-modal alert with one OK button and waits for it. Call it
/// outside GPUI updates (from a spawned task), since the modal run loop
/// keeps drawing windows.
pub fn alert(message: &str, detail: &str, warning: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(message));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.setAlertStyle(if warning {
        NSAlertStyle::Warning
    } else {
        NSAlertStyle::Informational
    });
    alert.addButtonWithTitle(&NSString::from_str("OK"));
    alert.runModal();
}

/// The standard About panel, with the bundle's name, version and icon.
pub fn about_panel() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).orderFrontStandardAboutPanel(None);
    }
}
