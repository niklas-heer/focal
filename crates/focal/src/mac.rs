//! Small AppKit calls GPUI has no API for.

use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertSecondButtonReturn, NSAlertStyle, NSAppearance,
    NSApplication, NSPasteboard,
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

/// Why closing a document would lose text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseQuestion {
    /// An untitled document with unsaved text.
    Untitled,
    /// The file changed on disk while it had unsaved edits.
    Conflict,
}

/// Asks what to do with the document `name`'s unsaved text before closing
/// it: save it (for a conflict, keep this version over the file's), don't,
/// or cancel. Like [`alert`], call it outside GPUI updates. Without
/// AppKit's main thread (tests) nothing is asked and the answer is to cancel.
pub fn ask_on_close(question: CloseQuestion, name: &str) -> SaveAnswer {
    let Some(mtm) = MainThreadMarker::new() else {
        return SaveAnswer::Cancel;
    };
    let (message, detail, save, discard) = match question {
        CloseQuestion::Untitled => (
            format!("Do you want to save “{name}”?"),
            "Its text will be lost if you don’t save it.",
            "Save…",
            "Don’t Save",
        ),
        CloseQuestion::Conflict => (
            format!("“{name}” changed on disk while you had unsaved edits."),
            "Keep your version to replace the file, or discard your edits and leave the file as it is.",
            "Keep Mine",
            "Discard Mine",
        ),
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&message));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.addButtonWithTitle(&NSString::from_str(save));
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    alert.addButtonWithTitle(&NSString::from_str(discard));
    match alert.runModal() {
        response if response == NSAlertFirstButtonReturn => SaveAnswer::Save,
        response if response == NSAlertSecondButtonReturn => SaveAnswer::Cancel,
        _ => SaveAnswer::DontSave,
    }
}

/// Asks whether to quit although `count` documents have text that could not
/// be saved (untitled, or changed on disk meanwhile). Like [`ask_on_close`],
/// without AppKit's main thread the answer is no.
pub fn confirm_discard(count: usize) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let (message, detail) = if count == 1 {
        (
            "Quit and discard unsaved changes in 1 document?".to_owned(),
            "It is untitled or changed on disk meanwhile. Cancel to save it first.",
        )
    } else {
        (
            format!("Quit and discard unsaved changes in {count} documents?"),
            "They are untitled or changed on disk meanwhile. Cancel to save them first.",
        )
    };
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&NSString::from_str(&message));
    alert.setInformativeText(&NSString::from_str(detail));
    alert.setAlertStyle(NSAlertStyle::Warning);
    alert.addButtonWithTitle(&NSString::from_str("Cancel"));
    alert.addButtonWithTitle(&NSString::from_str("Discard and Quit"));
    alert.runModal() == NSAlertSecondButtonReturn
}

/// Puts `html` and its `plain` text on the general pasteboard, so rich text
/// editors paste the HTML and others the text. Tests (off AppKit's main
/// thread) leave the pasteboard alone.
pub fn copy_html(html: &str, plain: &str) {
    if MainThreadMarker::new().is_none() {
        return;
    }
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    pasteboard.setString_forType(
        &NSString::from_str(html),
        &NSString::from_str("public.html"),
    );
    pasteboard.setString_forType(
        &NSString::from_str(plain),
        &NSString::from_str("public.utf8-plain-text"),
    );
}

/// The standard About panel, with the bundle's name, version and icon.
pub fn about_panel() {
    if let Some(mtm) = MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).orderFrontStandardAboutPanel(None);
    }
}
