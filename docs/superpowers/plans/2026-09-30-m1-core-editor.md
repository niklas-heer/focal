# Milestone 1 — Core Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Focal's editor daily-usable for prose and notes: lists, task lists and quotes drawn and edited correctly, the caret never inside hidden syntax, live reload through FSEvents, and a cheap accessibility tree, all covered by UI integration tests.

**Architecture:** `focal-core` gains a *line prefix* model: for every line, the container levels in front of its content (quote bars, list levels, the list marker) and where its content starts. The display map covers only the content; the app draws the prefix as real GPUI elements in columns beside the content, so wrapped lines hang under their text (the approach of Zed's Markdown renderer, which lays out list items as a marker column beside the content). The prefix is atomic for the caret and is edited through Markdown-aware commands. UI behavior is tested with GPUI Kit's headless test harness.

**Tech Stack:** Rust 1.97.1 (edition 2024), `pulldown-cmark` 0.13, `gpui-kit` 0.7.0 (GPUI `gpui-pre` 0.3.7) with its `test-support` feature for tests, `notify` 7 (FSEvents), `async-channel` 2, mise tasks.

**Spec:** [`docs/design.md`](../../design.md), sections 4 (pipeline, line layout, showing and hiding syntax), 3 (documents, live reload) and 8 (M1 row). Decision: [Build Focal in Rust with GPUI](../../../decisions/2026-09-30_190238170_build-focal-in-rust-with-gpui.md).

## Global Constraints

- The file is the document: opening and saving an unedited file must produce identical bytes. Commands change only the source ranges they are about.
- Keystroke budget: parse and restyle under 8 ms on the 5,000-line fixture (`mise run stress`).
- `mise run check` (rustfmt, Clippy with `-D warnings`, all tests) passes after every task.
- Logic lives in `focal-core` with unit tests; the app crate renders, handles input and files.
- No `unsafe` code (`unsafe_code = "forbid"`).
- Work on branch `m1-core-editor` in `.worktrees/rust-gpui`; conventional commits ending with the session's `Co-Authored-By` line.
- Accessibility text changes with rendering: when the prefix is drawn instead of text, the accessibility tree must still say what it is.
- Disk space is tight: `mise run clean` when done.

## Review Focus

1. **A list inside a quote and a quote inside a list** (`> - a` and `- a\n  > b`): prefix levels in the right order and content after all markers. Task 1 tests both.
2. **Ordered lists with different marker widths** (`9.` then `10.`, `1)`): the marker column fits and Tab indents to the parent's content column (3 spaces for `1.`). Tasks 1 and 4 test it.
3. **CRLF files**: prefix ranges end before `\r`, and list continuation inserts `\r\n`. Tasks 1 and 4 test it.
4. **Backspace at the start of an empty first line or of line 0**: never panics and never deletes outside the prefix. Task 4 tests line 0.
5. **An external change while the window is open, written by atomic rename** (editors and agents often write a temp file and rename it): live reload still fires. Task 6 tests the rename.

---

### Task 1: Line prefix model in `focal-core`

**Files:**
- Modify: `crates/focal-core/src/analysis.rs` (types, `block_quote`, `item`, `finish`, tests)
- Modify: `crates/focal-core/src/display.rs` (display starts at the content, tests)
- Modify: `crates/focal-core/src/lib.rs` (exports)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `pub enum ListMarker { Bullet, Ordered(String), Task { checked: bool } }` (Clone, Debug, PartialEq, Eq, Hash)
  - `pub enum PrefixLevel { Quote(Option<Alert>), List(Option<ListMarker>) }` — `List(None)` is a continuation column.
  - `pub struct LinePrefix { pub levels: Vec<PrefixLevel>, pub content_start: usize, pub marker: Option<Range<usize>>, pub quotes: Vec<Range<usize>> }` (Default) — `marker` is the list marker's source (task box and following spaces included); `quotes` are this line's `>` markers with one following space.
  - `LineInfo.prefix: LinePrefix`.
  - `Analysis::content_range(&self, line: usize) -> Range<usize>`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Bias { Left, Right }` and `Analysis::snap(&self, offset: usize, bias: Bias) -> usize`: an offset inside a line's prefix moves to that line's content start (`Right`) or the previous line's end (`Left`; line 0 goes to its content start).
  - `Replacement::Bullet`, `Replacement::Task`, `Reveal::Inside`, `InlineStyle::BULLET` and `InlineStyle::TASK_OPEN` are removed. Quote `>` markers are no longer `markers`.

- [ ] **Step 1: Write the failing tests**

Add to the tests module of `analysis.rs`, and delete the old `list_bullets_and_tasks_are_replaced`, `quotes_and_alerts` and `nested_quote_markers_stack` tests (they assert the removed marker behavior):

```rust
    fn prefix(text: &str, line: usize) -> LinePrefix {
        analyze(text).info(line).prefix.clone()
    }

    #[test]
    fn list_items_have_a_marker_and_content_after_it() {
        let text = "- one\n- [x] done\n12. twelve\n";
        let p = prefix(text, 0);
        assert_eq!(p.levels, [PrefixLevel::List(Some(ListMarker::Bullet))]);
        assert_eq!((p.marker, p.content_start), (Some(0..2), 2));
        let p = prefix(text, 1);
        assert_eq!(p.levels, [PrefixLevel::List(Some(ListMarker::Task { checked: true }))]);
        assert_eq!((p.marker, p.content_start), (Some(6..12), 12));
        let p = prefix(text, 2);
        assert_eq!(p.levels, [PrefixLevel::List(Some(ListMarker::Ordered("12.".into())))]);
        assert_eq!(&text[p.content_start..p.content_start + 6], "twelve");
    }

    #[test]
    fn nested_items_and_continuations_get_columns() {
        let text = "- a\n  - b\n    more b\n  back to a\n";
        let levels = |line| prefix(text, line).levels;
        assert_eq!(levels(1), [PrefixLevel::List(None), PrefixLevel::List(Some(ListMarker::Bullet))]);
        assert_eq!(levels(2), [PrefixLevel::List(None), PrefixLevel::List(None)]);
        assert_eq!(levels(3), [PrefixLevel::List(None)]);
        assert_eq!(&text[prefix(text, 2).content_start..], "more b\n  back to a\n");
        assert_eq!(prefix(text, 1).marker, Some(6..8));
    }

    #[test]
    fn quotes_and_lists_nest_in_source_order() {
        let text = "> - a\n\n- b\n  > c\n";
        assert_eq!(
            prefix(text, 0).levels,
            [PrefixLevel::Quote(None), PrefixLevel::List(Some(ListMarker::Bullet))]
        );
        assert_eq!(prefix(text, 0).quotes, [0..2]);
        assert_eq!(prefix(text, 0).content_start, 4);
        assert_eq!(prefix(text, 3).levels, [PrefixLevel::List(None), PrefixLevel::Quote(None)]);
        assert_eq!(&text[prefix(text, 3).content_start..], "c\n");
    }

    #[test]
    fn alerts_keep_their_kind_and_title_marker() {
        let text = "> [!WARNING]\n> Careful\n";
        let analysis = analyze(text);
        assert_eq!(analysis.info(1).prefix.levels, [PrefixLevel::Quote(Some(Alert::Warning))]);
        assert_eq!(analysis.info(0).alert, Some(Alert::Warning));
        assert_eq!(marker_texts(text, &analysis), ["[!WARNING]"]);
    }

    #[test]
    fn crlf_prefix_stops_before_the_line_ending() {
        let text = "- a\r\n- \r\n";
        let p = prefix(text, 1);
        assert_eq!(p.marker, Some(5..7));
        assert_eq!(p.content_start, 7);
        assert_eq!(analyze(text).content_range(1), 7..7);
    }

    #[test]
    fn snapping_moves_out_of_the_prefix() {
        let text = "intro\n- item\n";
        let analysis = analyze(text);
        assert_eq!(analysis.snap(6, Bias::Right), 8);
        assert_eq!(analysis.snap(7, Bias::Right), 8);
        assert_eq!(analysis.snap(7, Bias::Left), 5);
        assert_eq!(analysis.snap(9, Bias::Left), 9);
        assert_eq!(analyze("- x").snap(0, Bias::Left), 2);
    }
```

Replace `bullets_are_replaced_until_the_caret_enters` in `display.rs` tests with:

```rust
    #[test]
    fn list_lines_display_only_their_content() {
        let text = "- item **b**\n> - quoted\n";
        assert_eq!(view(text, 0, None).text, "item b");
        assert_eq!(view(text, 1, None).text, "quoted");
        let map = view(text, 0, None).map;
        assert_eq!(map.to_source(0), 2);
        assert_eq!(map.source_range().start, 2);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p focal-core`
Expected: compile errors (`LinePrefix`, `PrefixLevel`, `ListMarker`, `Bias`, `snap`, `content_range` not found).

- [ ] **Step 3: Implement the prefix model**

In `analysis.rs`:

1. Remove `Replacement::Bullet` and `Replacement::Task` (and their arms in `Replacement::text` and `Replacement::style`), `Reveal::Inside`, and `InlineStyle::BULLET` and `InlineStyle::TASK_OPEN`. Keep `TASK_DONE` (it strikes done task content).

2. Add the types next to `LineInfo`, and add `pub prefix: LinePrefix` to `LineInfo`:

```rust
/// A list item's marker, drawn in the item's marker column.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ListMarker {
    Bullet,
    /// The source label, such as `1.` or `3)`.
    Ordered(String),
    Task { checked: bool },
}

/// One container level in front of a line's content, outermost first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PrefixLevel {
    Quote(Option<Alert>),
    /// A list level: the marker on an item's first line, `None` on its other lines.
    List(Option<ListMarker>),
}

/// What stands in front of a line's content. Its source is drawn as elements
/// (quote bars, bullets, checkboxes) and the caret never stops inside it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LinePrefix {
    pub levels: Vec<PrefixLevel>,
    pub content_start: usize,
    /// The list marker's source on an item's first line, task box and spaces included.
    pub marker: Option<Range<usize>>,
    /// This line's `>` markers, each with one following space.
    pub quotes: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bias {
    Left,
    Right,
}
```

3. Add a container record to the builder and collect containers:

```rust
enum ContainerKind {
    Quote(Option<Alert>),
    Item { marker: ListMarker, marker_range: Range<usize> },
}

struct Container {
    range: Range<usize>,
    kind: ContainerKind,
}
```

Add `containers: Vec<Container>` to `Builder` (initialized empty in `analyze`). In `block_quote`, replace the `self.markers.push(Marker { … Reveal::Lines(line..line + 1) … })` for the `>` marker with `self.infos[line].prefix.quotes.push(self.prefix_end[line]..pos);`, keep updating `prefix_end`, `quote_depth` and `alert`, and at the end of the function push `self.containers.push(Container { range: self.trim_line_ending(range), kind: ContainerKind::Quote(alert) });`. Keep the alert title marker.

4. Replace `fn item` with:

```rust
    fn item(
        &mut self,
        range: &Range<usize>,
        content: Option<Range<usize>>,
        task: Option<(Range<usize>, bool)>,
    ) {
        let bytes = self.text.as_bytes();
        let line_end = self.lines.range(self.lines.line_of(range.start)).end;
        let start = range.start;
        let mut end = start;
        while end < line_end && !matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
        let label = &self.text[start..end];
        let mut marker = if matches!(label, "-" | "*" | "+") {
            ListMarker::Bullet
        } else {
            ListMarker::Ordered(label.to_owned())
        };
        if let Some((task_range, checked)) = &task {
            marker = ListMarker::Task { checked: *checked };
            end = task_range.end;
            if *checked && let Some(content) = &content {
                self.style(end.max(content.start)..content.end, InlineStyle::TASK_DONE);
            }
        }
        while end < line_end && matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
        self.containers.push(Container {
            range: self.trim_line_ending(range),
            kind: ContainerKind::Item { marker, marker_range: start..end },
        });
    }
```

5. In `finish`, before bucketing, compute prefixes (move `infos` into a mutable local first):

```rust
        let mut infos = self.infos;
        for (line, info) in infos.iter_mut().enumerate() {
            info.prefix.content_start = self.prefix_end[line];
        }
        let mut containers = self.containers;
        containers.sort_by_key(|c| (c.range.start, std::cmp::Reverse(c.range.end)));
        for container in &containers {
            let lines = self.lines.lines_of(&container.range);
            match &container.kind {
                ContainerKind::Quote(alert) => {
                    for line in lines {
                        infos[line].prefix.levels.push(PrefixLevel::Quote(*alert));
                    }
                }
                ContainerKind::Item { marker, marker_range } => {
                    let first = lines.start;
                    // The item's own indent, measured from where its prefix starts on
                    // its first line; continuation lines skip up to that much whitespace.
                    let own_start = infos[first].prefix.content_start.min(marker_range.start);
                    let indent = marker_range.end - own_start;
                    for line in lines {
                        let prefix = &mut infos[line].prefix;
                        if line == first {
                            prefix.levels.push(PrefixLevel::List(Some(marker.clone())));
                            prefix.marker = Some(marker_range.clone());
                            prefix.content_start = marker_range.end;
                        } else {
                            prefix.levels.push(PrefixLevel::List(None));
                            let end = self.lines.range(line).end;
                            let bytes = self.text.as_bytes();
                            let mut at = prefix.content_start;
                            while at < end && at - prefix.content_start < indent && matches!(bytes[at], b' ' | b'\t') {
                                at += 1;
                            }
                            prefix.content_start = at;
                        }
                    }
                }
            }
        }
```

Use `infos` in the returned `Analysis`. (A continuation line of an outer item that is also a nested item's first line is first skipped by the outer item, then set by the inner one, because containers are processed outermost first.)

6. Add the methods on `Analysis`:

```rust
    /// The part of a line after its prefix.
    pub fn content_range(&self, line: usize) -> Range<usize> {
        let range = self.lines.range(line);
        self.info(line).prefix.content_start.clamp(range.start, range.end)..range.end
    }

    /// Moves an offset out of a line's prefix, where the caret may not stop.
    pub fn snap(&self, offset: usize, bias: Bias) -> usize {
        let line = self.lines.line_of(offset);
        let content = self.content_range(line);
        let line_start = self.lines.range(line).start;
        if offset >= content.start || content.start == line_start {
            return offset;
        }
        match bias {
            Bias::Right => content.start,
            Bias::Left if line > 0 => self.lines.range(line - 1).end,
            Bias::Left => content.start,
        }
    }
```

In `display.rs`, make `line_view` start at the content:

```rust
pub fn line_view(analysis: &Analysis, text: &str, line: usize, caret: Option<&Caret>) -> LineView {
    range_view(analysis, text, line, analysis.content_range(line), caret)
}
```

and remove the `Reveal::Inside` arm from `Caret::reveals`. In `lib.rs`, export `pub use analysis::{Analysis, Bias, LinePrefix, ListMarker, PrefixLevel, analyze};`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p focal-core`
Expected: all tests pass. Then `cargo build -p focal` fails only where the app uses the removed `Replacement::Task`, `BULLET` and `TASK_OPEN`; Task 2 replaces that code, so for this task delete those uses (the `task` field of `Hit::Text`, its toggle branch in `on_mouse_down`, and the `BULLET`/`TASK_OPEN` color arms in `text_runs`) until `cargo build -p focal` succeeds.

- [ ] **Step 5: Commit**

```bash
git add crates/focal-core crates/focal/src/editor.rs
git commit -m "feat(core): describe each line's prefix of quotes and list levels"
```

---

### Task 2: UI test harness

**Files:**
- Modify: `crates/focal/Cargo.toml` (dev-dependency)
- Modify: `crates/focal/src/main.rs` (`#[cfg(test)] mod ui_tests;`)
- Modify: `crates/focal/src/editor.rs` (`.test_support()`, test accessors)
- Create: `crates/focal/src/ui_tests.rs`

**Interfaces:**
- Consumes: `Editor::new`, `Document::untitled`, `editor::bind_keys`.
- Produces: `ui_tests::open_editor(cx: &mut TestAppContext, text: &str) -> (AnyWindowHandle, Entity<Editor>)`, `ui_tests::act(cx, window, f: impl FnOnce(&mut Window, &mut App))`, and on `Editor`: `pub(crate) fn text(&self) -> &str`, `pub(crate) fn selection(&self) -> Range<usize>`.

- [ ] **Step 1: Add the dev-dependency and harness**

In `crates/focal/Cargo.toml`:

```toml
[dev-dependencies]
gpui-kit = { version = "=0.7.0", default-features = false, features = ["component", "test-support"] }
```

In `editor.rs`, make `fn text(&self) -> &str` `pub(crate)`, add `pub(crate) fn selection(&self) -> Range<usize> { self.selection.clone() }`, and insert `.test_support()` right before `.track_focus(&self.focus_handle)` on the root `div()` (import `gpui_kit::TestSupportExt as _`).

Create `crates/focal/src/ui_tests.rs`:

```rust
//! Headless UI tests: the real editor in a test window, driven by key presses,
//! text input and clicks through GPUI Kit's test harness.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, Focusable as _, TestAppContext, Window,
    WindowBounds, WindowOptions, point, px, size,
};

use crate::document::Document;
use crate::editor::{self, Editor};

pub fn open_editor(cx: &mut TestAppContext, text: &str) -> (AnyWindowHandle, Entity<Editor>) {
    let text = text.to_owned();
    cx.update(|cx| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
        let bounds = Bounds { origin: point(px(0.), px(0.)), size: size(px(900.), px(700.)) };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..WindowOptions::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let editor = cx.new(|cx| Editor::new(Document::untitled(), text, window, cx));
            window.focus(&editor.read(cx).focus_handle(cx), cx);
            editor
        })
        .expect("open test window")
    })
}

/// Renders a frame, then runs `f` with the window.
pub fn act(cx: &mut TestAppContext, window: AnyWindowHandle, f: impl FnOnce(&mut Window, &mut App)) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        f(window, cx);
    })
    .expect("window is open");
}

#[gpui_kit::test]
fn typing_inserts_at_the_caret(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "Hello");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.input(" world", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "Hello world"));
}

#[gpui_kit::test]
fn return_continues_a_list(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- one");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("enter", cx);
        window.input("two", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- one\n- two"));
}
```

In `main.rs`, add `#[cfg(test)] mod ui_tests;`.

- [ ] **Step 2: Run the tests**

Run: `cargo test -p focal ui_tests`
Expected: `2 passed`. These pin existing behavior, so they pass immediately; the harness is the deliverable. If `input` does not reach the editor, check that the root `div` has `.test_support()` before `.track_focus` and that the editor is focused.

- [ ] **Step 3: Commit**

```bash
git add crates/focal/Cargo.toml Cargo.lock crates/focal/src/main.rs crates/focal/src/editor.rs crates/focal/src/ui_tests.rs
git commit -m "test(ui): drive the real editor headlessly"
```

---

### Task 3: Draw the prefix: quote bars, bullets, numbers and checkboxes

**Files:**
- Create: `crates/focal/src/prefix.rs`
- Modify: `crates/focal/src/editor.rs` (`render_line`, `toggle_task`)
- Modify: `crates/focal/src/accessibility.rs` (marker text in the accessibility tree)
- Modify: `crates/focal/src/theme.rs` (`checkbox` color)
- Modify: `crates/focal/src/ui_tests.rs`

**Interfaces:**
- Consumes: `LinePrefix`, `PrefixLevel`, `ListMarker` (Task 1); `open_editor`, `act` (Task 2).
- Produces: `prefix::wrap(content: AnyElement, prefix: &LinePrefix, line: usize, theme: &Theme, line_height: Pixels, on_toggle: impl Fn(usize, &mut Window, &mut App) + Clone + 'static) -> AnyElement`, `prefix::marker_text(prefix: &LinePrefix) -> String`, and `Editor::toggle_task_on_line(&mut self, line: usize, cx: &mut Context<Self>)`. Task checkboxes have the element id `("task", line)`.

- [ ] **Step 1: Write the failing UI test**

Add to `ui_tests.rs`:

```rust
#[gpui_kit::test]
fn clicking_a_checkbox_toggles_the_task(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- [ ] buy milk\n- [x] done");
    act(cx, window, |window, cx| window.click(("task", 0usize), cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- [x] buy milk\n- [x] done"));
    act(cx, window, |window, cx| window.click(("task", 1usize), cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- [x] buy milk\n- [ ] done"));
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p focal clicking_a_checkbox`
Expected: FAIL with `missing ElementId`.

- [ ] **Step 3: Implement the prefix elements**

Add `pub checkbox: Hsla` to `Theme` (light `rgb(0x0026_6fb5)`, dark `rgb(0x0068_a9e8)`, the link colors).

Create `crates/focal/src/prefix.rs`:

```rust
//! Draws a line's prefix (quote bars, list markers, checkboxes) as elements in
//! columns beside the content, so wrapped text hangs under its first word.

use focal_core::{LinePrefix, ListMarker, PrefixLevel};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _,
    Pixels, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::theme::Theme;

/// Width of a list level's marker column.
pub const MARKER_COLUMN: f32 = 28.;

/// Wraps `content` in its prefix, innermost level closest to the content.
pub fn wrap(
    content: AnyElement,
    prefix: &LinePrefix,
    line: usize,
    theme: &Theme,
    line_height: Pixels,
    on_toggle: impl Fn(usize, &mut Window, &mut App) + Clone + 'static,
) -> AnyElement {
    let depth_of = |index: usize| {
        prefix.levels[..index].iter().filter(|l| matches!(l, PrefixLevel::List(_))).count()
    };
    let mut element = content;
    for (index, level) in prefix.levels.iter().enumerate().rev() {
        element = match level {
            PrefixLevel::Quote(alert) => div()
                .border_l(px(3.))
                .border_color(alert.map_or(theme.quote_bar, |alert| theme.alert(alert)))
                .pl(px(14.))
                .child(element)
                .into_any_element(),
            PrefixLevel::List(marker) => div()
                .flex()
                .items_start()
                .child(
                    div()
                        .w(px(MARKER_COLUMN))
                        .h(line_height)
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_end()
                        .pr(px(8.))
                        .children(marker.as_ref().map(|marker| {
                            marker_element(marker, depth_of(index), line, theme, on_toggle.clone())
                        })),
                )
                .child(div().flex_1().min_w(px(0.)).child(element))
                .into_any_element(),
        };
    }
    element
}

fn marker_element(
    marker: &ListMarker,
    depth: usize,
    line: usize,
    theme: &Theme,
    on_toggle: impl Fn(usize, &mut Window, &mut App) + 'static,
) -> AnyElement {
    match marker {
        ListMarker::Bullet => {
            let dot = div().size(px(6.)).rounded_full();
            match depth {
                0 => dot.bg(theme.text),
                1 => dot.border_1().border_color(theme.text),
                _ => dot.rounded(px(1.)).size(px(5.)).bg(theme.marker),
            }
            .into_any_element()
        }
        ListMarker::Ordered(label) => div().text_color(theme.marker).child(label.clone()).into_any_element(),
        ListMarker::Task { checked } => div()
            .id(("task", line))
            .test_support()
            .size(px(14.))
            .rounded(px(3.))
            .border_1()
            .border_color(if *checked { theme.checkbox } else { theme.marker })
            .when(*checked, |d| {
                d.bg(theme.checkbox)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(10.))
                    .text_color(theme.background)
                    .child("✓")
            })
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                on_toggle(line, window, cx);
            })
            .into_any_element(),
    }
}

/// What the prefix says, for the accessibility tree: "• ", "1. ", "☐ " or "☑ ".
pub fn marker_text(prefix: &LinePrefix) -> String {
    prefix
        .levels
        .iter()
        .find_map(|level| match level {
            PrefixLevel::List(Some(ListMarker::Bullet)) => Some("• ".to_owned()),
            PrefixLevel::List(Some(ListMarker::Ordered(label))) => Some(format!("{label} ")),
            PrefixLevel::List(Some(ListMarker::Task { checked })) => {
                Some(if *checked { "☑ " } else { "☐ " }.to_owned())
            }
            _ => None,
        })
        .unwrap_or_default()
}
```

Register it in `main.rs` (`mod prefix;`).

In `editor.rs`:

- Replace `toggle_task(&mut self, marker: Range<usize>, …)` with:

```rust
    pub(crate) fn toggle_task_on_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(marker) = self.snapshot.analysis.info(line).prefix.marker.clone() else {
            return;
        };
        let Some(open) = self.text()[marker.clone()].find('[') else {
            return;
        };
        let at = marker.start + open + 1;
        let checked = matches!(self.text().as_bytes().get(at), Some(b'x' | b'X'));
        let selection = self.selection.clone();
        self.edit(at..at + 1, if checked { " " } else { "x" }, selection, EditKind::Other, cx);
    }
```

- In `render_line`, remove the quote-bar styling (`.when(info.quote_depth > 0, …)`) from the content `div`, keep the content `div` as it is otherwise, and return it wrapped:

```rust
        let entity = cx.entity().downgrade();
        let on_toggle = move |line: usize, _: &mut Window, cx: &mut App| {
            entity.update(cx, |editor, cx| editor.toggle_task_on_line(line, cx)).ok();
        };
        let line_height = px(TEXT_SIZE * scale * if matches!(info.kind, LineKind::Heading(_)) { 1.3 } else { 1.6 });
        crate::prefix::wrap(content.into_any_element(), &info.prefix, line, theme, line_height, on_toggle)
```

(`content` is the existing content `div` chain, bound to a variable instead of returned.)

In `accessibility.rs`, prepend `prefix::marker_text(&analysis.info(line).prefix)` to each line's accessibility text in `A11yDocument::build`, store its byte length per line as `lead: Vec<usize>`, add it in `position` (`display + lead[line]`) and subtract it (saturating) in `source_offset`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p focal`
Expected: all pass, including `clicking_a_checkbox_toggles_the_task`.

- [ ] **Step 5: Check it by hand**

Run `mise run run -- examples/showcase.md` and confirm: the long bullet's second line starts under its text, not under the bullet; nested bullets are hollow; `1.`/`2.` sit right-aligned in their column; checkboxes are square, filled when done; quotes and alerts have their bars; a list inside a quote shows both.

- [ ] **Step 6: Commit**

```bash
git add crates/focal/src crates/focal/Cargo.toml
git commit -m "feat(render): draw list markers, checkboxes and quote bars beside the text"
```

---

### Task 4: The caret stays out of the prefix, and list editing commands

**Files:**
- Modify: `crates/focal-core/src/editing.rs` (commands, tests)
- Modify: `crates/focal/src/editor.rs` (movement, Backspace, Tab, Shift-Tab, Return)
- Modify: `crates/focal/src/ui_tests.rs`

**Interfaces:**
- Consumes: `Analysis::snap`, `Bias`, `LinePrefix` (Task 1); `continue_list`, `Change` (existing); `open_editor`, `act` (Task 2).
- Produces in `focal_core::editing`:
  - `backspace_prefix(analysis: &Analysis, head: usize) -> Option<Change>` — at a line's content start: removes the list marker if there is one, else the innermost `>`, else joins the line to the previous one; `None` when `head` is not at a content start that has a prefix.
  - `indent_list_item(text: &str, lines: &LineIndex, line: usize) -> Option<Change>` — indents to the previous sibling's content column.
  - `outdent_list_item(text: &str, lines: &LineIndex, line: usize) -> Option<Change>` — outdents to the parent item's indentation.
  - `continue_list` now outdents an empty nested item instead of removing its marker.

- [ ] **Step 1: Write the failing tests**

Add to `editing.rs` tests (and `use crate::analysis::analyze; use crate::lines::LineIndex;` in the module):

```rust
    fn apply(text: &str, change: &Change) -> String {
        let mut out = text.to_owned();
        out.replace_range(change.range.clone(), &change.text);
        out
    }

    #[test]
    fn backspace_removes_the_marker_then_quotes_then_joins() {
        let text = "- [ ] task\n> quote\nnext\n  cont";
        let analysis = analyze(text);
        let change = backspace_prefix(&analysis, 6).unwrap();
        assert_eq!(apply(text, &change), "task\n> quote\nnext\n  cont");
        let change = backspace_prefix(&analysis, 13).unwrap();
        assert_eq!(apply(text, &change), "- [ ] task\nquote\nnext\n  cont");
        assert!(backspace_prefix(&analysis, 19).is_none(), "no prefix on `next`");
        assert!(backspace_prefix(&analysis, 7).is_none(), "not at the content start");
    }

    #[test]
    fn backspace_on_line_zero_never_leaves_the_prefix() {
        let text = "> hi";
        let analysis = analyze(text);
        assert_eq!(apply(text, &backspace_prefix(&analysis, 2).unwrap()), "hi");
        assert!(backspace_prefix(&analyze(""), 0).is_none());
    }

    #[test]
    fn tab_indents_to_the_previous_siblings_content() {
        let text = "1. one\n2. two";
        let lines = LineIndex::new(text);
        let change = indent_list_item(text, &lines, 1).unwrap();
        assert_eq!(apply(text, &change), "1. one\n   2. two");
        assert!(indent_list_item(text, &lines, 0).is_none(), "first item has no sibling");
    }

    #[test]
    fn shift_tab_outdents_to_the_parent() {
        let text = "- a\n  - b";
        let lines = LineIndex::new(text);
        assert_eq!(apply(text, &outdent_list_item(text, &lines, 1).unwrap()), "- a\n- b");
        assert!(outdent_list_item(text, &lines, 0).is_none());
    }

    #[test]
    fn return_on_an_empty_nested_item_outdents_it() {
        let text = "- a\n  - ";
        let change = continue_list(text, &(4..8), &(8..8), "\n").unwrap();
        assert_eq!(apply(text, &change), "- a\n- ");
    }
```

Add to `ui_tests.rs`:

```rust
#[gpui_kit::test]
fn arrow_left_from_an_item_skips_its_marker(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "intro\n- item");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("cmd-left", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.selection(), 8..8));
    act(cx, window, |window, cx| window.press("left", cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.selection(), 5..5));
}

#[gpui_kit::test]
fn backspace_at_an_item_start_removes_the_bullet(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- item");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("cmd-left", cx);
        window.press("backspace", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "item"));
}

#[gpui_kit::test]
fn tab_and_shift_tab_change_the_list_level(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- a\n- b");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("tab", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- a\n  - b"));
    act(cx, window, |window, cx| window.press("shift-tab", cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- a\n- b"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p focal-core editing && cargo test -p focal ui_tests`
Expected: compile errors for the new functions, then UI failures (the caret lands inside the marker; Backspace deletes one character).

- [ ] **Step 3: Implement the commands**

In `editing.rs`:

```rust
use crate::analysis::Analysis;
use crate::lines::LineIndex;

/// Backspace at a line's content start edits the prefix instead of text.
pub fn backspace_prefix(analysis: &Analysis, head: usize) -> Option<Change> {
    let line = analysis.lines.line_of(head);
    let content = analysis.content_range(line);
    let line_start = analysis.lines.range(line).start;
    if head != content.start || content.start == line_start {
        return None;
    }
    let prefix = &analysis.info(line).prefix;
    let range = if let Some(marker) = &prefix.marker {
        marker.clone()
    } else if let Some(quote) = prefix.quotes.last() {
        quote.clone()
    } else if line > 0 {
        analysis.lines.range(line - 1).end..content.start
    } else {
        line_start..content.start
    };
    Some(Change { selection: range.start..range.start, range, text: String::new() })
}

fn lead_width(prefix: &ListPrefix) -> usize {
    prefix.lead.len()
}

/// The previous line that is a list item, with its prefix.
fn previous_item(text: &str, lines: &LineIndex, line: usize) -> impl Iterator<Item = (usize, ListPrefix)> + '_ {
    (0..line).rev().filter_map(move |l| list_prefix(text, &lines.range(l)).map(|p| (l, p)))
}

pub fn indent_list_item(text: &str, lines: &LineIndex, line: usize) -> Option<Change> {
    let range = lines.range(line);
    let current = list_prefix(text, &range)?;
    let (_, sibling) = previous_item(text, lines, line).find(|(_, p)| lead_width(p) == lead_width(&current))?;
    let column = sibling.marker_end - sibling.lead.start;
    let add = column.saturating_sub(lead_width(&current));
    if add == 0 {
        return None;
    }
    let at = current.lead.end;
    Some(Change { range: at..at, text: " ".repeat(add), selection: at + add..at + add })
}

pub fn outdent_list_item(text: &str, lines: &LineIndex, line: usize) -> Option<Change> {
    let range = lines.range(line);
    let current = list_prefix(text, &range)?;
    if lead_width(&current) == 0 {
        return None;
    }
    let parent = previous_item(text, lines, line)
        .find(|(_, p)| lead_width(p) < lead_width(&current))
        .map_or(0, |(_, p)| lead_width(&p));
    let remove = lead_width(&current) - parent;
    let end = current.lead.end;
    Some(Change { range: end - remove..end, text: String::new(), selection: end - remove..end - remove })
}
```

Change `ListPrefix` so `lead` covers only indentation *after* any quote markers (scan `>` and the following space first, then whitespace), add `quotes: Range<usize>` for the quote markers before it, and add `marker_end: usize` (the position after the marker and its spaces, before a task box). `continue_list` keeps copying everything in front of the marker, `text[quotes.start..lead.end]`, onto the new line, so a list inside a quote continues inside the quote. In `continue_list`, for an empty item with a non-empty `lead`, return `outdent_list_item`'s change instead of removing the marker (compute `LineIndex::new(text)` and the line from `line.start`).

In `editor.rs`:
- `move_to` and `select_to` snap their target: `let offset = self.snapshot.analysis.snap(offset, bias);`, where `left`, `word_left`, `up`, `select_left`, `select_up`, `select_word_left` pass `Bias::Left` and the others `Bias::Right` (add a `bias` parameter to both functions).
- `line_start`/`select_line_start` go to `self.snapshot.analysis.content_range(line).start`.
- `backspace`: when the selection is empty, first try `editing::backspace_prefix(&self.snapshot.analysis, self.head())` and `apply` it.
- `indent`/`outdent` use `indent_list_item`/`outdent_list_item` for the head's line, falling back to inserting `\t` (indent) or doing nothing (outdent); shift the selection by the change's length difference.

- [ ] **Step 4: Run the tests**

Run: `cargo test --workspace`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/focal-core/src/editing.rs crates/focal/src/editor.rs crates/focal/src/ui_tests.rs
git commit -m "feat(editing): keep the caret out of list markers and edit list levels"
```

---

### Task 5: Pin reveal and island behavior with UI tests

**Files:**
- Modify: `crates/focal/src/ui_tests.rs`

**Interfaces:**
- Consumes: `open_editor`, `act`, `Editor::selection`.
- Produces: tests only.

- [ ] **Step 1: Write the tests**

```rust
#[gpui_kit::test]
fn down_into_a_table_shows_its_source_with_the_caret_in_it(cx: &mut TestAppContext) {
    let text = "before\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nafter";
    let (window, editor) = open_editor(cx, text);
    act(cx, window, |window, cx| {
        window.press("down", cx);
        window.press("down", cx);
    });
    editor.read_with(cx, |editor, _| {
        let caret = editor.selection().start;
        assert!((8..37).contains(&caret), "caret {caret} is inside the table source");
    });
}

#[gpui_kit::test]
fn undo_restores_text_after_list_editing(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- a");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("enter", cx);
        window.input("b", cx);
        window.press("cmd-z", cx);
        window.press("cmd-z", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- a"));
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p focal ui_tests`
Expected: pass. If the first fails because the caret skips the table, that contradicts design section 4 (islands reveal their source when the caret enters): fix `vertical` in `editor.rs` rather than the test.

- [ ] **Step 3: Commit**

```bash
git add crates/focal/src/ui_tests.rs
git commit -m "test(ui): pin table reveal and undo after list edits"
```

---

### Task 6: Live reload through FSEvents

**Files:**
- Modify: `crates/focal/Cargo.toml` (`notify = "7.0.0"`, `async-channel = "2"`; match the versions already in `Cargo.lock`)
- Modify: `crates/focal/src/document.rs` (`watch`, test)
- Modify: `crates/focal/src/editor.rs` (replace the 1-second poll)

**Interfaces:**
- Consumes: `Document`, `Stamp`, `Editor::check_disk`.
- Produces: `document::watch(path: &Path) -> anyhow::Result<(notify::RecommendedWatcher, async_channel::Receiver<()>)>` — one message per change to the file, including an atomic replace by rename; the watcher stops when dropped.

- [ ] **Step 1: Write the failing tests**

In `document.rs` tests:

```rust
    fn next_event(rx: &async_channel::Receiver<()>) -> bool {
        let rx = rx.clone();
        let (tx, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(rx.recv_blocking().is_ok());
        });
        done.recv_timeout(std::time::Duration::from_secs(5)).unwrap_or(false)
    }

    #[test]
    fn watching_reports_writes_and_atomic_renames() {
        let dir = std::env::temp_dir().join(format!("focal-watch-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("watched.md");
        fs::write(&path, "one").unwrap();
        let (_watcher, rx) = watch(&path).unwrap();
        fs::write(&path, "two").unwrap();
        assert!(next_event(&rx), "a write is reported");
        while rx.try_recv().is_ok() {}
        let temp = dir.join(".watched.md.tmp");
        fs::write(&temp, "three").unwrap();
        fs::rename(&temp, &path).unwrap();
        assert!(next_event(&rx), "an atomic rename is reported");
        fs::remove_dir_all(&dir).unwrap();
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p focal watching`
Expected: compile error, `watch` not found.

- [ ] **Step 3: Implement**

```rust
/// Watches `path` for changes, including being replaced by a rename. The
/// file's folder is watched, because a rename replaces the file's inode.
pub fn watch(path: &Path) -> Result<(notify::RecommendedWatcher, async_channel::Receiver<()>)> {
    use notify::{RecursiveMode, Watcher as _};
    let (tx, rx) = async_channel::unbounded();
    let target = path.to_owned();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && event.paths.iter().any(|p| p == &target)
        {
            let _ = tx.try_send(());
        }
    })?;
    let folder = path.parent().context("the file has no folder")?;
    watcher.watch(folder, RecursiveMode::NonRecursive)?;
    Ok((watcher, rx))
}
```

(If FSEvents reports canonical paths such as `/private/var/…` for `/var/…`, compare canonicalized paths: canonicalize `target` and the folder once, and each event path's parent joined with its file name.)

In `editor.rs`, replace the polling task: in `Editor::new`, when the document has a path, call `document::watch`, keep the watcher in a new field `_watcher: Option<notify::RecommendedWatcher>`, and spawn:

```rust
        let poll = cx.spawn(async move |this, cx| {
            let Some(events) = events else { return };
            while events.recv().await.is_ok() {
                // Editors write in bursts; let them finish.
                cx.background_executor().timer(Duration::from_millis(50)).await;
                while events.try_recv().is_ok() {}
                if this.update(cx, |this, cx| this.check_disk(cx)).is_err() {
                    break;
                }
            }
        });
```

Keep `check_disk` unchanged (it already ignores Focal's own saves through the stamp). Remove `DISK_POLL`. For "Save As" of an untitled document (the `save` action), start a watcher for the new path the same way (extract the spawning into `fn watch_document(&mut self, cx)`).

- [ ] **Step 4: Run the tests and check by hand**

Run: `cargo test --workspace`. Then open a file with `mise run run -- /tmp/x.md`, and in a terminal run `printf 'more\n' >> /tmp/x.md`: the window shows the new line at once (not after a second).

- [ ] **Step 5: Commit**

```bash
git add crates/focal/Cargo.toml Cargo.lock crates/focal/src/document.rs crates/focal/src/editor.rs
git commit -m "feat(files): reload on FSEvents instead of polling"
```

---

### Task 7: Cache the accessibility tree

**Files:**
- Modify: `crates/focal/src/accessibility.rs`

**Interfaces:**
- Consumes: `A11ySource`, `A11yDocument` (existing).
- Produces: `A11yDocument` also holds `nodes: Vec<(NodeKey, accesskit::Node)>` built once per document version, and `run_index: HashMap<(usize, usize), NodeKey>`; `build_tree` only clones them, maps keys to ids and sets the selection. `NodeKey` is `(&'static str, usize, usize, usize)`.

- [ ] **Step 1: Record the baseline**

Run: `cargo run -q --release -p focal-core --example stress -- /tmp/focal-stress.md`, then `FOCAL_FOREGROUND=1 FOCAL_TRACE=1 ./target/release/focal /tmp/focal-stress.md`, connect an accessibility client (for example Accessibility Inspector, or turn VoiceOver on and off), type a character, and note the `accessibility tree … ms` lines (about 12 ms before this task).

- [ ] **Step 2: Cache the nodes**

Move the node construction out of `build_tree` into `A11yDocument::build` (which then also needs `rows` and `table_cells` from `A11ySource`, so pass `&A11ySource`): build every text-run, cell, row and table `accesskit::Node` there with its key, the list of top-level keys in order, and a `(line, chunk) → key` map. In `build_tree`, for each cached `(key, node)` call `builder.push_child(builder.synthetic_node_id(key), node.clone())`, set the parent's children from the top-level keys, look up selection positions through the map (no scan over all runs), and fill `RunIds` from the map.

- [ ] **Step 3: Measure again**

Repeat Step 1. Expected: under 4 ms per frame; record the number in the commit message. If it is still above 4 ms, note it in `docs/gpui-spike.md` under the gate's limits instead of optimizing further in this milestone.

- [ ] **Step 4: Verify behavior did not change**

Run `cargo test --workspace`, then re-run the accessibility-client check from the gate (rendered text, selection round-trip, table cells) or the `ax` client if you still have it.

- [ ] **Step 5: Commit**

```bash
git add crates/focal/src/accessibility.rs
git commit -m "perf(a11y): build accessibility nodes once per document version"
```

---

### Task 8: Showcase, documentation and milestone check

**Files:**
- Modify: `examples/showcase.md` (nested lists, ordered widths, a list in a quote, a quote in a list)
- Modify: `docs/design.md` (section 8: M1 done items; move code highlighting to M4)
- Modify: `README.md` (status)

- [ ] **Step 1: Extend the showcase**

Add a "Lists in depth" section to `examples/showcase.md`:

```markdown
## Lists in depth

1. First
   - nested bullet
     - deeper, with a long line that wraps so the continuation should start under this text rather than under the bullet
2. Second
9. Nine
10. Ten

- [ ] Open task
  - [x] Nested done task

> - A list inside a quote
> - Second item

- A quote inside a list:
  > Quoted under the item
```

- [ ] **Step 2: Update the design**

In section 8, change the M1 row's outcome to what shipped, and move "code highlighting" to the M4 row (`Alerts, front matter, footnotes, highlight, images, math, wiki links and code highlighting through GPUI Kit's tree-sitter highlighter`). Mark section 4's "Line layout" as Agreed with a sentence noting that prefix levels are drawn as columns beside the content.

- [ ] **Step 3: Full check and manual pass**

Run: `mise run check` and `mise run stress` (parse + restyle still well under 8 ms). Open the showcase and walk the new section: bullets, numbers, checkboxes, nested indentation, wrapping, quote bars; Return, Tab, Shift-Tab and Backspace on items; click a checkbox.

- [ ] **Step 4: Commit and clean**

```bash
git add examples/showcase.md docs/design.md README.md
git commit -m "docs: record Milestone 1 and extend the showcase"
mise run clean
```
