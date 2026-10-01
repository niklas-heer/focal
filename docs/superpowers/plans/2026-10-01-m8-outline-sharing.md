# Milestone 8 — Outline and Sharing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move around a long document by its headings, and take a document out of Focal: Go to Heading (⇧⌘O), Export as HTML… and Copy as HTML.

**Architecture:** `focal-core` gains `outline.rs` (the headings with their level, plain title and offset) and `export.rs` (Markdown to an HTML fragment with `pulldown-cmark`, and a standalone page around it). Rendering that needs the app, typeset math and Mermaid diagrams, is passed in as a callback, so the core stays UI-free and testable. The quick switcher becomes a generic picker (items with a label and a detail) used by both ⌘P and Go to Heading. Copy as HTML writes HTML and plain text to the general pasteboard through `objc2-app-kit`'s safe `NSPasteboard` methods.

**Tech Stack:** Rust, `pulldown-cmark` 0.13 `html` renderer, MathJax (display and inline) and `merman` through the existing workers, `NSPasteboard`.

**Spec:** [`docs/design.md`](../../design.md) section 3 (documents), section 7 (extras rendered the same way as in the editor), section 10 (printing and PDF remain open; this milestone does not decide them).

## Global Constraints

- The file is the document: outline, export and copy never change the text.
- Export uses the same Markdown dialect as the editor (tables, footnotes, strikethrough, task lists, math, GFM alerts, wiki links, `==highlight==`) and leaves front matter out.
- Display and inline math export as MathJax SVG; Mermaid blocks as SVG in Focal's light palette; a failed render falls back to the escaped source in `<pre>`.
- The exported page stands alone: one HTML file with its CSS inline (iA Writer fonts when installed, system fonts otherwise), light and dark through `prefers-color-scheme`.
- No web views (design principle "Native first").
- `mise run check` passes after every task; logic in `focal-core` with unit tests; UI behavior with headless UI tests.
- Work on branch `m8-outline-sharing` in `.worktrees/rust-gpui`; conventional commits with the session's `Co-Authored-By` line; `mise run clean` when done.

## Review Focus

1. **Headings inside code blocks, or setext headings (`Title\n===`)**: code is not an outline entry; setext headings are. Task 1 tests both.
2. **Raw HTML in the Markdown** (`<script>`): exported as written, like every Markdown renderer, but never executed by Focal; Copy as HTML includes it too. Task 3 states it in a test name so the choice is visible.
3. **Relative image paths when exporting to another folder**: image `src` stays relative to the document; exporting elsewhere rewrites relative paths to absolute `file://` URLs so images still show. Task 4 tests it.
4. **Go to Heading in an empty document or one without headings**: the picker says so and Return does nothing. Task 2 tests it.
5. **A wiki link to a missing note**: exported as a link to `name.md` beside the document, styled like any link. Task 3 tests it.

---

### Task 1: Outline in focal-core

**Files:** Create `crates/focal-core/src/outline.rs`; modify `lib.rs`.

**Produces:** `pub struct Heading { pub level: u8, pub title: String, pub offset: usize }`; `pub fn outline(analysis: &Analysis, text: &str) -> Vec<Heading>` — ATX and setext headings in order, the title without `#`s, closing `#`s, or inline markers (`**`, `` ` ``, link syntax); `offset` is where the title text starts.

- [ ] Tests: ATX levels; closing hashes; inline markup stripped (`## A **bold** [link](x)` → `A bold link`); setext; a `#` line inside a fenced code block is not a heading; empty text → empty.

### Task 2: A generic picker, and Go to Heading

**Files:** Modify `switcher.rs` (generic `Picker` with `PickItem { label, detail }`, `PickerEvent::{Pick(usize), Dismiss}`, a placeholder and an empty message), `workspace.rs`, `menus.rs`, `ui_tests.rs`.

- ⌘P keeps working as before through the picker (files, newest first, fuzzy by path). ⇧⌘O "Go to Heading…" lists the outline in document order, indented by level, fuzzy by title; Return moves the caret to the heading (pushing the back stack, so ⌘[ returns) and reveals it. Works in single-file and folder mode.
- [ ] UI tests: ⇧⌘O, type part of a title, Return selects that heading; ⌘[ returns; no headings → "No headings", Return does nothing; the existing switcher tests still pass.

### Task 3: Export to HTML in focal-core

**Files:** Create `crates/focal-core/src/export.rs`.

**Produces:** `pub enum Embed<'a> { DisplayMath(&'a str), InlineMath(&'a str), Diagram(&'a str), WikiLink(&'a str) }`; `pub fn to_html(text: &str, render: &mut dyn FnMut(Embed) -> Option<String>) -> String` (an HTML fragment); `pub fn page(title: &str, body: &str) -> String` (a full document with inline CSS).

- Front matter is skipped; `==x==` becomes `<mark>x</mark>`; Mermaid fenced blocks and math go through `render` (math wrapped in `<span class="math">`/`<div class="math">`); `None` falls back to escaped source; wiki links use `render(WikiLink(name))` for the `href`, `name.md` when `None`; alerts get `<div class="alert alert-note">`; task list checkboxes are disabled inputs.
- [ ] Tests: headings/emphasis/table round out; front matter dropped; highlight; math and diagram go through the callback and fall back; wiki link href; raw HTML passes through (documented); page has title escaped and CSS.

### Task 4: Export as HTML… and Copy as HTML

**Files:** Create `crates/focal/src/export.rs` (app side: the render callback with MathJax and merman, the save flow, image path rewriting); modify `editor.rs` (actions), `mac.rs` (`copy_html(html, plain)`), `menus.rs`, `Cargo.toml` (`NSPasteboard` feature), `ui_tests.rs`.

- `ExportHtml` (⇧⌘E): save panel in the document's folder named `<stem>.html` (or `Untitled.html`), writes `page(title, body)`. `CopyHtml` (⌥⇧⌘C): the selection, or the whole document when nothing is selected, as HTML and plain text.
- Image `src` relative paths: kept when exporting beside the document, otherwise made absolute `file://` URLs.
- [ ] Tests: `export_html(document, text, destination)` writes a file with typeset math SVG and diagram SVG; a relative image becomes absolute when exported elsewhere; Copy as HTML's fragment for a selection.

### Task 5: Docs and manual pass

- `docs/design.md` (section 3 documents, M8 row, section 10: printing and PDF stay open; until decided, an exported page can be printed or saved as PDF from a browser), README shortcuts; `mise run check`; manual pass; merge to `main`, push, `mise run clean`.
