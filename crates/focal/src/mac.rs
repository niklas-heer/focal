//! Small AppKit calls GPUI has no API for.

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSAlertStyle, NSAppearance,
    NSApplication,
};
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

/// What to do with an untitled document's text when its window closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAnswer {
    Save,
    DontSave,
    Cancel,
}

/// Asks whether to save the untitled document `name` before closing it.
/// Like [`alert`], call it outside GPUI updates. Without AppKit's main
/// thread (tests) nothing is asked and the answer is to cancel.
pub fn ask_to_save(name: &str) -> SaveAnswer {
    let Some(mtm) = MainThreadMarker::new() else {
        return SaveAnswer::Cancel;
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&format!(
        "Do you want to save “{name}”?"
    )));
    alert.setInformativeText(&NSString::from_str(
        "Its text will be lost if you don’t save it.",
    ));
    alert.addButtonWithTitle(&NSString::from_str("Save…"));
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    alert.addButtonWithTitle(&NSString::from_str("Don’t Save"));
    match alert.runModal() {
        response if response == NSAlertFirstButtonReturn => SaveAnswer::Save,
        response if response == NSAlertSecondButtonReturn => SaveAnswer::Cancel,
        _ => SaveAnswer::DontSave,
    }
}

/// Asks whether to quit although `count` untitled documents have unsaved
/// text. Like [`ask_to_save`], without AppKit's main thread the answer is no.
pub fn confirm_discard(count: usize) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let (documents, detail) = if count == 1 {
        (
            "1 untitled document".to_owned(),
            "Its text has not been saved. Cancel to save it first.",
        )
    } else {
        (
            format!("{count} untitled documents"),
            "Their text has not been saved. Cancel to save it first.",
        )
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&format!(
        "Quit and discard {documents}?"
    )));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    alert.addButtonWithTitle(&NSString::from_str("Discard and Quit"));
    alert.runModal() == NSAlertSecondButtonReturn
}

/// The standard About panel, with the bundle's name, version and icon.
pub fn about_panel() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).orderFrontStandardAboutPanel(None);
    }
}
