# Milestone 0 — TextKit Engine Spike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build one throwaway AppKit app that runs the same Markdown document on TextKit 1 and on TextKit 2. Measure both against the Milestone 0 cases and record which engine Focal is built on.

**Architecture:** A small Rust library (`focal-markdown`) wraps `pulldown-cmark` and returns element spans with UTF-16 offsets through a C interface. A Swift package links it statically. Engine-neutral Swift code (`SpikeCore`) turns spans into styling, marker visibility, dirty ranges and table models, and is unit tested. The `Spike` app puts that code behind an `EditorEngine` protocol with two implementations:

- **TK1Engine** hides characters with null glyphs and overlays table grids as subviews.
- **TK2Engine** hides markers with a near-zero font and swaps each table for an attachment view.

A benchmark mode measures both engines in a real window and prints JSON.

**Tech Stack:** Rust 1.97 (stable, edition 2024) with `pulldown-cmark` 0.13; Swift 6.4 (language mode 6) with SwiftPM tools 6.2; AppKit with TextKit 1 and TextKit 2; Swift Testing; mise for tasks.

**Spec:** [`docs/design.md`](../../design.md), section 4 ("Text engine choice: Milestone 0") and section 5 (tables). Accepted decisions: [native AppKit](../../../decisions/2026-09-28_221517960_build-focal-as-a-native-appkit-application.md), [source text as the document model](../../../decisions/2026-09-28_221517982_keep-the-markdown-source-as-the-document-model.md), [pulldown-cmark through Rust](../../../decisions/2026-09-28_221517990_parse-markdown-with-pulldown-cmark-through-a-rust-library.md).

## Global Constraints

- All spike code lives under `spike/` on branch `spike/m0-textkit`. It is **never merged into `main`**. Only the findings, the decision record and doc updates (Task 11) land on `main`.
- Minimum platform: macOS 26 (`.macOS("26.0")`); Swift language mode 6 with strict concurrency; SwiftPM tools version 6.2.
- `pulldown-cmark = { version = "0.13", default-features = false }`. No other Rust dependencies.
- No web views. No third-party Swift packages.
- The spike never writes files. Documents are in-memory fixtures, so the exact round-trip invariant cannot be violated on disk. The benchmark still checks that hiding markers never changes the text (`sourceIntact`, `revealKeepsSource`).
- Keystroke budget from the spec: **restyling after a keystroke in under 8 ms** on the 5,000-line fixture on Niklas's Mac.
- On the TextKit 2 path, never touch `NSTextView.layoutManager`. Accessing it silently switches the view to TextKit 1. The benchmark reports `textKit2Active` to catch this.
- Run shell commands from `spike/` unless a step says otherwise. Prefix with `rtk` where installed.
- Conventional commit messages with the scope `spike`, each ending with the attribution line from the session's instructions.
- Disk space is tight on this Mac. Run `mise run clean` in `spike/` when the spike is finished (Task 11).

## Review Focus

These inputs are implied by the spec but not covered by the Milestone 0 checks. They are the ones most likely to bite a real user. Each has a check in the task that owns the code.

1. **Table at the very end of a file without a trailing newline.** The grid must cover the whole table, and text after it must not be swallowed. Task 7 tests `hiddenRange` for no newline, LF and CRLF.
2. **CRLF line endings.** Heading markers must hide without joining lines, and table line breaks must be kept. Task 1 tests headings with `\r\n`; Task 7 tests CRLF tables.
3. **Input-method composition inside formatted text** (dead keys, Japanese input). Composition must work and restyling must wait until it is committed. Task 8 skips restyling while `hasMarkedText()`, with a manual check in Tasks 8, 9 and 11.
4. **Undo after a grid edit.** ⌘Z restores the exact previous table source, and the grid shows the old values. Task 10 measures `undoRestoresTable`; Tasks 8 and 9 check it by hand.
5. **Keyboard caret moving into a table's hidden source** (arrow down from the line above a table). The caret must not disappear into invisible text. Tasks 8 and 9 have manual checks, and Task 11 records the behavior for each engine.

## Background for the implementer

- **Spans.** `focal-markdown` returns one span per styled element: `kind`, `range` (the whole element, markers included), `content` (the part between markers) and `level` (heading level). Markers are `range` minus `content`, for example `**` on both sides of `**bold**`. `pulldown-cmark` reports byte ranges; the library converts them to UTF-16 because `NSString` and `NSRange` count UTF-16 code units.
- **Reveal rule.** A span's markers are hidden unless the selection *touches* the span, meaning the caret is inside it or directly at either edge. So the caret can never sit inside hidden text: the moment it reaches a span, that span's markers appear.
- **Islands.** A table is drawn as a grid view in place of its source lines. The source stays in the text storage; only its display changes. A grid edit rewrites the table's source through `shouldChangeText` / `didChangeText`, which makes it undoable.
- **Measured before planning** (scratch build on 2026-09-29, release mode): parsing the 5,000-line, 270 KB fixture and converting offsets takes about 4 ms, and Rust parsing alone about 3.5 ms. Full reparse therefore uses about half the keystroke budget. If the benchmark exceeds 8 ms, record by how much. Incremental reparsing is Milestone 1 work, not spike work.

## File Structure

```text
spike/
  README.md                         what this is and how to run it
  .gitignore                        SwiftPM and Cargo output
  mise.toml                         rust, test, app, bench, clean tasks
  Package.swift                     SwiftPM package; links the Rust static library
  focal-markdown/
    Cargo.toml                      staticlib + rlib, pulldown-cmark only
    src/lib.rs                      module wiring and exports
    src/spans.rs                    pulldown-cmark events → spans (byte ranges)
    src/utf16.rs                    byte offsets → UTF-16 offsets
    src/ffi.rs                      C interface: focal_parse, focal_spans_free
  Sources/CFocalMarkdown/
    module.modulemap                exposes the C header, links focal_markdown
    focal_markdown.h                C declarations matching ffi.rs
  Sources/SpikeCore/                engine-neutral, unit tested
    Span.swift                      SpanKind, Span
    MarkdownParser.swift            Swift wrapper around focal_parse
    Fixture.swift                   sample document and 5,000-line fixture
    MarkerVisibility.swift          which markers are hidden for a selection
    Styler.swift                    Theme and attribute styling
    DirtyRange.swift                what to restyle after an edit
    TableModel.swift                TableIsland, table extraction and serialization
    Stats.swift                     benchmark percentiles
  Sources/Spike/                    the app; checked by running it
    main.swift                      NSApplication bootstrap
    App.swift                       LaunchOptions, MainMenu, AppDelegate
    EditorEngine.swift              EditorEngine and TableHost protocols, TextViewFactory
    EditorController.swift          parse → style → hide → islands; grid edits
    TableGridView.swift             the editable grid
    TK1Engine.swift                 TextKit 1 engine
    TK2Engine.swift                 TextKit 2 engine, TableAttachment, TableViewProvider
    Bench.swift                     benchmark mode and JSON report
  Tests/SpikeCoreTests/
    ParserTests.swift, MarkerVisibilityTests.swift, StylerTests.swift,
    DirtyRangeTests.swift, TableModelTests.swift, StatsTests.swift
```

On `main` (Task 11 only): `docs/spikes/2026-09-29-m0-textkit.md`, one new record in `decisions/`, edits to `docs/design.md` and `AGENTS.md`.

---

### Task 1: Spike workspace and span extraction in Rust

**Files:**
- Create: `spike/README.md`, `spike/.gitignore`, `spike/mise.toml`
- Create: `spike/focal-markdown/Cargo.toml`, `spike/focal-markdown/src/lib.rs`, `spike/focal-markdown/src/spans.rs` (tests inline)

**Interfaces:**
- Consumes: nothing.
- Produces: `focal_markdown::parse(source: &str) -> Vec<Span>`. `Span { kind: SpanKind, range: Range<usize>, content: Range<usize>, level: u32 }` with UTF-8 byte ranges, sorted by `range.start`, outer spans first. `SpanKind` values: Heading=1, Emphasis=2, Strong=3, Strikethrough=4, Code=5, Link=6, CodeBlock=7, BlockQuote=8, ListItem=9, Table=10, TableHead=11, TableRow=12, TableCell=13. Heading ranges exclude their line ending. An element without children has `content = range.end..range.end`.

- [ ] **Step 1: Create the spike branch and workspace files**

From the repository root:

```bash
git switch main && git pull --ff-only
git switch -c spike/m0-textkit
mkdir -p spike/focal-markdown/src
```

`spike/README.md`:

```markdown
# Milestone 0 spike (throwaway)

Compares TextKit 1 and TextKit 2 for Focal's editor. See the plan in
`docs/superpowers/plans/2026-09-29-m0-textkit-spike.md` on `main`. This code is never merged.

    mise run test                          # Rust and Swift unit tests
    mise run app -- --engine tk1           # sample document on TextKit 1
    mise run app -- --engine tk2 --fixture big
    mise run bench -- --engine tk2         # JSON measurements on stdout
    mise run clean                         # remove build output
```

`spike/.gitignore`:

```gitignore
.build/
build/
focal-markdown/target/
```

`spike/mise.toml`. Extra arguments after `--` are appended to the end of `run`, so the executable must come last:

```toml
[tasks.rust]
description = "Build the focal-markdown static library"
run = "cargo build --release --manifest-path focal-markdown/Cargo.toml"

[tasks.test]
description = "Run the Rust and Swift unit tests"
depends = ["rust"]
run = [
  "cargo test --manifest-path focal-markdown/Cargo.toml",
  "cargo clippy --manifest-path focal-markdown/Cargo.toml --all-targets -- -D warnings",
  "swift test",
]

[tasks.app]
description = "Run the spike app, for example: mise run app -- --engine tk1 --fixture big"
depends = ["rust"]
run = 'swift build --product Spike && "$(swift build --show-bin-path)/Spike"'

[tasks.bench]
description = "Benchmark one engine on the big fixture: mise run bench -- --engine tk2"
depends = ["rust"]
run = 'swift build -c release --product Spike && "$(swift build -c release --show-bin-path)/Spike" --fixture big --bench'

[tasks.clean]
description = "Remove spike build output"
run = ["cargo clean --manifest-path focal-markdown/Cargo.toml", "rm -rf .build build"]
```

`spike/focal-markdown/Cargo.toml`:

```toml
[package]
name = "focal-markdown"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
crate-type = ["staticlib", "rlib"]

[dependencies]
pulldown-cmark = { version = "0.13", default-features = false }
```

`spike/focal-markdown/src/lib.rs` (Task 2 extends it):

```rust
//! Markdown span extraction for the Focal Milestone 0 spike.

mod spans;

pub use spans::{Span, SpanKind, parse};
```

- [ ] **Step 2: Write the span types, a stub parser and the failing tests**

`spike/focal-markdown/src/spans.rs`:

````rust
use std::ops::Range;


/// Element kinds the spike styles. Values are shared with Swift's `SpanKind`.
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    Heading = 1,
    Emphasis = 2,
    Strong = 3,
    Strikethrough = 4,
    Code = 5,
    Link = 6,
    CodeBlock = 7,
    BlockQuote = 8,
    ListItem = 9,
    Table = 10,
    TableHead = 11,
    TableRow = 12,
    TableCell = 13,
}

/// One element with UTF-8 byte ranges. Markers are `range` minus `content`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub kind: SpanKind,
    pub range: Range<usize>,
    pub content: Range<usize>,
    pub level: u32,
}

/// Parses `source` and returns styled elements sorted by start, outer elements first.
pub fn parse(_source: &str) -> Vec<Span> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(spans: &[Span], kind: SpanKind) -> &Span {
        spans.iter().find(|span| span.kind == kind).expect("span of kind")
    }

    #[test]
    fn strong_markers_surround_content() {
        let spans = parse("a **bold** b");
        let strong = find(&spans, SpanKind::Strong);
        assert_eq!(strong.range, 2..10);
        assert_eq!(strong.content, 4..8);
    }

    #[test]
    fn heading_excludes_line_ending() {
        let spans = parse("## Hi\r\nnext\r\n");
        let heading = find(&spans, SpanKind::Heading);
        assert_eq!((heading.range.clone(), heading.content.clone(), heading.level), (0..5, 3..5, 2));
    }

    #[test]
    fn inline_code_content_excludes_backticks() {
        let spans = parse("x `code` y");
        assert_eq!(find(&spans, SpanKind::Code).content, 3..7);
    }

    #[test]
    fn link_content_is_the_label() {
        let spans = parse("[link](http://x.y)");
        let link = find(&spans, SpanKind::Link);
        assert_eq!((link.range.clone(), link.content.clone()), (0..18, 1..5));
    }

    #[test]
    fn unclosed_fence_runs_to_end_of_file() {
        let source = "```\ncode\n";
        let block = find(&parse(source), SpanKind::CodeBlock).clone();
        assert_eq!((block.range, block.content), (0..9, 4..9));
    }

    #[test]
    fn table_reports_rows_and_cells() {
        let source = "| A | B |\n|:--|--:|\n| 1 | 2 |\n";
        let spans = parse(source);
        let table = find(&spans, SpanKind::Table);
        assert_eq!(table.range, 0..30);
        let rows = spans.iter().filter(|s| matches!(s.kind, SpanKind::TableHead | SpanKind::TableRow)).count();
        let cells: Vec<&str> = spans
            .iter()
            .filter(|s| s.kind == SpanKind::TableCell)
            .map(|s| &source[s.content.clone()])
            .collect();
        assert_eq!(rows, 2);
        assert_eq!(cells, ["A", "B", "1", "2"]);
    }

    #[test]
    fn empty_table_cell_has_empty_content() {
        let source = "| A | B |\n|---|---|\n| x |   |\n";
        let spans = parse(source);
        let last_cell = spans.iter().rfind(|s| s.kind == SpanKind::TableCell).expect("cell");
        assert!(last_cell.content.is_empty());
    }

    #[test]
    fn offsets_are_utf8_bytes() {
        let spans = parse("naïve 🙂 **ü**\n");
        let strong = find(&spans, SpanKind::Strong);
        assert_eq!((strong.range.clone(), strong.content.clone()), (12..18, 14..16));
    }

    #[test]
    fn outer_spans_sort_before_inner_ones() {
        let spans = parse("> **a**\n");
        assert_eq!(spans.iter().map(|s| s.kind).collect::<Vec<_>>(), [SpanKind::BlockQuote, SpanKind::Strong]);
    }
}
````

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --manifest-path focal-markdown/Cargo.toml`
Expected: FAIL, `test result: FAILED. 0 passed; 9 failed`. The tests panic with `span of kind` or report mismatched ranges.

- [ ] **Step 4: Implement the parser**

In `spans.rs`, add `use pulldown_cmark::{Event, Options, Parser, Tag};` below `use std::ops::Range;`, and replace the stub `parse` function (with its doc comment) with:

```rust
struct Open {
    kind: Option<SpanKind>,
    level: u32,
    range: Range<usize>,
    first_child: Option<usize>,
    last_child: Option<usize>,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
}

fn classify(tag: &Tag<'_>) -> (Option<SpanKind>, u32) {
    match tag {
        Tag::Heading { level, .. } => (Some(SpanKind::Heading), *level as u32),
        Tag::Emphasis => (Some(SpanKind::Emphasis), 0),
        Tag::Strong => (Some(SpanKind::Strong), 0),
        Tag::Strikethrough => (Some(SpanKind::Strikethrough), 0),
        Tag::Link { .. } => (Some(SpanKind::Link), 0),
        Tag::CodeBlock(_) => (Some(SpanKind::CodeBlock), 0),
        Tag::BlockQuote(_) => (Some(SpanKind::BlockQuote), 0),
        Tag::Item => (Some(SpanKind::ListItem), 0),
        Tag::Table(_) => (Some(SpanKind::Table), 0),
        Tag::TableHead => (Some(SpanKind::TableHead), 0),
        Tag::TableRow => (Some(SpanKind::TableRow), 0),
        Tag::TableCell => (Some(SpanKind::TableCell), 0),
        _ => (None, 0),
    }
}

/// Records a direct child's extent on the innermost open element.
fn note_child(stack: &mut [Open], range: &Range<usize>) {
    if let Some(parent) = stack.last_mut() {
        parent.first_child.get_or_insert(range.start);
        parent.last_child = Some(range.end);
    }
}

/// Headings include their line ending; drop it so hiding markers never joins lines.
fn trim_line_ending(source: &str, range: Range<usize>) -> Range<usize> {
    let text = source.get(range.clone()).unwrap_or_default();
    let trimmed = text.trim_end_matches(['\n', '\r']);
    range.start..range.start + trimmed.len()
}

/// Inline code content sits between equal-length backtick runs.
fn code_content(source: &str, range: &Range<usize>) -> Range<usize> {
    let text = source.get(range.clone()).unwrap_or_default();
    let ticks = text.bytes().take_while(|byte| *byte == b'`').count();
    let start = range.start + ticks;
    let end = range.end.saturating_sub(ticks);
    if start <= end { start..end } else { range.end..range.end }
}

/// Parses `source` and returns styled elements sorted by start, outer elements first.
pub fn parse(source: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    for (event, range) in Parser::new_ext(source, options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                note_child(&mut stack, &range);
                let (kind, level) = classify(&tag);
                stack.push(Open { kind, level, range, first_child: None, last_child: None });
            }
            Event::End(_) => {
                let Some(open) = stack.pop() else { continue };
                let Some(kind) = open.kind else { continue };
                let range = if kind == SpanKind::Heading {
                    trim_line_ending(source, open.range)
                } else {
                    open.range
                };
                let content = match (open.first_child, open.last_child) {
                    (Some(first), Some(last)) => first..last.min(range.end),
                    _ => range.end..range.end,
                };
                spans.push(Span { kind, range, content, level: open.level });
            }
            Event::Code(_) => {
                note_child(&mut stack, &range);
                let content = code_content(source, &range);
                spans.push(Span { kind: SpanKind::Code, range, content, level: 0 });
            }
            _ => note_child(&mut stack, &range),
        }
    }
    spans.sort_by_key(|span| (span.range.start, std::cmp::Reverse(span.range.end)));
    spans
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path focal-markdown/Cargo.toml && cargo clippy --manifest-path focal-markdown/Cargo.toml --all-targets -- -D warnings`
Expected: `test result: ok. 9 passed`; clippy prints nothing.

- [ ] **Step 6: Commit**

```bash
git add spike/README.md spike/.gitignore spike/mise.toml spike/focal-markdown/Cargo.toml spike/focal-markdown/Cargo.lock spike/focal-markdown/src
git commit -m "feat(spike): extract Markdown spans with pulldown-cmark"
```

---

### Task 2: UTF-16 offsets and the C interface

**Files:**
- Create: `spike/focal-markdown/src/utf16.rs`, `spike/focal-markdown/src/ffi.rs` (tests inline)
- Modify: `spike/focal-markdown/src/lib.rs`

**Interfaces:**
- Consumes: `parse`, `Span`, `SpanKind` from Task 1.
- Produces: C symbols `focal_parse(const uint8_t *utf8, size_t len) -> FocalSpanList` and `focal_spans_free(FocalSpanList)`. `FocalSpan { u32 kind, level, start, end, content_start, content_end }` has **UTF-16 offsets**; `FocalSpanList { FocalSpan *spans; size_t len }`. Invalid UTF-8, a null pointer or `len == 0` yield `{ NULL, 0 }`, which is safe to free.

- [ ] **Step 1: Write the offset table stub and its failing tests**

Replace `lib.rs` with:

```rust
//! Markdown span extraction for the Focal Milestone 0 spike.

mod spans;
mod utf16;

pub use spans::{Span, SpanKind, parse};
```

`spike/focal-markdown/src/utf16.rs`:

```rust
//! Converts UTF-8 byte offsets to UTF-16 code-unit offsets in one pass.

/// Lookup table for a fixed set of byte offsets.
pub struct Utf16Offsets {
    bytes: Vec<usize>,
    units: Vec<usize>,
}

impl Utf16Offsets {
    /// Resolves every offset in `wanted`. Offsets must fall on character boundaries.
    pub fn new(_source: &str, _wanted: impl IntoIterator<Item = usize>) -> Self {
        Self { bytes: Vec::new(), units: Vec::new() }
    }

    /// The UTF-16 offset for `byte`, if it was requested.
    pub fn get(&self, byte: usize) -> Option<usize> {
        let index = self.bytes.binary_search(&byte).ok()?;
        self.units.get(index).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_surrogate_pairs_as_two_units() {
        let source = "a🙂b";
        let offsets = Utf16Offsets::new(source, [0, 1, 5, 6]);
        assert_eq!([0, 1, 5, 6].map(|byte| offsets.get(byte)), [Some(0), Some(1), Some(3), Some(4)]);
    }

    #[test]
    fn unrequested_offsets_are_absent() {
        assert_eq!(Utf16Offsets::new("abc", [1]).get(2), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path focal-markdown/Cargo.toml`
Expected: FAIL, `1 failed` (`counts_surrogate_pairs_as_two_units`). Dead-code warnings for `Utf16Offsets` are expected until Step 5.

- [ ] **Step 3: Implement the conversion**

Replace the stub `new` with:

```rust
    pub fn new(source: &str, wanted: impl IntoIterator<Item = usize>) -> Self {
        let mut bytes: Vec<usize> = wanted.into_iter().collect();
        bytes.sort_unstable();
        bytes.dedup();
        let mut units = Vec::with_capacity(bytes.len());
        let mut next = bytes.iter().peekable();
        let mut unit = 0;
        for (byte, character) in source.char_indices() {
            while next.next_if(|&&wanted| wanted <= byte).is_some() {
                units.push(unit);
            }
            if next.peek().is_none() {
                break;
            }
            unit += character.len_utf16();
        }
        units.resize(bytes.len(), unit);
        Self { bytes, units }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path focal-markdown/Cargo.toml`
Expected: `test result: ok. 11 passed`.

- [ ] **Step 5: Add the C interface with its tests**

`spike/focal-markdown/src/ffi.rs`:

```rust
//! C interface used by the Swift spike. Offsets are UTF-16 code units, as `NSString` uses.

use crate::spans::{Span, parse};
use crate::utf16::Utf16Offsets;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocalSpan {
    pub kind: u32,
    pub level: u32,
    pub start: u32,
    pub end: u32,
    pub content_start: u32,
    pub content_end: u32,
}

#[repr(C)]
pub struct FocalSpanList {
    pub spans: *mut FocalSpan,
    pub len: usize,
}

fn to_ffi(span: &Span, offsets: &Utf16Offsets) -> Option<FocalSpan> {
    let convert = |byte: usize| u32::try_from(offsets.get(byte)?).ok();
    Some(FocalSpan {
        kind: span.kind as u32,
        level: span.level,
        start: convert(span.range.start)?,
        end: convert(span.range.end)?,
        content_start: convert(span.content.start)?,
        content_end: convert(span.content.end)?,
    })
}

/// Parses `len` bytes of UTF-8. Invalid UTF-8 yields an empty list.
/// Free the result with `focal_spans_free`.
///
/// # Safety
/// `utf8` must be null or point to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn focal_parse(utf8: *const u8, len: usize) -> FocalSpanList {
    let empty = FocalSpanList { spans: std::ptr::null_mut(), len: 0 };
    if utf8.is_null() || len == 0 {
        return empty;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `utf8`.
    let bytes = unsafe { std::slice::from_raw_parts(utf8, len) };
    let Ok(source) = std::str::from_utf8(bytes) else { return empty };
    let spans = parse(source);
    let offsets = Utf16Offsets::new(
        source,
        spans.iter().flat_map(|s| [s.range.start, s.range.end, s.content.start, s.content.end]),
    );
    let spans: Box<[FocalSpan]> = spans.iter().filter_map(|span| to_ffi(span, &offsets)).collect();
    let len = spans.len();
    FocalSpanList { spans: Box::into_raw(spans).cast::<FocalSpan>(), len }
}

/// Releases a list returned by `focal_parse`.
///
/// # Safety
/// `list` must come from `focal_parse` and must not be freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn focal_spans_free(list: FocalSpanList) {
    if list.spans.is_null() {
        return;
    }
    let slice = std::ptr::slice_from_raw_parts_mut(list.spans, list.len);
    // SAFETY: `slice` was produced by `Box::into_raw` in `focal_parse`.
    drop(unsafe { Box::from_raw(slice) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_c_interface() {
        let source = "**b**";
        let list = unsafe { focal_parse(source.as_ptr(), source.len()) };
        let spans = unsafe { std::slice::from_raw_parts(list.spans, list.len) }.to_vec();
        unsafe { focal_spans_free(list) };
        assert_eq!(
            spans,
            [FocalSpan { kind: 3, level: 0, start: 0, end: 5, content_start: 2, content_end: 3 }]
        );
    }

    #[test]
    fn offsets_are_utf16_code_units() {
        let source = "naïve 🙂 **ü**\n";
        let list = unsafe { focal_parse(source.as_ptr(), source.len()) };
        let spans = unsafe { std::slice::from_raw_parts(list.spans, list.len) }.to_vec();
        unsafe { focal_spans_free(list) };
        let strong = spans.iter().find(|s| s.kind == 3).expect("strong span");
        assert_eq!((strong.start, strong.end, strong.content_start, strong.content_end), (9, 14, 11, 12));
    }

    #[test]
    fn invalid_utf8_yields_no_spans() {
        let bytes = [0xff_u8, 0xfe];
        let list = unsafe { focal_parse(bytes.as_ptr(), bytes.len()) };
        assert_eq!(list.len, 0);
        unsafe { focal_spans_free(list) };
    }
}
```

Replace `lib.rs` with:

```rust
//! Markdown span extraction for the Focal Milestone 0 spike.

mod ffi;
mod spans;
mod utf16;

pub use ffi::{FocalSpan, FocalSpanList, focal_parse, focal_spans_free};
pub use spans::{Span, SpanKind, parse};
```

- [ ] **Step 6: Run all Rust checks**

The Swift package arrives in Task 3, so run the Rust checks directly:
`cargo test --manifest-path focal-markdown/Cargo.toml && cargo clippy --manifest-path focal-markdown/Cargo.toml --all-targets -- -D warnings && cargo build --release --manifest-path focal-markdown/Cargo.toml`
Expected: `test result: ok. 14 passed`, no clippy output, and `focal-markdown/target/release/libfocal_markdown.a` exists.

- [ ] **Step 7: Commit**

```bash
git add spike/focal-markdown/src
git commit -m "feat(spike): expose spans with UTF-16 offsets through a C interface"
```

---

### Task 3: Swift package, parser wrapper and fixtures

**Files:**
- Create: `spike/Package.swift`, `spike/Sources/CFocalMarkdown/module.modulemap`, `spike/Sources/CFocalMarkdown/focal_markdown.h`
- Create: `spike/Sources/SpikeCore/Span.swift`, `spike/Sources/SpikeCore/MarkdownParser.swift`, `spike/Sources/SpikeCore/Fixture.swift`
- Test: `spike/Tests/SpikeCoreTests/ParserTests.swift`

**Interfaces:**
- Consumes: the C interface from Task 2.
- Produces: `SpanKind` (a `UInt32` raw-value enum with `isInline` and `isBlock`) and `Span { kind, range: NSRange, content: NSRange, level: Int }`. `MarkdownParser.parse(_ text: String) -> [Span]` returns spans with UTF-16 `NSRange`s. `Fixture.sample: String`; `Fixture.big() -> String` is 5,000 lines with 50 tables and 50 code blocks.

- [ ] **Step 1: Create the package and the C module**

`spike/Package.swift`. The executable target is added in Task 8:

```swift
// swift-tools-version: 6.2
import PackageDescription

let rustLibraryDirectory = Context.packageDirectory + "/focal-markdown/target/release"

let package = Package(
    name: "FocalSpike",
    platforms: [.macOS("26.0")],
    targets: [
        .systemLibrary(name: "CFocalMarkdown", path: "Sources/CFocalMarkdown"),
        .target(
            name: "SpikeCore",
            dependencies: ["CFocalMarkdown"],
            linkerSettings: [.unsafeFlags(["-L", rustLibraryDirectory])]
        ),
        .testTarget(name: "SpikeCoreTests", dependencies: ["SpikeCore"]),
    ]
)
```

`spike/Sources/CFocalMarkdown/module.modulemap`:

```text
module CFocalMarkdown [system] {
    header "focal_markdown.h"
    link "focal_markdown"
    export *
}
```

`spike/Sources/CFocalMarkdown/focal_markdown.h`. It must match `ffi.rs` field for field:

```c
#pragma once
#include <stddef.h>
#include <stdint.h>

/// One Markdown element. Offsets are UTF-16 code units, matching NSString.
/// Markers are [start, content_start) and [content_end, end).
typedef struct {
    uint32_t kind;
    uint32_t level;
    uint32_t start;
    uint32_t end;
    uint32_t content_start;
    uint32_t content_end;
} FocalSpan;

typedef struct {
    FocalSpan *spans;
    size_t len;
} FocalSpanList;

/// Parses UTF-8 text. Invalid UTF-8 yields an empty list. Free with focal_spans_free.
FocalSpanList focal_parse(const uint8_t *utf8, size_t len);
void focal_spans_free(FocalSpanList list);
```

- [ ] **Step 2: Write the failing tests**

`spike/Tests/SpikeCoreTests/ParserTests.swift`:

```swift
import Foundation
import Testing
@testable import SpikeCore

@Test func parserConvertsOffsetsToUTF16() {
    let strong = MarkdownParser.parse("naïve 🙂 **ü**\n").first { $0.kind == .strong }
    #expect(strong?.range == NSRange(location: 9, length: 5))
    #expect(strong?.content == NSRange(location: 11, length: 1))
}

@Test func parserHandlesEmptyInput() {
    #expect(MarkdownParser.parse("").isEmpty)
}

@Test func bigFixtureHasTheAdvertisedShape() {
    let text = Fixture.big()
    let spans = MarkdownParser.parse(text)
    #expect(text.split(separator: "\n", omittingEmptySubsequences: false).count == 5_001)
    #expect(spans.filter { $0.kind == .table }.count == 50)
    #expect(spans.filter { $0.kind == .codeBlock }.count == 50)
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `mise run rust && swift test`
Expected: FAIL to compile with `cannot find 'MarkdownParser' in scope` (and `Fixture`).

- [ ] **Step 4: Implement spans, the parser wrapper and the fixtures**

`spike/Sources/SpikeCore/Span.swift`:

```swift
import Foundation

/// Element kinds the spike styles. Raw values match `SpanKind` in focal-markdown.
public enum SpanKind: UInt32, Sendable {
    case heading = 1, emphasis, strong, strikethrough, code, link, codeBlock, blockQuote, listItem, table, tableHead, tableRow, tableCell

    public var isInline: Bool {
        switch self {
        case .emphasis, .strong, .strikethrough, .code, .link: true
        default: false
        }
    }

    public var isBlock: Bool {
        switch self {
        case .heading, .codeBlock, .blockQuote, .listItem, .table: true
        default: false
        }
    }
}

public struct Span: Equatable, Sendable {
    public let kind: SpanKind
    public let range: NSRange
    public let content: NSRange
    public let level: Int

    public init(kind: SpanKind, range: NSRange, content: NSRange, level: Int) {
        self.kind = kind
        self.range = range
        self.content = content
        self.level = level
    }
}
```

`spike/Sources/SpikeCore/MarkdownParser.swift`:

```swift
import CFocalMarkdown
import Foundation

/// Parses Markdown with focal-markdown. Offsets arrive as UTF-16 code units, matching `NSString`.
public enum MarkdownParser {
    public static func parse(_ text: String) -> [Span] {
        var source = text
        return source.withUTF8 { buffer in
            let list = focal_parse(buffer.baseAddress, buffer.count)
            defer { focal_spans_free(list) }
            return UnsafeBufferPointer(start: list.spans, count: list.len).compactMap { span in
                guard let kind = SpanKind(rawValue: span.kind) else { return nil }
                return Span(
                    kind: kind,
                    range: NSRange(location: Int(span.start), length: Int(span.end) - Int(span.start)),
                    content: NSRange(location: Int(span.content_start), length: Int(span.content_end) - Int(span.content_start)),
                    level: Int(span.level)
                )
            }
        }
    }
}
```

`spike/Sources/SpikeCore/Fixture.swift`:

````swift
import Foundation

/// Documents the spike opens. Everything stays in memory; the spike never writes files.
public enum Fixture {
    public static let sample = """
    # Focal M0 sample

    Plain text with **bold**, *italic*, `inline code`, ~~struck~~ and a [link](https://example.com).
    Unicode stays intact: naïve café, 日本語, 🙂 and **ümlaut bold**.

    ## A table

    | Name | Value | Notes |
    |:-----|------:|:-----:|
    | alpha | 1 | *first* |
    | beta | 22 | `code` |
    | gamma | 333 | last |

    Text after the table.

    ```swift
    let greeting = "Hello"
    print(greeting) // **not bold**
    ```

    > A quote with **emphasis** inside.

    - A list item
    - Another with `code`

    ### Heading three

    Final paragraph.

    """

    /// 5,000 lines: 50 sections of 100 lines, each with one 4-row table and one 8-line code block.
    public static func big() -> String {
        var lines: [String] = []
        for section in 1...50 {
            lines.append("## Section \(section)")
            lines.append("")
            lines.append("| Name | Value | Notes | Done |")
            lines.append("|:-----|------:|:-----:|------|")
            for row in 1...4 {
                lines.append("| item \(row) | \(section * row) | *note* \(row) | [x] |")
            }
            lines.append("")
            lines.append("```swift")
            for line in 1...8 {
                lines.append("let value\(line) = \(line) * \(section) // **not bold**")
            }
            lines.append("```")
            lines.append("")
            for paragraph in 1...40 {
                lines.append("Paragraph \(paragraph) of section \(section) has **bold**, *italic*, `code`, ~~gone~~ and a [link](https://example.com/\(paragraph)) — naïve 🙂.")
                lines.append("")
            }
        }
        return lines.joined(separator: "\n") + "\n"
    }
}
````

- [ ] **Step 5: Run the tests to verify they pass**

Run: `mise run test`
Expected: Rust `14 passed`, then Swift `Test run with 3 tests … passed`. If linking fails with `library 'focal_markdown' not found`, the Rust library was not built: run `mise run rust` and check that `rustLibraryDirectory` in `Package.swift` points at `focal-markdown/target/release`.

- [ ] **Step 6: Commit**

```bash
git add spike/Package.swift spike/Sources spike/Tests
git commit -m "feat(spike): parse Markdown from Swift through focal-markdown"
```

---

### Task 4: Marker visibility

**Files:**
- Create: `spike/Sources/SpikeCore/MarkerVisibility.swift`
- Test: `spike/Tests/SpikeCoreTests/MarkerVisibilityTests.swift`

**Interfaces:**
- Consumes: `Span`, `SpanKind`, `MarkdownParser`, `Fixture.sample`.
- Produces:
  - `MarkerVisibility.hiddenRanges(spans: [Span], selection: NSRange) -> [NSRange]`, sorted by location.
  - `MarkerVisibility.markers(of: Span) -> [NSRange]`.
  - `MarkerVisibility.changedRanges(old: [NSRange], new: [NSRange]) -> [NSRange]`, the symmetric difference.
  - `MarkerVisibility.hidesMarkers(of: SpanKind) -> Bool`, true for inline kinds, headings and code blocks.

- [ ] **Step 1: Write the failing tests**

`spike/Tests/SpikeCoreTests/MarkerVisibilityTests.swift`:

```swift
import Foundation
import Testing
@testable import SpikeCore

@Test func hidesInlineMarkersAwayFromCaret() {
    let spans = MarkdownParser.parse("a **bold** b")
    #expect(MarkerVisibility.hiddenRanges(spans: spans, selection: NSRange(location: 0, length: 0))
        == [NSRange(location: 2, length: 2), NSRange(location: 8, length: 2)])
}

@Test(arguments: [2, 5, 10])
func revealsMarkersWhenCaretTouchesSpan(caret: Int) {
    let spans = MarkdownParser.parse("a **bold** b")
    #expect(MarkerVisibility.hiddenRanges(spans: spans, selection: NSRange(location: caret, length: 0)).isEmpty)
}

@Test func headingMarkerHidesOnOtherLines() {
    let spans = MarkdownParser.parse("# Title\nbody\n")
    #expect(MarkerVisibility.hiddenRanges(spans: spans, selection: NSRange(location: 10, length: 0))
        == [NSRange(location: 0, length: 2)])
    #expect(MarkerVisibility.hiddenRanges(spans: spans, selection: NSRange(location: 7, length: 0)).isEmpty)
}

@Test func noCaretPositionTouchesAHiddenMarker() {
    let text = Fixture.sample
    let spans = MarkdownParser.parse(text)
    for caret in 0...(text as NSString).length {
        let hidden = MarkerVisibility.hiddenRanges(spans: spans, selection: NSRange(location: caret, length: 0))
        #expect(!hidden.contains { $0.location <= caret && caret <= NSMaxRange($0) }, "caret \(caret)")
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `swift test`
Expected: FAIL to compile with `cannot find 'MarkerVisibility' in scope`.

- [ ] **Step 3: Implement**

`spike/Sources/SpikeCore/MarkerVisibility.swift`:

```swift
import Foundation

/// Decides which syntax markers are hidden. A span's markers show while the selection touches it.
public enum MarkerVisibility {
    public static func hiddenRanges(spans: [Span], selection: NSRange) -> [NSRange] {
        spans
            .filter { hidesMarkers(of: $0.kind) && !touches(selection, $0.range) }
            .flatMap(markers(of:))
            .sorted { $0.location < $1.location }
    }

    public static func markers(of span: Span) -> [NSRange] {
        let lead = NSRange(location: span.range.location, length: span.content.location - span.range.location)
        let trailStart = NSMaxRange(span.content)
        let trail = NSRange(location: trailStart, length: NSMaxRange(span.range) - trailStart)
        return [lead, trail].filter { $0.length > 0 }
    }

    public static func changedRanges(old: [NSRange], new: [NSRange]) -> [NSRange] {
        Array(Set(old).symmetricDifference(Set(new)))
    }

    public static func hidesMarkers(of kind: SpanKind) -> Bool {
        kind.isInline || kind == .heading || kind == .codeBlock
    }

    static func touches(_ selection: NSRange, _ range: NSRange) -> Bool {
        selection.location <= NSMaxRange(range) && NSMaxRange(selection) >= range.location
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `swift test`
Expected: all tests pass, including `noCaretPositionTouchesAHiddenMarker`, which checks every caret position in the sample document.

- [ ] **Step 5: Commit**

```bash
git add spike/Sources/SpikeCore/MarkerVisibility.swift spike/Tests/SpikeCoreTests/MarkerVisibilityTests.swift
git commit -m "feat(spike): hide syntax markers away from the selection"
```

---

### Task 5: Styling

**Files:**
- Create: `spike/Sources/SpikeCore/Styler.swift`
- Test: `spike/Tests/SpikeCoreTests/StylerTests.swift`

**Interfaces:**
- Consumes: `Span`, `MarkerVisibility.markers(of:)`, `MarkerVisibility.hidesMarkers(of:)`.
- Produces:
  - `@MainActor struct Theme`, with `bodyFont`, `monoFont`, `textColor`, `markerColor`, `linkColor`, `quoteColor`, `codeBackground`, `baseAttributes` and `headingFont(level:)`. All colors are dynamic system colors, so dark mode works.
  - `@MainActor Styler.apply(spans: [Span], to: NSMutableAttributedString, in: NSRange, theme: Theme)`. It resets `range` to the base attributes and reapplies every intersecting span, outer spans first. It never changes characters.

- [ ] **Step 1: Write the failing test**

`spike/Tests/SpikeCoreTests/StylerTests.swift`:

```swift
import AppKit
import Testing
@testable import SpikeCore

@MainActor @Test func stylerCombinesHeadingAndEmphasis() {
    let text = "# Hi *b*\n"
    let storage = NSMutableAttributedString(string: text)
    let theme = Theme()
    Styler.apply(spans: MarkdownParser.parse(text), to: storage, in: NSRange(location: 0, length: storage.length), theme: theme)
    let font = storage.attribute(.font, at: 6, effectiveRange: nil) as? NSFont
    #expect(font?.pointSize == 28)
    #expect(font.map { NSFontManager.shared.traits(of: $0).contains(.italicFontMask) } == true)
    #expect(storage.attribute(.foregroundColor, at: 5, effectiveRange: nil) as? NSColor == theme.markerColor)
    #expect(storage.string == text)
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `swift test`
Expected: FAIL to compile with `cannot find 'Styler' in scope`.

- [ ] **Step 3: Implement**

`spike/Sources/SpikeCore/Styler.swift`:

```swift
import AppKit

@MainActor
public struct Theme {
    public var bodyFont = NSFont.systemFont(ofSize: 16)
    public var monoFont = NSFont.monospacedSystemFont(ofSize: 14, weight: .regular)
    public var textColor = NSColor.textColor
    public var markerColor = NSColor.tertiaryLabelColor
    public var linkColor = NSColor.linkColor
    public var quoteColor = NSColor.secondaryLabelColor
    public var codeBackground = NSColor.quaternarySystemFill

    public init() {}

    public var baseAttributes: [NSAttributedString.Key: Any] {
        [.font: bodyFont, .foregroundColor: textColor]
    }

    public func headingFont(level: Int) -> NSFont {
        let sizes: [CGFloat] = [28, 24, 20, 18, 16, 16]
        return NSFont.systemFont(ofSize: sizes[min(max(level, 1), 6) - 1], weight: .bold)
    }
}

@MainActor
public enum Styler {
    public static func apply(spans: [Span], to storage: NSMutableAttributedString, in range: NSRange, theme: Theme) {
        let range = NSIntersectionRange(range, NSRange(location: 0, length: storage.length))
        guard range.length > 0 else { return }
        storage.setAttributes(theme.baseAttributes, range: range)
        let relevant = spans
            .filter { NSIntersectionRange($0.range, range).length > 0 }
            .sorted { ($0.range.location, -$0.range.length) < ($1.range.location, -$1.range.length) }
        for span in relevant {
            let target = NSIntersectionRange(span.range, range)
            switch span.kind {
            case .heading:
                storage.addAttribute(.font, value: theme.headingFont(level: span.level), range: target)
            case .strong:
                convert(storage, target, to: .boldFontMask)
            case .emphasis:
                convert(storage, target, to: .italicFontMask)
            case .strikethrough:
                storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: target)
            case .code, .codeBlock:
                storage.addAttributes([.font: theme.monoFont, .backgroundColor: theme.codeBackground], range: target)
            case .link:
                storage.addAttribute(.foregroundColor, value: theme.linkColor, range: target)
            case .blockQuote:
                storage.addAttribute(.foregroundColor, value: theme.quoteColor, range: target)
            case .listItem, .table, .tableHead, .tableRow, .tableCell:
                break
            }
            guard MarkerVisibility.hidesMarkers(of: span.kind) else { continue }
            for marker in MarkerVisibility.markers(of: span) {
                let visible = NSIntersectionRange(marker, range)
                if visible.length > 0 {
                    storage.addAttribute(.foregroundColor, value: theme.markerColor, range: visible)
                }
            }
        }
    }

    private static func convert(_ storage: NSMutableAttributedString, _ range: NSRange, to trait: NSFontTraitMask) {
        storage.enumerateAttribute(.font, in: range) { value, subrange, _ in
            guard let font = value as? NSFont else { return }
            storage.addAttribute(.font, value: NSFontManager.shared.convert(font, toHaveTrait: trait), range: subrange)
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `swift test`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add spike/Sources/SpikeCore/Styler.swift spike/Tests/SpikeCoreTests/StylerTests.swift
git commit -m "feat(spike): style Markdown spans with attributes"
```

---

### Task 6: Dirty ranges after an edit

**Files:**
- Create: `spike/Sources/SpikeCore/DirtyRange.swift`
- Test: `spike/Tests/SpikeCoreTests/DirtyRangeTests.swift`

**Interfaces:**
- Consumes: `Span`, `SpanKind.isBlock`, `MarkdownParser`.
- Produces:
  - `DirtyRange.compute(edited: NSRange, delta: Int, oldSpans: [Span], newSpans: [Span], text: NSString) -> NSRange`. `edited` and `delta` are exactly what `NSTextStorageDelegate.textStorage(_:didProcessEditing:range:changeInLength:)` passes, `oldSpans` come from before the edit, and `text` is the text after it.
  - `DirtyRange.shift(_:edited:delta:)`, which maps a pre-edit range into post-edit coordinates.

- [ ] **Step 1: Write the failing tests**

`spike/Tests/SpikeCoreTests/DirtyRangeTests.swift`:

````swift
import Foundation
import Testing
@testable import SpikeCore

@Test func typingInParagraphDirtiesOnlyThatParagraph() {
    let old = "one\n\ntwo\n\nthree\n"
    let new = "one\n\ntwxo\n\nthree\n"
    let dirty = DirtyRange.compute(
        edited: NSRange(location: 7, length: 1), delta: 1,
        oldSpans: MarkdownParser.parse(old), newSpans: MarkdownParser.parse(new), text: new as NSString)
    #expect(dirty == NSRange(location: 5, length: 5))
}

@Test func openingAFenceDirtiesToEndOfDocument() {
    let old = "intro\n\n``\nline\n\nlast\n"
    let new = "intro\n\n```\nline\n\nlast\n"
    let dirty = DirtyRange.compute(
        edited: NSRange(location: 9, length: 1), delta: 1,
        oldSpans: MarkdownParser.parse(old), newSpans: MarkdownParser.parse(new), text: new as NSString)
    #expect(dirty == NSRange(location: 7, length: (new as NSString).length - 7))
}

@Test func closingAFenceDirtiesTheOldBlock() {
    let old = "intro\n\n```\nline\n\nlast\n"
    let new = "intro\n\n``\nline\n\nlast\n"
    let dirty = DirtyRange.compute(
        edited: NSRange(location: 9, length: 0), delta: -1,
        oldSpans: MarkdownParser.parse(old), newSpans: MarkdownParser.parse(new), text: new as NSString)
    #expect(NSMaxRange(dirty) == (new as NSString).length)
}
````

- [ ] **Step 2: Run the tests to verify they fail**

Run: `swift test`
Expected: FAIL to compile with `cannot find 'DirtyRange' in scope`.

- [ ] **Step 3: Implement**

`spike/Sources/SpikeCore/DirtyRange.swift`:

```swift
import Foundation

/// Finds the text to restyle after an edit: the edited paragraphs plus any block that overlaps them,
/// before or after the edit (so opening or closing a code fence restyles everything it affects).
public enum DirtyRange {
    public static func shift(_ range: NSRange, edited: NSRange, delta: Int) -> NSRange {
        let oldEditEnd = NSMaxRange(edited) - delta
        func map(_ position: Int) -> Int {
            position >= oldEditEnd ? position + delta : min(position, NSMaxRange(edited))
        }
        let start = map(range.location)
        let end = map(NSMaxRange(range))
        return NSRange(location: start, length: max(0, end - start))
    }

    public static func compute(edited: NSRange, delta: Int, oldSpans: [Span], newSpans: [Span], text: NSString) -> NSRange {
        let whole = NSRange(location: 0, length: text.length)
        let clamped = NSIntersectionRange(edited, whole)
        var dirty = text.paragraphRange(for: NSRange(location: min(edited.location, text.length), length: clamped.length))
        let blocks = newSpans.filter(\.kind.isBlock).map(\.range)
            + oldSpans.filter(\.kind.isBlock).map { shift($0.range, edited: edited, delta: delta) }
        var grew = true
        while grew {
            grew = false
            for block in blocks where block.length > 0 && NSIntersectionRange(block, dirty).length > 0 {
                let union = NSUnionRange(dirty, block)
                if union != dirty {
                    dirty = union
                    grew = true
                }
            }
        }
        return NSIntersectionRange(dirty, whole)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `swift test`
Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add spike/Sources/SpikeCore/DirtyRange.swift spike/Tests/SpikeCoreTests/DirtyRangeTests.swift
git commit -m "feat(spike): restyle only the blocks an edit touches"
```

---

### Task 7: Table model

**Files:**
- Create: `spike/Sources/SpikeCore/TableModel.swift`
- Test: `spike/Tests/SpikeCoreTests/TableModelTests.swift`

**Interfaces:**
- Consumes: `Span`, `MarkdownParser`.
- Produces:
  - `TableIsland { range: NSRange, rows: [[String]] }` (Equatable, Sendable), with `columnCount`, `hiddenRange(in: NSString)`, `lineEnding(in:)` and `endsWithLineBreak(in:)`. `hiddenRange` is the table without its final line break.
  - `TableModel.islands(spans:text:) -> [TableIsland]`, sorted by location. Cell strings are Markdown source.
  - `TableModel.serialize(_ rows: [[String]], lineEnding: String, trailingNewline: Bool) -> String`. This is the spike's minimal, *unaligned* serializer; aligned output is Milestone 2. It escapes unescaped `|` exactly once.

- [ ] **Step 1: Write the failing tests** (Review Focus 1 and 2)

`spike/Tests/SpikeCoreTests/TableModelTests.swift`:

```swift
import Foundation
import Testing
@testable import SpikeCore

@Test func extractsTableRowsAndCells() {
    let text = "| A | B |\n|:--|--:|\n| 1 | **2** |\n| x |   |\n"
    let islands = TableModel.islands(spans: MarkdownParser.parse(text), text: text as NSString)
    #expect(islands.map(\.rows) == [[["A", "B"], ["1", "**2**"], ["x", ""]]])
}

@Test(arguments: [
    ("| a |\n|---|\n| 1 |", 17),
    ("| a |\n|---|\n| 1 |\n", 17),
    ("| a |\r\n|---|\r\n| 1 |\r\n", 19),
])
func tableHiddenRangeKeepsFinalLineBreak(text: String, hiddenLength: Int) {
    let island = TableModel.islands(spans: MarkdownParser.parse(text), text: text as NSString).first
    #expect(island?.hiddenRange(in: text as NSString) == NSRange(location: 0, length: hiddenLength))
}

@Test func serializesTablesAndEscapesPipesOnce() {
    let rows = [["A", "B"], ["1", "a|b"], ["x", #"c\|d"#]]
    let source = TableModel.serialize(rows, lineEnding: "\n", trailingNewline: true)
    #expect(source == "| A | B |\n| --- | --- |\n| 1 | a\\|b |\n| x | c\\|d |\n")
}

@Test func serializedTablesParseBackToTheSameCells() {
    let rows = [["Name", "Value"], ["one", "**1**"], ["", "2"]]
    let source = TableModel.serialize(rows, lineEnding: "\n", trailingNewline: true)
    let islands = TableModel.islands(spans: MarkdownParser.parse(source), text: source as NSString)
    #expect(islands.first?.rows == rows)
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `swift test`
Expected: FAIL to compile with `cannot find 'TableModel' in scope`.

- [ ] **Step 3: Implement**

`spike/Sources/SpikeCore/TableModel.swift`:

```swift
import Foundation

/// A table shown as a grid. `rows[0]` is the header; cell strings are Markdown source.
public struct TableIsland: Equatable, Sendable {
    public let range: NSRange
    public let rows: [[String]]

    public init(range: NSRange, rows: [[String]]) {
        self.range = range
        self.rows = rows
    }

    public var columnCount: Int { rows.map(\.count).max() ?? 0 }

    public func hiddenRange(in text: NSString) -> NSRange {
        var end = NSMaxRange(range)
        if end > range.location, text.character(at: end - 1) == 0x0A {
            end -= 1
            if end > range.location, text.character(at: end - 1) == 0x0D { end -= 1 }
        }
        return NSRange(location: range.location, length: end - range.location)
    }

    public func lineEnding(in text: NSString) -> String {
        text.substring(with: range).contains("\r\n") ? "\r\n" : "\n"
    }

    public func endsWithLineBreak(in text: NSString) -> Bool {
        range.length > 0 && text.character(at: NSMaxRange(range) - 1) == 0x0A
    }
}

public enum TableModel {
    public static func islands(spans: [Span], text: NSString) -> [TableIsland] {
        let byStart: (Span, Span) -> Bool = { $0.range.location < $1.range.location }
        return spans.filter { $0.kind == .table }.sorted(by: byStart).map { table in
            let inside = spans.filter { NSLocationInRange($0.range.location, table.range) }
            let rows = inside.filter { $0.kind == .tableHead || $0.kind == .tableRow }.sorted(by: byStart)
            let cells = inside.filter { $0.kind == .tableCell }.sorted(by: byStart)
            let grid = rows.map { row in
                cells.filter { NSLocationInRange($0.range.location, row.range) }.map { text.substring(with: $0.content) }
            }
            return TableIsland(range: table.range, rows: grid)
        }
    }

    public static func serialize(_ rows: [[String]], lineEnding: String, trailingNewline: Bool) -> String {
        let columns = rows.map(\.count).max() ?? 0
        func line(_ cells: [String]) -> String {
            let values = (0..<columns).map { index in
                index < cells.count ? escapePipes(cells[index]) : ""
            }
            return "| " + values.joined(separator: " | ") + " |"
        }
        var lines: [String] = []
        if let header = rows.first {
            lines.append(line(header))
            lines.append("|" + String(repeating: " --- |", count: columns))
        }
        lines += rows.dropFirst().map(line)
        return lines.joined(separator: lineEnding) + (trailingNewline ? lineEnding : "")
    }

    static func escapePipes(_ value: String) -> String {
        value.replacingOccurrences(of: #"(?<!\\)\|"#, with: #"\\|"#, options: .regularExpression)
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `mise run test`
Expected: Rust `14 passed`; Swift `Test run with 15 tests … passed`.

- [ ] **Step 5: Commit**

```bash
git add spike/Sources/SpikeCore/TableModel.swift spike/Tests/SpikeCoreTests/TableModelTests.swift
git commit -m "feat(spike): model Markdown tables as editable grids"
```

---

### Task 8: App shell, controller, grid view and the TextKit 1 engine

**Files:**
- Modify: `spike/Package.swift` (add the executable target)
- Create: `spike/Sources/Spike/main.swift`, `App.swift`, `EditorEngine.swift`, `EditorController.swift`, `TableGridView.swift`, `TK1Engine.swift`

**Interfaces:**
- Consumes: everything in `SpikeCore`.
- Produces:
  - `protocol EditorEngine`: `scrollView`, `textView`, `applyHidden(_:in:)`, `setIslands(_:dirty:host:)`, `focusCell(table:row:column:)`.
  - `protocol TableHost`: `commitCell(tableIndex:row:column:value:)`, `appendRow(tableIndex:)`, `setFocusIntent(tableIndex:row:column:)`.
  - `TextViewFactory.make(textKit2:) -> (NSScrollView, NSTextView)`.
  - `EditorController(engine:)`, with `load(_:)`, `spans`, `islands`, `restyleDurations: [Duration]` and `resetMetrics()`.
  - `TableGridView(rows:)`, with `update(rows:)`, `focus(row:column:)`, `onCommit`, `onAppendRow`, `onFocusIntent` and `nonisolated static func size(for:) -> NSSize`.
  - `LaunchOptions(arguments:)` with `engine: EngineKind` (`.tk1`/`.tk2`), `fixture` and `bench`.

This task's code is checked by building and running the app, because AppKit layout behavior cannot be unit tested meaningfully.

- [ ] **Step 1: Add the executable target**

In `spike/Package.swift`, add this line above the `.testTarget` line:

```swift
        .executableTarget(name: "Spike", dependencies: ["SpikeCore"]),
```

- [ ] **Step 2: Add the engine protocols and the text view factory**

`spike/Sources/Spike/EditorEngine.swift`:

```swift
import SpikeCore
import AppKit

/// What the controller needs from a text engine. TK1Engine and TK2Engine implement it.
@MainActor
protocol EditorEngine: AnyObject {
    var scrollView: NSScrollView { get }
    var textView: NSTextView { get }
    /// Hides markers inside `range`. `hidden` is the complete, sorted set for the document.
    func applyHidden(_ hidden: [NSRange], in range: NSRange)
    /// Replaces the table islands after a parse. `dirty` is the range just restyled.
    func setIslands(_ islands: [TableIsland], dirty: NSRange, host: TableHost)
    /// Focuses a grid cell, now or as soon as the grid view exists.
    func focusCell(table: Int, row: Int, column: Int)
}

/// Receives grid edits. EditorController implements it.
@MainActor
protocol TableHost: AnyObject {
    func commitCell(tableIndex: Int, row: Int, column: Int, value: String)
    func appendRow(tableIndex: Int)
    func setFocusIntent(tableIndex: Int, row: Int, column: Int)
}

@MainActor
enum TextViewFactory {
    static func make(textKit2: Bool) -> (NSScrollView, NSTextView) {
        let scrollView = NSScrollView(frame: NSRect(x: 0, y: 0, width: 900, height: 1000))
        scrollView.hasVerticalScroller = true
        scrollView.autoresizingMask = [.width, .height]
        let textView = NSTextView(usingTextLayoutManager: textKit2)
        textView.frame = NSRect(origin: .zero, size: scrollView.contentSize)
        textView.minSize = .zero
        textView.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.autoresizingMask = [.width]
        textView.textContainer?.widthTracksTextView = true
        textView.textContainerInset = NSSize(width: 48, height: 32)
        textView.allowsUndo = true
        textView.isRichText = false
        textView.usesFindBar = true
        textView.isAutomaticQuoteSubstitutionEnabled = false
        textView.isAutomaticDashSubstitutionEnabled = false
        textView.isAutomaticTextReplacementEnabled = false
        scrollView.documentView = textView
        return (scrollView, textView)
    }
}

extension NSRange {
    /// A Swift range for IndexSet operations; empty when the NSRange is invalid.
    var indexRange: Range<Int> { Range(self) ?? 0..<0 }
}
```

- [ ] **Step 3: Add the grid view**

`spike/Sources/Spike/TableGridView.swift`:

```swift
import SpikeCore
import AppKit

/// A grid of editable cells that stands in for a Markdown table.
@MainActor
final class TableGridView: NSView, NSTextFieldDelegate {
    nonisolated static let rowHeight: CGFloat = 30
    nonisolated static let columnWidth: CGFloat = 150

    nonisolated static func size(for rows: [[String]]) -> NSSize {
        let columns = rows.map(\.count).max() ?? 0
        return NSSize(width: CGFloat(columns) * columnWidth + 1, height: CGFloat(rows.count) * rowHeight + 1)
    }

    /// Called with (row, column, new source text) when a cell finishes editing.
    var onCommit: ((Int, Int, String) -> Void)?
    /// Called when Return is pressed in the last row.
    var onAppendRow: (() -> Void)?
    /// Called before focus moves to (row, column), so the host can restore it if the view is rebuilt.
    var onFocusIntent: ((Int, Int) -> Void)?

    private var fields: [[NSTextField]] = []

    override var isFlipped: Bool { true }

    init(rows: [[String]]) {
        super.init(frame: NSRect(origin: .zero, size: Self.size(for: rows)))
        update(rows: rows)
    }

    required init?(coder: NSCoder) { nil }

    func update(rows: [[String]]) {
        let shapeChanged = rows.count != fields.count || zip(rows, fields).contains { $0.count != $1.count }
        if shapeChanged {
            rebuild(rows)
        } else {
            for (rowIndex, row) in rows.enumerated() {
                for (column, value) in row.enumerated() where fields[rowIndex][column].currentEditor() == nil {
                    fields[rowIndex][column].stringValue = value
                }
            }
        }
        setFrameSize(Self.size(for: rows))
        needsDisplay = true
    }

    func focus(row: Int, column: Int) {
        guard fields.indices.contains(row), fields[row].indices.contains(column) else { return }
        window?.makeFirstResponder(fields[row][column])
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.quaternarySystemFill.setFill()
        NSRect(x: 0, y: 0, width: bounds.width, height: Self.rowHeight).fill()
        let path = NSBezierPath()
        for row in 0...fields.count {
            let y = CGFloat(row) * Self.rowHeight + 0.5
            path.move(to: NSPoint(x: 0, y: y))
            path.line(to: NSPoint(x: bounds.width, y: y))
        }
        let columns = fields.map(\.count).max() ?? 0
        for column in 0...columns {
            let x = CGFloat(column) * Self.columnWidth + 0.5
            path.move(to: NSPoint(x: x, y: 0))
            path.line(to: NSPoint(x: x, y: bounds.height))
        }
        NSColor.separatorColor.setStroke()
        path.lineWidth = 1
        path.stroke()
    }

    func controlTextDidEndEditing(_ notification: Notification) {
        guard let field = notification.object as? NSTextField, let (row, column) = position(of: field) else { return }
        onCommit?(row, column, field.stringValue)
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
        guard let field = control as? NSTextField, let (row, column) = position(of: field) else { return false }
        let lastRow = fields.count - 1
        switch selector {
        case #selector(NSResponder.insertTab(_:)):
            if column + 1 < fields[row].count {
                move(to: row, column + 1)
            } else if row < lastRow {
                move(to: row + 1, 0)
            }
            return true
        case #selector(NSResponder.insertBacktab(_:)):
            if column > 0 {
                move(to: row, column - 1)
            } else if row > 0 {
                move(to: row - 1, fields[row - 1].count - 1)
            }
            return true
        case #selector(NSResponder.insertNewline(_:)):
            if row == lastRow {
                window?.makeFirstResponder(self)
                onAppendRow?()
            } else {
                move(to: row + 1, column)
            }
            return true
        default:
            return false
        }
    }

    private func move(to row: Int, _ column: Int) {
        onFocusIntent?(row, column)
        focus(row: row, column: column)
    }

    private func position(of field: NSTextField) -> (Int, Int)? {
        for (row, cells) in fields.enumerated() {
            if let column = cells.firstIndex(of: field) { return (row, column) }
        }
        return nil
    }

    private func rebuild(_ rows: [[String]]) {
        fields.joined().forEach { $0.removeFromSuperview() }
        fields = rows.enumerated().map { rowIndex, row in
            row.enumerated().map { column, value in
                let field = NSTextField(string: value)
                field.isBordered = false
                field.drawsBackground = false
                field.focusRingType = .none
                field.font = rowIndex == 0 ? .boldSystemFont(ofSize: 14) : .systemFont(ofSize: 14)
                field.frame = NSRect(
                    x: CGFloat(column) * Self.columnWidth + 8,
                    y: CGFloat(rowIndex) * Self.rowHeight + 6,
                    width: Self.columnWidth - 16,
                    height: Self.rowHeight - 10
                )
                field.delegate = self
                addSubview(field)
                return field
            }
        }
    }
}
```

- [ ] **Step 4: Add the controller**

It parses in `didProcessEditing`, which runs before either engine lays out the edit. That way spans, hidden ranges and islands are current when TextKit asks for glyphs or paragraphs. It skips restyling while an input method is composing (Review Focus 3).

`spike/Sources/Spike/EditorController.swift`:

```swift
import SpikeCore
import AppKit

/// Owns parsing and styling. Engine-specific work goes through `EditorEngine`.
@MainActor
final class EditorController: NSObject {
    let engine: EditorEngine
    let theme = Theme()
    private(set) var spans: [Span] = []
    private(set) var islands: [TableIsland] = []
    private(set) var restyleDurations: [Duration] = []
    private var hidden: [NSRange] = []
    private var pendingFocus: (table: Int, row: Int, column: Int)?

    init(engine: EditorEngine) {
        self.engine = engine
        super.init()
        engine.textView.textStorage?.delegate = self
        engine.textView.delegate = self
    }

    private var storage: NSTextStorage {
        guard let storage = engine.textView.textStorage else { preconditionFailure("text view has no storage") }
        return storage
    }

    func load(_ text: String) {
        engine.textView.string = text
        engine.textView.setSelectedRange(NSRange(location: 0, length: 0))
    }

    func resetMetrics() {
        restyleDurations.removeAll()
    }

    /// Restyles `range` from the current spans and reapplies marker hiding there.
    private func restyle(_ range: NSRange) {
        Styler.apply(spans: spans, to: storage, in: range, theme: theme)
        engine.applyHidden(hidden, in: range)
    }

    private func replaceTable(_ index: Int, rows: [[String]], actionName: String) {
        let island = islands[index]
        let text = storage.string as NSString
        let source = TableModel.serialize(
            rows,
            lineEnding: island.lineEnding(in: text),
            trailingNewline: island.endsWithLineBreak(in: text)
        )
        let textView = engine.textView
        textView.breakUndoCoalescing()
        guard textView.shouldChangeText(in: island.range, replacementString: source) else { return }
        storage.replaceCharacters(in: island.range, with: source)
        textView.didChangeText()
        textView.undoManager?.setActionName(actionName)
        if let focus = pendingFocus {
            pendingFocus = nil
            engine.focusCell(table: focus.table, row: focus.row, column: focus.column)
        }
    }
}

extension EditorController: @preconcurrency NSTextStorageDelegate {
    func textStorage(
        _ textStorage: NSTextStorage,
        didProcessEditing editedMask: NSTextStorageEditActions,
        range editedRange: NSRange,
        changeInLength delta: Int
    ) {
        guard editedMask.contains(.editedCharacters) else { return }
        let start = ContinuousClock.now
        let text = textStorage.string
        let nsText = text as NSString
        let oldSpans = spans
        spans = MarkdownParser.parse(text)
        // The selection has not moved yet; the caret will land at the end of the edit.
        hidden = MarkerVisibility.hiddenRanges(spans: spans, selection: NSRange(location: NSMaxRange(editedRange), length: 0))
        let dirty = DirtyRange.compute(edited: editedRange, delta: delta, oldSpans: oldSpans, newSpans: spans, text: nsText)
        if !engine.textView.hasMarkedText() {
            restyle(dirty)
        }
        islands = TableModel.islands(spans: spans, text: nsText)
        engine.setIslands(islands, dirty: dirty, host: self)
        restyleDurations.append(ContinuousClock.now - start)
    }
}

extension EditorController: NSTextViewDelegate {
    func textViewDidChangeSelection(_ notification: Notification) {
        let textView = engine.textView
        guard !textView.hasMarkedText() else { return }
        let newHidden = MarkerVisibility.hiddenRanges(spans: spans, selection: textView.selectedRange())
        guard newHidden != hidden else { return }
        let affected = MarkerVisibility.changedRanges(old: hidden, new: newHidden)
        hidden = newHidden
        storage.beginEditing()
        for range in affected {
            restyle(range)
        }
        storage.endEditing()
    }
}

extension EditorController: TableHost {
    func commitCell(tableIndex: Int, row: Int, column: Int, value: String) {
        guard islands.indices.contains(tableIndex) else { return }
        var rows = islands[tableIndex].rows
        guard rows.indices.contains(row) else { return }
        while rows[row].count <= column { rows[row].append("") }
        guard rows[row][column] != value else {
            pendingFocus = nil
            return
        }
        rows[row][column] = value
        replaceTable(tableIndex, rows: rows, actionName: "Edit Table Cell")
    }

    func appendRow(tableIndex: Int) {
        guard islands.indices.contains(tableIndex) else { return }
        var rows = islands[tableIndex].rows
        rows.append(Array(repeating: "", count: islands[tableIndex].columnCount))
        pendingFocus = (tableIndex, rows.count - 1, 0)
        replaceTable(tableIndex, rows: rows, actionName: "Add Table Row")
    }

    func setFocusIntent(tableIndex: Int, row: Int, column: Int) {
        pendingFocus = (tableIndex, row, column)
    }
}
```

- [ ] **Step 5: Add the TextKit 1 engine**

The engine works like this:

- Hidden markers and all table characters except the final line break get null glyphs.
- Each table's first line is given the grid's height through a fixed line height, and its other lines collapse to 0.01 pt.
- A `TableGridView` subview is placed over the first line. Only grids within one screen of the viewport are positioned.

`spike/Sources/Spike/TK1Engine.swift`:

```swift
import SpikeCore
import AppKit

/// TextKit 1: hides characters with null glyphs and overlays table grids as subviews.
@MainActor
final class TK1Engine: NSObject, EditorEngine {
    let scrollView: NSScrollView
    let textView: NSTextView
    private var hiddenChars = IndexSet()
    private var islandChars = IndexSet()
    private var islands: [TableIsland] = []
    private var grids: [TableGridView] = []
    private weak var host: TableHost?
    private var positioningScheduled = false

    override init() {
        (scrollView, textView) = TextViewFactory.make(textKit2: false)
        super.init()
        layoutManager.delegate = self
        layoutManager.allowsNonContiguousLayout = true
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(visibleAreaChanged),
            name: NSView.boundsDidChangeNotification,
            object: scrollView.contentView
        )
    }

    private var layoutManager: NSLayoutManager {
        guard let layoutManager = textView.layoutManager else { preconditionFailure("TextKit 1 text view expected") }
        return layoutManager
    }

    func applyHidden(_ hidden: [NSRange], in range: NSRange) {
        hiddenChars = IndexSet()
        for marker in hidden { hiddenChars.insert(integersIn: marker.indexRange) }
        invalidate(range)
    }

    func setIslands(_ islands: [TableIsland], dirty: NSRange, host: TableHost) {
        self.host = host
        let text = textView.string as NSString
        let previous = self.islands
        self.islands = islands
        islandChars = IndexSet()
        for island in islands { islandChars.insert(integersIn: island.hiddenRange(in: text).indexRange) }
        for (index, island) in islands.enumerated() {
            let changed = index >= previous.count || previous[index] != island
            if changed || NSIntersectionRange(island.range, dirty).length > 0 {
                reserveSpace(for: island, text: text)
                invalidate(island.range)
            }
        }
        syncGrids()
        schedulePositioning()
    }

    func focusCell(table: Int, row: Int, column: Int) {
        guard grids.indices.contains(table) else { return }
        positionGrids()
        grids[table].focus(row: row, column: column)
    }

    private func invalidate(_ range: NSRange) {
        layoutManager.invalidateGlyphs(forCharacterRange: range, changeInLength: 0, actualCharacterRange: nil)
        layoutManager.invalidateLayout(forCharacterRange: range, actualCharacterRange: nil)
    }

    /// The table's first line grows to the grid's height; its other lines collapse.
    private func reserveSpace(for island: TableIsland, text: NSString) {
        guard let storage = textView.textStorage else { return }
        let height = TableGridView.size(for: island.rows).height + 16
        let first = text.paragraphRange(for: NSRange(location: island.range.location, length: 0))
        let tall = NSMutableParagraphStyle()
        tall.minimumLineHeight = height
        tall.maximumLineHeight = height
        storage.addAttribute(.paragraphStyle, value: tall, range: first)
        let restStart = NSMaxRange(first)
        let restEnd = NSMaxRange(island.range)
        guard restEnd > restStart else { return }
        let flat = NSMutableParagraphStyle()
        flat.minimumLineHeight = 0.01
        flat.maximumLineHeight = 0.01
        storage.addAttribute(.paragraphStyle, value: flat, range: NSRange(location: restStart, length: restEnd - restStart))
    }

    private func syncGrids() {
        while grids.count > islands.count { grids.removeLast().removeFromSuperview() }
        for (index, island) in islands.enumerated() {
            if index < grids.count {
                grids[index].update(rows: island.rows)
            } else {
                let grid = TableGridView(rows: island.rows)
                grid.onCommit = { [weak self] row, column, value in
                    self?.host?.commitCell(tableIndex: index, row: row, column: column, value: value)
                }
                grid.onAppendRow = { [weak self] in self?.host?.appendRow(tableIndex: index) }
                grid.onFocusIntent = { [weak self] row, column in
                    self?.host?.setFocusIntent(tableIndex: index, row: row, column: column)
                }
                grid.isHidden = true
                textView.addSubview(grid)
                grids.append(grid)
            }
        }
    }

    @objc private func visibleAreaChanged() {
        schedulePositioning()
    }

    private func schedulePositioning() {
        guard !positioningScheduled else { return }
        positioningScheduled = true
        DispatchQueue.main.async { [weak self] in
            self?.positioningScheduled = false
            self?.positionGrids()
        }
    }

    /// Places grids over their reserved line, laying out only the area around the viewport.
    private func positionGrids() {
        guard let container = textView.textContainer else { return }
        let origin = textView.textContainerOrigin
        let visible = textView.visibleRect.insetBy(dx: 0, dy: -textView.visibleRect.height)
        let glyphs = layoutManager.glyphRange(forBoundingRect: visible.offsetBy(dx: -origin.x, dy: -origin.y), in: container)
        let chars = layoutManager.characterRange(forGlyphRange: glyphs, actualGlyphRange: nil)
        for (index, island) in islands.enumerated() where grids.indices.contains(index) {
            let grid = grids[index]
            guard NSIntersectionRange(island.range, chars).length > 0 else {
                grid.isHidden = true
                continue
            }
            let glyph = layoutManager.glyphIndexForCharacter(at: island.range.location)
            let line = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
            grid.setFrameOrigin(NSPoint(x: line.minX + origin.x, y: line.minY + origin.y + 8))
            grid.isHidden = false
        }
    }
}

extension TK1Engine: @preconcurrency NSLayoutManagerDelegate {
    func layoutManager(
        _ layoutManager: NSLayoutManager,
        shouldGenerateGlyphs glyphs: UnsafePointer<CGGlyph>,
        properties props: UnsafePointer<NSLayoutManager.GlyphProperty>,
        characterIndexes charIndexes: UnsafePointer<Int>,
        font aFont: NSFont,
        forGlyphRange glyphRange: NSRange
    ) -> Int {
        let count = glyphRange.length
        var properties = Array(UnsafeBufferPointer(start: props, count: count))
        var changed = false
        for index in 0..<count {
            let character = charIndexes[index]
            if hiddenChars.contains(character) || islandChars.contains(character) {
                properties[index] = .null
                changed = true
            }
        }
        guard changed else { return 0 }
        properties.withUnsafeBufferPointer { buffer in
            guard let base = buffer.baseAddress else { return }
            layoutManager.setGlyphs(glyphs, properties: base, characterIndexes: charIndexes, font: aFont, forGlyphRange: glyphRange)
        }
        return count
    }

    func layoutManager(_ layoutManager: NSLayoutManager, didCompleteLayoutFor textContainer: NSTextContainer?, atEnd layoutFinishedFlag: Bool) {
        schedulePositioning()
    }
}
```

- [ ] **Step 6: Add the app entry point**

`spike/Sources/Spike/App.swift`. Task 9 adds the TK2 choice and Task 10 adds the benchmark:

```swift
import SpikeCore
import AppKit

enum EngineKind: String {
    case tk1, tk2
}

struct LaunchOptions {
    var engine: EngineKind = .tk2
    var fixture = "sample"
    var bench = false

    init(arguments: [String]) {
        var remaining = arguments.dropFirst().makeIterator()
        while let argument = remaining.next() {
            switch argument {
            case "--engine":
                if let value = remaining.next(), let kind = EngineKind(rawValue: value) { engine = kind }
            case "--fixture":
                if let value = remaining.next() { fixture = value }
            case "--bench":
                bench = true
            default:
                break
            }
        }
    }
}

@MainActor
enum MainMenu {
    static func make() -> NSMenu {
        let main = NSMenu()
        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)

        let editItem = NSMenuItem()
        let edit = NSMenu(title: "Edit")
        edit.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        let redo = edit.addItem(withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        edit.addItem(.separator())
        edit.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        edit.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        edit.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        edit.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = edit
        main.addItem(editItem)
        return main
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private let options: LaunchOptions
    private var window: NSWindow?
    private var controller: EditorController?

    init(options: LaunchOptions) {
        self.options = options
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.mainMenu = MainMenu.make()
        let engine: EditorEngine = TK1Engine()
        let controller = EditorController(engine: engine)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 1000),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Focal M0 — \(options.engine.rawValue)"
        window.contentView = engine.scrollView
        window.center()
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(engine.textView)
        NSApp.activate()
        self.window = window
        self.controller = controller

        controller.load(options.fixture == "big" ? Fixture.big() : Fixture.sample)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }
}
```

`spike/Sources/Spike/main.swift`:

```swift
import AppKit

let options = LaunchOptions(arguments: CommandLine.arguments)
let app = NSApplication.shared
let delegate = AppDelegate(options: options)
app.delegate = delegate
app.setActivationPolicy(.regular)
app.run()
```

- [ ] **Step 7: Build with no warnings**

Run: `mise run rust && swift build 2>&1 | grep -E 'warning|error|Build complete'`
Expected: `Build complete!` and no `warning:` or `error:` lines.

- [ ] **Step 8: Run the sample on TextKit 1 and check by hand**

Run: `mise run app -- --engine tk1`. A window titled "Focal M0 — tk1" opens with the sample document. Check each item and note the result (pass, fail or partial, plus one line of detail) in `spike/build/manual-tk1.md`, which is ignored and used in Task 11:

1. Arrow right through `**bold**`: the markers appear when the caret reaches the word and disappear after it leaves. The caret never moves without visibly moving. Then select the whole line, copy it and paste into TextEdit: the pasted text contains the `**` markers.
2. Click into the middle of `*italic*`: its markers appear.
3. Put the caret in the `## A table` heading: its `## ` appears; move away and it hides.
4. The code block's fences hide when the caret is outside the block.
5. The table shows as a grid with no leftover pipe characters or blank lines around it.
6. Click a cell, type, press Tab: the source changes (⌘A, ⌘C and paste into TextEdit to see), and the next cell is focused.
7. In the last row, press Return: a row is added and its first cell is focused.
8. ⌘Z undoes the cell edit and the grid shows the old value; ⌘⇧Z redoes it. Type a word in a paragraph and undo it too. *(Review Focus 4)*
9. Put the caret at the end of "Text after the table." and press ↑ until it passes the table. Note what happens: does the caret skip the table, vanish or get stuck? *(Review Focus 5)*
10. Type `ü` with the dead key (⌥U then U) inside `**ümlaut bold**`, and type Japanese Romaji if that input source is installed. Composition works. *(Review Focus 3)*
11. Switch System Settings to Dark: text, markers and grid stay readable.
12. Resize the window narrower and wider: text rewraps and the grid stays attached to its place.

Then run `mise run app -- --engine tk1 --fixture big` and scroll fast with the trackpad from top to bottom and back. Rate flicker and jumps from 1 (unusable) to 5 (flawless) and note what you saw.

- [ ] **Step 9: Commit**

```bash
git add spike/Package.swift spike/Sources/Spike
git commit -m "feat(spike): add the app shell and a TextKit 1 engine"
```

---

### Task 9: The TextKit 2 engine

**Files:**
- Create: `spike/Sources/Spike/TK2Engine.swift`
- Modify: `spike/Sources/Spike/App.swift` (engine selection)

**Interfaces:**
- Consumes: `EditorEngine`, `TableHost`, `TextViewFactory`, `TableGridView`, `TableIsland`.
- Produces: `TK2Engine` (conforms to `EditorEngine`), `TableAttachment` and `TableViewProvider`.

How it works:

- Markers get a 0.01 pt font and a clear color.
- For each table, the content storage delegate returns a paragraph holding one attachment in place of the table's first line.
- `textContentManager(_:shouldEnumerate:options:)` skips the table's other lines.
- When the number of tables changes, the island paragraphs are regenerated on the next run-loop turn, outside `processEditing`.

- [ ] **Step 1: Add the engine**

`spike/Sources/Spike/TK2Engine.swift`:

```swift
import SpikeCore
import AppKit

/// TextKit 2: hides markers with a near-zero font and replaces tables with attachment views.
@MainActor
final class TK2Engine: NSObject, EditorEngine {
    let scrollView: NSScrollView
    let textView: NSTextView
    private var islands: [TableIsland] = []
    private var islandIndexByStart: [Int: Int] = [:]
    /// Table lines after the first; their paragraphs are skipped during layout.
    private var collapsedRanges: [NSRange] = []
    private var grids: [Int: TableGridView] = [:]
    private weak var host: TableHost?
    private var pendingFocus: (table: Int, row: Int, column: Int)?
    private let hiddenFont = NSFont.systemFont(ofSize: 0.01)

    override init() {
        (scrollView, textView) = TextViewFactory.make(textKit2: true)
        super.init()
        contentStorage.delegate = self
    }

    private var contentStorage: NSTextContentStorage {
        guard let storage = textView.textContentStorage else { preconditionFailure("TextKit 2 text view expected") }
        return storage
    }

    func applyHidden(_ hidden: [NSRange], in range: NSRange) {
        guard let storage = textView.textStorage else { return }
        for marker in hidden {
            let target = NSIntersectionRange(marker, range)
            if target.length > 0 {
                storage.addAttributes([.font: hiddenFont, .foregroundColor: NSColor.clear], range: target)
            }
        }
    }

    func setIslands(_ islands: [TableIsland], dirty: NSRange, host: TableHost) {
        self.host = host
        let text = textView.string as NSString
        let countChanged = islands.count != self.islands.count
        self.islands = islands
        islandIndexByStart = [:]
        collapsedRanges = []
        for (index, island) in islands.enumerated() {
            islandIndexByStart[island.range.location] = index
            let first = text.paragraphRange(for: NSRange(location: island.range.location, length: 0))
            let rest = NSRange(location: NSMaxRange(first), length: max(0, NSMaxRange(island.range) - NSMaxRange(first)))
            if rest.length > 0 { collapsedRanges.append(rest) }
        }
        // A table that appeared or disappeared outside the edited paragraph needs its elements rebuilt.
        if countChanged {
            let ranges = islands.map(\.range)
            DispatchQueue.main.async { [weak self] in self?.regenerate(ranges) }
        }
    }

    func focusCell(table: Int, row: Int, column: Int) {
        pendingFocus = (table, row, column)
        DispatchQueue.main.async { [weak self] in
            guard let self, let focus = self.pendingFocus, let grid = self.grids[focus.table], grid.window != nil else { return }
            self.pendingFocus = nil
            grid.focus(row: focus.row, column: focus.column)
        }
    }

    /// Called by `TableViewProvider` when it creates a grid view.
    func didLoad(_ grid: TableGridView, index: Int) {
        grids[index] = grid
        grid.onCommit = { [weak self] row, column, value in
            self?.host?.commitCell(tableIndex: index, row: row, column: column, value: value)
        }
        grid.onAppendRow = { [weak self] in self?.host?.appendRow(tableIndex: index) }
        grid.onFocusIntent = { [weak self] row, column in
            self?.host?.setFocusIntent(tableIndex: index, row: row, column: column)
        }
        if let focus = pendingFocus, focus.table == index {
            pendingFocus = nil
            DispatchQueue.main.async { grid.focus(row: focus.row, column: focus.column) }
        }
    }

    private func regenerate(_ ranges: [NSRange]) {
        guard let storage = textView.textStorage else { return }
        storage.beginEditing()
        for range in ranges where NSMaxRange(range) <= storage.length {
            storage.edited(.editedAttributes, range: range, changeInLength: 0)
        }
        storage.endEditing()
    }

    fileprivate func island(at location: Int) -> (TableIsland, Int)? {
        guard let index = islandIndexByStart[location] else { return nil }
        return (islands[index], index)
    }

    fileprivate func isCollapsed(_ location: Int) -> Bool {
        collapsedRanges.contains { NSLocationInRange(location, $0) }
    }
}

extension TK2Engine: @preconcurrency NSTextContentStorageDelegate {
    func textContentStorage(_ textContentStorage: NSTextContentStorage, textParagraphWith range: NSRange) -> NSTextParagraph? {
        guard let (island, index) = island(at: range.location) else { return nil }
        let attachment = TableAttachment(island: island, index: index, engine: self)
        let paragraph = NSMutableAttributedString(attributedString: NSAttributedString(attachment: attachment))
        paragraph.append(NSAttributedString(string: "\n"))
        return NSTextParagraph(attributedString: paragraph)
    }

    func textContentManager(
        _ textContentManager: NSTextContentManager,
        shouldEnumerate textElement: NSTextElement,
        options: NSTextContentManager.EnumerationOptions
    ) -> Bool {
        guard let elementRange = textElement.elementRange else { return true }
        let location = textContentManager.offset(from: textContentManager.documentRange.location, to: elementRange.location)
        return !isCollapsed(location)
    }
}

final class TableAttachment: NSTextAttachment {
    let island: TableIsland
    let index: Int
    weak var engine: TK2Engine?

    init(island: TableIsland, index: Int, engine: TK2Engine) {
        self.island = island
        self.index = index
        self.engine = engine
        super.init(data: nil, ofType: nil)
        allowsTextAttachmentView = true
    }

    required init?(coder: NSCoder) { nil }

    override func viewProvider(for parentView: NSView?, location: NSTextLocation, textContainer: NSTextContainer?) -> NSTextAttachmentViewProvider? {
        let provider = TableViewProvider(
            textAttachment: self,
            parentView: parentView,
            textLayoutManager: textContainer?.textLayoutManager,
            location: location
        )
        provider.tracksTextAttachmentViewBounds = true
        return provider
    }
}

final class TableViewProvider: NSTextAttachmentViewProvider {
    override func loadView() {
        // TextKit calls this on the main thread; the SDK does not declare it main-actor isolated.
        nonisolated(unsafe) let provider = self
        MainActor.assumeIsolated {
            guard let attachment = provider.textAttachment as? TableAttachment else { return }
            let grid = TableGridView(rows: attachment.island.rows)
            attachment.engine?.didLoad(grid, index: attachment.index)
            provider.view = grid
        }
    }

    override func attachmentBounds(
        for attributes: [NSAttributedString.Key: Any],
        location: NSTextLocation,
        textContainer: NSTextContainer?,
        proposedLineFragment: CGRect,
        position: CGPoint
    ) -> CGRect {
        guard let attachment = textAttachment as? TableAttachment else { return .zero }
        return CGRect(origin: .zero, size: TableGridView.size(for: attachment.island.rows))
    }
}
```

- [ ] **Step 2: Let the app choose the engine**

In `App.swift`, replace

```swift
        let engine: EditorEngine = TK1Engine()
```

with

```swift
        let engine: EditorEngine = options.engine == .tk1 ? TK1Engine() : TK2Engine()
```

- [ ] **Step 3: Build with no warnings**

Run: `swift build 2>&1 | grep -E 'warning|error|Build complete'`
Expected: `Build complete!` only.

- [ ] **Step 4: Run the sample on TextKit 2 and check by hand**

Run: `mise run app -- --engine tk2`. Go through the same 12 checks and the big-fixture scroll rating from Task 8, Step 8, and write the results to `spike/build/manual-tk2.md`. Also note whether the grid keeps keyboard focus after Tab. The view can be rebuilt when its paragraph regenerates, and `focusCell` exists to restore focus.

- [ ] **Step 5: Commit**

```bash
git add spike/Sources/Spike/TK2Engine.swift spike/Sources/Spike/App.swift
git commit -m "feat(spike): add a TextKit 2 engine with attachment-view tables"
```

---

### Task 10: Benchmark mode

**Files:**
- Create: `spike/Sources/SpikeCore/Stats.swift`, `spike/Sources/Spike/Bench.swift`
- Test: `spike/Tests/SpikeCoreTests/StatsTests.swift`
- Modify: `spike/Sources/Spike/App.swift` (run the benchmark when `--bench` is passed)

**Interfaces:**
- Consumes: `EditorController` (`load`, `islands`, `restyleDurations`, `resetMetrics`, `appendRow`, `commitCell`), `Fixture.big()`.
- Produces: `Stats(_ values: [Double])` with `p50`, `p95` and `max`. `Bench(controller:window:).run(engine:) async -> BenchReport`, whose `json` property is pretty-printed with sorted keys.

The report fields:

| Field | Meaning | Good value |
| --- | --- | --- |
| `openMs` | Load the 5,000-line fixture and draw the first screen | lower is better |
| `sourceIntact` | Text storage equals the fixture after loading | `true` |
| `revealKeepsSource` | Text unchanged after moving the caret over 570 positions | `true` |
| `keystrokeMs` | Wall time of one typed character, including drawing | p95 < 16 |
| `restyleMs` | The controller's parse, style and hide work per edit | **p95 < 8 (budget)** |
| `scrollStepMs` | One 600 pt scroll step including drawing | p95 < 16 |
| `scrollDriftChars` | Largest change in the character at the top of the viewport when revisiting an offset | `0` |
| `heightChanges` | Times the document height changed while scrolling (TextKit 2 estimates heights) | low |
| `tableAboveDriftPt` | How far text above a table moved when a row was added | `0` |
| `tableBelowShiftPt` | How far text below it moved | about one row (30 pt) |
| `undoRestoresTable` | ⌘Z after a cell edit restores the exact source | `true` |
| `textKit2Active` | The text view still uses TextKit 2 at the end | `true` for tk2, `false` for tk1 |

- [ ] **Step 1: Write the failing test**

`spike/Tests/SpikeCoreTests/StatsTests.swift`:

```swift
import Foundation
import Testing
@testable import SpikeCore

@Test func statsReportsPercentiles() {
    let stats = Stats((1...100).map(Double.init))
    #expect([stats.p50, stats.p95, stats.max] == [51, 96, 100])
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `swift test`
Expected: FAIL to compile with `cannot find 'Stats' in scope`.

- [ ] **Step 3: Implement `Stats`**

`spike/Sources/SpikeCore/Stats.swift`:

```swift
import Foundation

/// Percentiles for benchmark samples, in milliseconds.
public struct Stats: Encodable, Equatable, Sendable {
    public let p50: Double
    public let p95: Double
    public let max: Double

    public init(_ values: [Double]) {
        let sorted = values.sorted()
        guard let last = sorted.last else {
            p50 = 0
            p95 = 0
            max = 0
            return
        }
        p50 = sorted[sorted.count / 2]
        p95 = sorted[Swift.min(sorted.count - 1, Int(Double(sorted.count) * 0.95))]
        max = last
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `mise run test`
Expected: Rust `14 passed`; Swift `Test run with 16 tests … passed`.

- [ ] **Step 5: Add the benchmark**

`spike/Sources/Spike/Bench.swift`:

```swift
import SpikeCore
import AppKit

struct BenchReport: Encodable {
    let engine: String
    let openMs: Double
    let sourceIntact: Bool
    let revealKeepsSource: Bool
    let keystrokeMs: Stats
    let restyleMs: Stats
    let scrollStepMs: Stats
    let scrollDriftChars: Int
    let heightChanges: Int
    let tableAboveDriftPt: Double
    let tableBelowShiftPt: Double
    let undoRestoresTable: Bool
    let textKit2Active: Bool

    var json: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(self) else { return "{}" }
        return String(decoding: data, as: UTF8.self)
    }
}

/// Measures one engine on the 5,000-line fixture in a real, visible window.
@MainActor
struct Bench {
    let controller: EditorController
    let window: NSWindow

    private var textView: NSTextView { controller.engine.textView }
    private var scrollView: NSScrollView { controller.engine.scrollView }

    func run(engine: String) async -> BenchReport {
        let source = Fixture.big()

        let openStart = ContinuousClock.now
        controller.load(source)
        window.displayIfNeeded()
        let openMs = Self.milliseconds(ContinuousClock.now - openStart)
        let sourceIntact = textView.string == source

        // Walk the caret across formatted text; hiding and revealing must not change the source.
        for location in stride(from: 0, to: min(4_000, (source as NSString).length), by: 7) {
            textView.setSelectedRange(NSRange(location: location, length: 0))
        }
        let revealKeepsSource = textView.string == source

        let (keystrokes, restyles) = await typeCharacters()
        let (scrollSteps, drift, heightChanges) = await scrollThrough()
        let (aboveDrift, belowShift) = await growTable(index: 10)
        let undoRestores = await editAndUndo(index: 10)

        return BenchReport(
            engine: engine,
            openMs: openMs,
            sourceIntact: sourceIntact,
            revealKeepsSource: revealKeepsSource,
            keystrokeMs: Stats(keystrokes),
            restyleMs: Stats(restyles),
            scrollStepMs: Stats(scrollSteps),
            scrollDriftChars: drift,
            heightChanges: heightChanges,
            tableAboveDriftPt: aboveDrift,
            tableBelowShiftPt: belowShift,
            undoRestoresTable: undoRestores,
            textKit2Active: textView.textLayoutManager != nil
        )
    }

    /// Types one character at the end of 20 paragraph lines spread through the document.
    private func typeCharacters() async -> ([Double], [Double]) {
        controller.resetMetrics()
        var wallTimes: [Double] = []
        for step in 0..<20 {
            let text = textView.string as NSString
            let probe = text.length * step / 20
            let found = text.range(of: "Paragraph", range: NSRange(location: probe, length: text.length - probe))
            guard found.location != NSNotFound else { continue }
            let line = text.lineRange(for: found)
            let caret = NSMaxRange(line) - 1
            textView.scrollRangeToVisible(line)
            window.displayIfNeeded()
            textView.setSelectedRange(NSRange(location: caret, length: 0))
            let start = ContinuousClock.now
            textView.insertText("x", replacementRange: textView.selectedRange())
            window.displayIfNeeded()
            wallTimes.append(Self.milliseconds(ContinuousClock.now - start))
            await settle()
        }
        return (wallTimes, controller.restyleDurations.map(Self.milliseconds))
    }

    /// Scrolls top to bottom in 600 pt steps, then revisits each offset to detect layout drift.
    private func scrollThrough() async -> ([Double], Int, Int) {
        textView.setSelectedRange(NSRange(location: 0, length: 0))
        let clip = scrollView.contentView
        let step: CGFloat = 600
        var times: [Double] = []
        var tops: [Int] = []
        var heightChanges = 0
        var lastHeight = textView.frame.height
        var offset: CGFloat = 0
        while offset < textView.frame.height - clip.bounds.height, tops.count < 2_000 {
            let start = ContinuousClock.now
            scroll(to: offset)
            times.append(Self.milliseconds(ContinuousClock.now - start))
            tops.append(textView.characterIndexForInsertion(at: NSPoint(x: 80, y: offset + 40)))
            if textView.frame.height != lastHeight {
                heightChanges += 1
                lastHeight = textView.frame.height
            }
            offset += step
        }
        var drift = 0
        for (index, top) in tops.enumerated().reversed() {
            let revisit = CGFloat(index) * step
            scroll(to: revisit)
            drift = max(drift, abs(textView.characterIndexForInsertion(at: NSPoint(x: 80, y: revisit + 40)) - top))
        }
        await settle()
        return (times, drift, heightChanges)
    }

    /// Appends a row to a visible table. Text above must not move; text below moves by one row.
    private func growTable(index: Int) async -> (Double, Double) {
        guard controller.islands.indices.contains(index) else { return (.nan, .nan) }
        let island = controller.islands[index]
        textView.scrollRangeToVisible(island.range)
        window.displayIfNeeded()
        await settle()
        let text = textView.string as NSString
        let above = NSRange(location: island.range.location - 3, length: 1)
        let belowStart = text.range(of: "let value1", range: NSRange(location: NSMaxRange(island.range), length: text.length - NSMaxRange(island.range)))
        let aboveBefore = textView.firstRect(forCharacterRange: above, actualRange: nil).minY
        let belowBefore = textView.firstRect(forCharacterRange: belowStart, actualRange: nil).minY
        controller.appendRow(tableIndex: index)
        await settle()
        window.displayIfNeeded()
        let aboveAfter = textView.firstRect(forCharacterRange: above, actualRange: nil).minY
        let belowAfterRange = NSRange(location: belowStart.location + (controller.islands[index].range.length - island.range.length), length: belowStart.length)
        let belowAfter = textView.firstRect(forCharacterRange: belowAfterRange, actualRange: nil).minY
        return (Double(aboveAfter - aboveBefore), Double(belowAfter - belowBefore))
    }

    /// Edits a cell, then undoes; the source must return to its exact previous state.
    private func editAndUndo(index: Int) async -> Bool {
        guard controller.islands.indices.contains(index) else { return false }
        let before = textView.string
        controller.commitCell(tableIndex: index, row: 1, column: 0, value: "edited")
        await settle()
        let changed = textView.string != before
        textView.undoManager?.undo()
        await settle()
        return changed && textView.string == before
    }

    private func scroll(to offset: CGFloat) {
        let clip = scrollView.contentView
        clip.scroll(to: NSPoint(x: 0, y: offset))
        scrollView.reflectScrolledClipView(clip)
        window.displayIfNeeded()
    }

    /// Lets the run loop close undo groups and run deferred layout work.
    private func settle() async {
        try? await Task.sleep(for: .milliseconds(100))
    }

    static func milliseconds(_ duration: Duration) -> Double {
        let parts = duration.components
        return Double(parts.seconds) * 1_000 + Double(parts.attoseconds) / 1e15
    }
}
```

In `App.swift`, replace

```swift
        controller.load(options.fixture == "big" ? Fixture.big() : Fixture.sample)
```

with

```swift
        if options.bench {
            Task { @MainActor in
                let report = await Bench(controller: controller, window: window).run(engine: options.engine.rawValue)
                print(report.json)
                NSApp.terminate(nil)
            }
        } else {
            controller.load(options.fixture == "big" ? Fixture.big() : Fixture.sample)
        }
```

- [ ] **Step 6: Build and run one benchmark per engine**

Keep the Mac unlocked and don't touch the keyboard or trackpad while the window runs.

Run: `mise run bench -- --engine tk1` and then `mise run bench -- --engine tk2`
Expected: each prints one JSON object with every field from the table above, then quits within about a minute. If a run hangs, note the last visible state and stop it with ⌘Q. A hang is itself a finding for that engine.

- [ ] **Step 7: Commit**

```bash
git add spike/Sources/SpikeCore/Stats.swift spike/Tests/SpikeCoreTests/StatsTests.swift spike/Sources/Spike/Bench.swift spike/Sources/Spike/App.swift
git commit -m "feat(spike): benchmark both engines on the 5,000-line fixture"
```

---

### Task 11: Evaluate, decide and record

**Files (on `main`):**
- Create: `docs/spikes/2026-09-29-m0-textkit.md`
- Create: one decision record in `decisions/` via `vrdx new`
- Modify: `docs/design.md` (section 4, "Text engine choice: Milestone 0"), `AGENTS.md` ("Current state")

**Interfaces:**
- Consumes: the benchmark JSON and the manual notes from Tasks 8–10.
- Produces: an engine choice that Niklas can accept. Milestone 1 plans against it.

- [ ] **Step 1: Collect three benchmark runs per engine**

From `spike/`:

```bash
mkdir -p build
for run in 1 2 3; do
  mise run bench -- --engine tk1 > build/bench-tk1-$run.json
  mise run bench -- --engine tk2 > build/bench-tk2-$run.json
done
```

If the build output mixes into the files, strip everything before the first `{`. Use the median of the three runs for each number.

- [ ] **Step 2: Check VoiceOver on both engines**

Run `mise run app -- --engine tk1`, turn on VoiceOver (⌘F5) and let it read the first paragraphs and the table. Repeat with `tk2`. Record whether it reads the text in order, whether it reads the grid cells, and whether hidden markers are spoken.

- [ ] **Step 3: Apply the decision rule**

For each engine, go through these in order:

1. **Blockers:** manual checks 1–8 and 12 fail, `sourceIntact` or `revealKeepsSource` is `false`, `undoRestoresTable` is `false`, VoiceOver cannot read the text, or TK2's `textKit2Active` is `false`.
2. **Stability:** `scrollDriftChars` is not `0`, `tableAboveDriftPt` is not `0`, or the scroll rating is below 4.
3. **Budget:** `restyleMs` p95 is 8 ms or more.

Choose the engine with no blockers and the fewest stability problems; break ties on the budget, then on `keystrokeMs` and `scrollStepMs`. **If both engines have a blocker, do not choose.** Write up the findings and stop to discuss options with Niklas, for example TextKit 1 with a custom attachment layer, or TextKit 2 with custom layout fragments.

- [ ] **Step 4: Write the findings on `main`**

From the repository root: `git switch main`. Create `docs/spikes/2026-09-29-m0-textkit.md` with these sections, filled from the data. Where a measurement is missing, say why; don't leave a gap.

```markdown
# Milestone 0: TextKit 1 vs TextKit 2

Date, Mac model, macOS and Xcode versions (`sw_vers`, `xcodebuild -version`), spike branch and commit.

## Result
One paragraph: the chosen engine (or "no choice" and why), and the deciding evidence.

## Measurements
Table with one row per report field and columns TK1 median, TK2 median, good value.

## Manual checks
Table with one row per check 1–12 plus scroll rating and VoiceOver, and columns TK1 and TK2
(pass, fail or partial, with one line of detail each). Include Review Focus 3 and 5 observations.

## What surprised us
Bullets: behavior that differed from the plan's assumptions, for example whether null-glyphed
newlines still break lines in TK1, or whether TK2 recreated grid views on every edit.

## Consequences for Milestone 1
Bullets: what M1 must handle with this engine (caret behavior at tables, incremental parsing if
the budget was missed, known workarounds).
```

- [ ] **Step 5: Record the decision**

Write the body to `/tmp/m0-decision.md`:

- **Decision:** "Build the editor on TextKit N", with scope (the main text view; islands as described).
- **Context:** the Milestone 0 question, both approaches, the key measurements, a link to the findings file, and the spike branch and commit.
- **Consequences:** from the findings' last section, plus when to revisit (for example, a future macOS fixing a TK2 blocker).

Then run:

```bash
vrdx new "Build the editor on TextKit N" --tag architecture --tag rendering --body-file /tmp/m0-decision.md --json
```

Replace N with 1 or 2. Leave the status `proposed`; Niklas accepts it. In the new record's metadata, set `depends_on = ["01M3N1CC6YF89Y8YAY1QJW5ZWS"]` (the source-model decision), then run `vrdx validate --json` and expect `"valid":true`.

- [ ] **Step 6: Update the design and agent guidance**

- In `docs/design.md`, section 4, add a final paragraph under "Text engine choice: Milestone 0": "Result (2026-MM-DD): TextKit N, pending acceptance. See `docs/spikes/2026-09-29-m0-textkit.md` and the decision record." Use relative links.
- If `restyleMs` p95 missed the budget, add this to section 4's pipeline notes: "Full reparse measured X ms p95; Milestone 1 reparses from the edited block."
- In `AGENTS.md`, change "Current state" to say that Milestone 0 is done, which engine was chosen (pending acceptance), and that the next step is the Milestone 1 plan.

- [ ] **Step 7: Verify and commit on `main`**

```bash
vrdx validate --json
git diff --check
git add docs/spikes/2026-09-29-m0-textkit.md decisions docs/design.md AGENTS.md
git commit -m "docs: record the Milestone 0 TextKit findings"
git push origin main
```

- [ ] **Step 8: Clean up and hand over**

```bash
git switch spike/m0-textkit && (cd spike && mise run clean) && git switch main
```

Ask Niklas whether to push `spike/m0-textkit` to GitHub for reference; it is a new remote branch on a public repository. Then report:

- the chosen engine and the three numbers that decided it;
- the findings path and the decision record ID;
- open questions for Milestone 1.
