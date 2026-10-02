+++
schema_version = 1
id = "01M3WG5NNYPNXAMJ1EYMYN8RGV"
title = "Print and export PDF through WebKit"
date = "2026-10-01"
status = "accepted"
tags = ["output", "printing"]
supersedes = []
superseded_by = []
depends_on = ["01M3WG5NPAHN0J0ZV95T94QM9Y"]
related_to = []
+++
## Decision

Focal prints and exports PDF through WebKit. The document becomes a page for paper (`export::write_print_page`: the same HTML as Export as HTML, light whatever the appearance, `@page` margins, folded callouts open, the iA Writer typefaces beside it, a `<base>` at the document's folder), an offscreen `WKWebView` loads it, and AppKit's print system takes it from there: Print… (⌥⌘P) shows the print panel as a sheet with a preview; Export as PDF… writes the file without asking, through the same print operation with the save disposition.

Proposed on 2026-10-01 while building Milestone 12; Niklas asked for PDF export and printing but did not choose the mechanism.

Accepted by Niklas on 2026-10-02: following the recommendation ("The rest yes let's follow your recommendation").

## Context

- Export as HTML already renders the document the way the editor shows it, with typeset math and drawn diagrams as SVG. Printing that page means one renderer for HTML, PDF and paper, so the three never drift apart.
- WebKit paginates properly (`@page`, `break-inside`, headings kept with what follows) and the print panel gives paper size, orientation, scaling, page ranges and "Save as PDF" for free.
- WebKit's printing only works asynchronously: `NSPrintOperation.run()` hangs with a `WKWebView`, so the operation runs as a sheet on the document's window (`runOperationModalForWindow`). The view must be given the paper's frame, or the pages come out blank.
- The design principle "Native first; no web views" is about the editor. Here the web view is never shown and only lays out a page Focal wrote itself.

Alternatives considered:

- **Typst** (pure Rust): typographically excellent PDFs, but a second renderer that turns Markdown, math and every diagram language into Typst, and would differ from the HTML export in many small ways.
- **Drawing the editor's own layout into a PDF context** with GPUI: no web view, but GPUI has no pagination or print path, and the editor's layout is for a screen (caret, revealed syntax, scrolling islands).
- **No printing; "export HTML and print from a browser"**, the stopgap until now. Rejected: Niklas asked for both.

## Consequences

- `crates/focal/src/print.rs` calls WebKit and AppKit through `objc2`, which marks those methods `unsafe`; it is one of the modules the unsafe policy allows (see the decision on unsafe code in macOS bridges).
- The page's images load from disk through WebKit with read access to the whole disk, as the editor shows them; remote images are fetched like a browser would.
- Printing waits up to 20 seconds for the page and its images to load, then prints what it has.
- There is no automated test of the WebKit step (GPUI's test platform has no Cocoa run loop); the page for paper is tested, the print path is checked by hand.
