//! Printing and PDF export. The document's page for paper
//! (`export::write_print_page`) is laid out by WebKit, the engine exported
//! HTML is read in, and handed to AppKit's print system: to a printer
//! through the print panel, or straight into a PDF file.
//!
//! WebKit's API is reached through `objc2`, which marks its methods
//! `unsafe`; this module is one of the few that allow `unsafe` (see the
//! decision "Allow unsafe code only in named macOS bridges").
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSPrintInfo, NSPrintJobSavingURL, NSPrintPanelOptions, NSPrintSaveJob, NSWindow,
};
use objc2_foundation::{NSCopying as _, NSPoint, NSRect, NSSize, NSString, NSURL};
use objc2_web_kit::{WKWebView, WKWebViewConfiguration};

/// Where a printed document goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    /// A printer chosen in the print panel (which can also save a PDF).
    Printer,
    /// A PDF file, without asking.
    Pdf(PathBuf),
}

/// A page loading in a web view that is never shown, to be printed once
/// it has loaded. Keep it until printing has finished.
pub struct Job {
    web: Retained<WKWebView>,
    output: Output,
    title: String,
    /// The window the print operation runs on, once it does.
    window: Option<Retained<NSWindow>>,
    /// Tells this job from a later one.
    pub id: u64,
}

static JOBS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Job {
    /// Starts loading `page`. Images may come from anywhere on disk, as
    /// they do in the editor.
    pub fn load(page: &Path, output: Output, title: &str) -> anyhow::Result<Self> {
        let mtm = MainThreadMarker::new().context("printing runs on the main thread")?;
        let frame = NSRect::new(NSPoint::new(0., 0.), NSSize::new(800., 600.));
        let url = NSURL::fileURLWithPath(&NSString::from_str(&page.to_string_lossy()));
        let disk = NSURL::fileURLWithPath(&NSString::from_str("/"));
        // SAFETY: a default configuration, a web view made from it on the main
        // thread (checked above), and file URLs that live through the call.
        let web = unsafe {
            let configuration = WKWebViewConfiguration::new(mtm);
            let web = WKWebView::initWithFrame_configuration(mtm.alloc(), frame, &configuration);
            web.loadFileURL_allowingReadAccessToURL(&url, &disk);
            web
        };
        Ok(Self {
            web,
            output,
            title: title.to_owned(),
            window: None,
            id: JOBS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        })
    }

    /// Whether the page, with its images, is still loading.
    pub fn is_loading(&self) -> bool {
        // SAFETY: a property read on the web view this job owns.
        unsafe { self.web.isLoading() }
    }

    /// Prints the loaded page from the frontmost window: a PDF is written
    /// without asking, a printer is chosen in the print panel.
    pub fn print(&mut self) -> anyhow::Result<()> {
        let mtm = MainThreadMarker::new().context("printing runs on the main thread")?;
        let window = NSApplication::sharedApplication(mtm)
            .mainWindow()
            .context("there is no window to print from")?;
        let info = NSPrintInfo::sharedPrintInfo().copy();
        // The page's own `@page` margins apply.
        info.setTopMargin(0.);
        info.setBottomMargin(0.);
        info.setLeftMargin(0.);
        info.setRightMargin(0.);
        if let Output::Pdf(path) = &self.output {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            // SAFETY: AppKit's constant strings, which live for the program;
            // the saving URL key takes an NSURL, in the print info's own
            // mutable dictionary.
            unsafe {
                info.setJobDisposition(NSPrintSaveJob);
                info.dictionary().setObject_forKey(
                    &url,
                    objc2::runtime::ProtocolObject::from_ref(NSPrintJobSavingURL),
                );
            }
        }
        let paper = info.paperSize();
        // SAFETY: the web view has loaded; the operation is run as a sheet on
        // a window of this app, without a delegate or callback.
        unsafe {
            let operation = self.web.printOperationWithPrintInfo(&info);
            operation.setJobTitle(Some(&NSString::from_str(&self.title)));
            let to_printer = self.output == Output::Printer;
            operation.setShowsPrintPanel(to_printer);
            operation.setShowsProgressPanel(to_printer);
            operation.printPanel().setOptions(
                NSPrintPanelOptions::ShowsCopies
                    | NSPrintPanelOptions::ShowsPageRange
                    | NSPrintPanelOptions::ShowsPaperSize
                    | NSPrintPanelOptions::ShowsOrientation
                    | NSPrintPanelOptions::ShowsScaling
                    | NSPrintPanelOptions::ShowsPreview,
            );
            // WebKit lays the page out for the view's size; without one it
            // prints blank pages.
            if let Some(view) = operation.view() {
                view.setFrame(NSRect::new(NSPoint::new(0., 0.), paper));
            }
            operation.runOperationModalForWindow_delegate_didRunSelector_contextInfo(
                &window,
                None,
                None,
                std::ptr::null_mut(),
            );
        }
        self.window = Some(window);
        Ok(())
    }

    /// Whether the print panel or progress sheet is still open.
    pub fn sheet_open(&self) -> bool {
        self.window
            .as_ref()
            .is_some_and(|window| window.attachedSheet().is_some())
    }
}
