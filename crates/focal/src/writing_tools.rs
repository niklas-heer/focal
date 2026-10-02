//! Apple's Writing Tools (Proofread, Rewrite, Summarize, …) for the editor.
//!
//! GPUI's view does not offer its text to macOS, so Writing Tools stayed
//! greyed out. This module teaches GPUI's view class the Services protocol
//! (`NSServicesMenuRequestor`), the way AppKit lets any custom view take
//! part: Writing Tools reads the editor's selection, or the whole document
//! when nothing is selected, shows its result in Apple's panel, and Replace
//! hands the new text back, which the editor applies as one undo step.
//!
//! Adding methods to a class at runtime is `unsafe`; this module is one of
//! the bridges the decision "Allow unsafe code only in named macOS bridges"
//! names.
#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::ops::Range;

use gpui_kit::EntityId;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{ClassType as _, MainThreadMarker, msg_send, sel};
use objc2_app_kit::{NSApplication, NSPasteboard, NSPasteboardTypeString, NSView};
use objc2_foundation::{NSArray, NSString};

/// Text the focused editor offers, and where Replace sends the new text.
pub struct Target {
    pub owner: EntityId,
    /// The offered text's byte range in the document.
    pub range: Range<usize>,
    pub text: String,
    pub sender: async_channel::Sender<Replacement>,
}

/// New text for `range`, which held `original` when it was offered.
pub struct Replacement {
    pub range: Range<usize>,
    pub original: String,
    pub text: String,
}

type ValidRequestor =
    extern "C-unwind" fn(&AnyObject, Sel, Option<&NSString>, Option<&NSString>) -> *mut AnyObject;
type WriteSelection = extern "C-unwind" fn(&AnyObject, Sel, &NSPasteboard, &AnyObject) -> Bool;
type ReadSelection = extern "C-unwind" fn(&AnyObject, Sel, &NSPasteboard) -> Bool;

thread_local! {
    static TARGET: RefCell<Option<Target>> = const { RefCell::new(None) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Offers `target` to Writing Tools, in place of what was offered before.
pub fn offer(target: Target) {
    TARGET.with_borrow_mut(|slot| *slot = Some(target));
}

/// Stops offering text from `owner`, if it was offering.
pub fn withdraw(owner: EntityId) {
    TARGET.with_borrow_mut(|slot| {
        if slot.as_ref().is_some_and(|target| target.owner == owner) {
            *slot = None;
        }
    });
}

/// Whether `owner` offers `range` of the document as it is now.
pub fn offers(owner: EntityId, range: &Range<usize>, text: &str) -> bool {
    TARGET.with_borrow(|slot| {
        slot.as_ref().is_some_and(|target| {
            target.owner == owner && target.range == *range && target.text == text
        })
    })
}

/// The text Writing Tools would get now.
pub fn offered_text() -> Option<String> {
    TARGET.with_borrow(|slot| slot.as_ref().map(|target| target.text.clone()))
}

/// Sends `text` to the offering editor in place of what it offered, as
/// Replace in Writing Tools' panel does. False when nothing is offered.
pub fn replace(text: String) -> bool {
    TARGET.with_borrow(|slot| {
        slot.as_ref().is_some_and(|target| {
            target
                .sender
                .try_send(Replacement {
                    range: target.range.clone(),
                    original: target.text.clone(),
                    text,
                })
                .is_ok()
        })
    })
}

/// Adds the Services methods to GPUI's view class and tells AppKit that
/// Focal sends and takes plain text. Does nothing until GPUI has opened a
/// window (the class exists from then on), and only once.
pub fn install() {
    if INSTALLED.get() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let Some(class) = AnyClass::get(c"GPUIView") else {
        return;
    };
    let class: *const AnyClass = class;
    // SAFETY: each function has the signature its type encoding states and
    // the selector expects (object and BOOL results, object arguments that
    // may be nil); `class_addMethod` leaves a method the class already has
    // untouched.
    unsafe {
        let valid: ValidRequestor = valid_requestor;
        let write: WriteSelection = write_selection;
        let read: ReadSelection = read_selection;
        objc2::ffi::class_addMethod(
            class.cast_mut(),
            sel!(validRequestorForSendType:returnType:),
            std::mem::transmute::<ValidRequestor, Imp>(valid),
            c"@@:@@".as_ptr(),
        );
        objc2::ffi::class_addMethod(
            class.cast_mut(),
            sel!(writeSelectionToPasteboard:types:),
            std::mem::transmute::<WriteSelection, Imp>(write),
            c"B@:@@".as_ptr(),
        );
        objc2::ffi::class_addMethod(
            class.cast_mut(),
            sel!(readSelectionFromPasteboard:),
            std::mem::transmute::<ReadSelection, Imp>(read),
            c"B@:@".as_ptr(),
        );
    }
    // SAFETY: AppKit's constant string, which lives for the program.
    let string: &NSString = unsafe { NSPasteboardTypeString };
    let types = NSArray::from_slice(&[string]);
    NSApplication::sharedApplication(mtm).registerServicesMenuSendTypes_returnTypes(&types, &types);
    INSTALLED.set(true);
}

/// Plain text, or no type at all.
fn is_text(kind: Option<&NSString>) -> bool {
    // SAFETY: AppKit's constant string, which lives for the program.
    kind.is_none_or(|kind| kind == unsafe { NSPasteboardTypeString })
}

extern "C-unwind" fn valid_requestor(
    this: &AnyObject,
    _: Sel,
    send: Option<&NSString>,
    give: Option<&NSString>,
) -> *mut AnyObject {
    let offered = TARGET.with_borrow(Option::is_some);
    if offered && is_text(send) && is_text(give) && (send.is_some() || give.is_some()) {
        return std::ptr::from_ref(this).cast_mut();
    }
    // SAFETY: GPUI's view is an NSView, whose own answer is asked for.
    unsafe {
        msg_send![super(this, NSView::class()), validRequestorForSendType: send, returnType: give]
    }
}

extern "C-unwind" fn write_selection(
    _: &AnyObject,
    _: Sel,
    pasteboard: &NSPasteboard,
    _: &AnyObject,
) -> Bool {
    let Some(text) = offered_text() else {
        return Bool::NO;
    };
    pasteboard.clearContents();
    // SAFETY: AppKit's constant string, which lives for the program.
    let kind = unsafe { NSPasteboardTypeString };
    Bool::new(pasteboard.setString_forType(&NSString::from_str(&text), kind))
}

extern "C-unwind" fn read_selection(_: &AnyObject, _: Sel, pasteboard: &NSPasteboard) -> Bool {
    // SAFETY: AppKit's constant string, which lives for the program.
    let kind = unsafe { NSPasteboardTypeString };
    let text: Option<Retained<NSString>> = pasteboard.stringForType(kind);
    Bool::new(text.is_some_and(|text| replace(text.to_string())))
}
