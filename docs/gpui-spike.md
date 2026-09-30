# Focal on Rust and GPUI: spike findings

This branch (`rust-gpui`) builds the same app idea as [the design](design.md), in Rust with [GPUI](https://gpui.rs), the GPU-accelerated UI framework from Zed, instead of Swift and AppKit. It answers one question: can GPUI carry Focal's core, a calm live-rendering Markdown editor, before any milestone commits to a stack?

**Status:** working spike, not the agreed direction. The AppKit decision stays accepted until Niklas decides otherwise; the alternative is recorded as a proposed decision in [`decisions/`](../decisions/).

## What the spike does

```sh
mise run run -- examples/showcase.md    # or: cargo run -p focal -- --wait FILE
mise run stress                         # timings on the 5,000-line document
mise run check                          # fmt, Clippy, tests
```

- `focal FILE` opens a window and returns the terminal; `--wait` blocks until the window closes, so it works as `$EDITOR`; `cat x.md | focal` opens the text as an untitled document (⌘S asks where to save). A missing file opens empty and is created on first save. Folders are refused for now.
- **Live rendering, Bear-style.** Emphasis, strong, strikethrough, inline code, links, wiki links, `==highlight==`, inline math and footnote references hide their markers until the caret or selection touches them. Heading `#`s, quote `>`s and fences show when the caret is on their line or block. Bullets become `•` and task boxes `☐`/`☑` until the caret enters them; clicking a box toggles it.
- **Blocks:** headings in three sizes, fenced and indented code on a tinted block with a language label, quotes with a bar, GitHub alerts with a colored bar and title, YAML front matter as quiet mono text, thematic breaks as a rule.
- **Tables as islands.** Away from the caret, a table is a real grid with header, column alignment and styled cells. Clicking it or moving the caret in shows the source, in mono so pipes align. Grid editing is Milestone 2.
- **Editing:** typing with IME support through GPUI's platform input handler, grapheme and word movement, visual up/down through wrapped lines, click, double-click word, triple-click line, drag and shift selection, copy/cut/paste of the Markdown source, coalesced undo/redo, list continuation on Return (Return on an empty item ends the list), Tab/Shift-Tab to indent list items, ⌘B/⌘I to toggle wrapping, ⌘-click to open links.
- **Files:** exact byte round-trip (CRLF, no trailing newline, tabs and non-ASCII are preserved; non-UTF-8 files are refused), autosave 400 ms after the last edit, save on close and quit, and live reload: the file is polled every second and reloaded when it changed and nothing is unsaved; with unsaved edits a banner offers "Keep mine" or "Load theirs".
- **Look:** iA Writer Quattro and Mono, a column of at most 720 pt, light and dark palettes following the system, and focus mode (⌘D) that dims everything but the current paragraph.

Checked by hand in the running app, with synthetic key and mouse events: rendering of every block above in dark mode, caret movement through wrapped lines and tables, typing, list continuation, click, drag and double-click selection, task toggling, undo/redo, autosave, live reload and focus mode. **Not yet checked by hand:** the reload conflict banner, ⌘-click on links, a real input method (Japanese or Chinese), light mode, and saving an untitled document.

## Architecture

```text
crates/focal-core   no UI dependency; 32 unit tests
  analysis.rs       pulldown-cmark offset events → styles, markers (hide/replace + reveal rule), line kinds, tables, links
  display.rs        one source line + caret → display text, styled runs and a display↔source offset map
  buffer.rs         the text, undo/redo with coalescing, grapheme and word boundaries
  editing.rs        list continuation, wrap toggles
crates/focal        GPUI app (via gpui-kit 0.7.0, which pins gpui-pre 0.3.7)
  editor.rs         list rows, caret and selection painting, hit testing, actions, input handler, autosave, reload
  document.rs       file IO and change stamps
  main.rs           CLI, detaching, fonts, window
```

The source text is the only document model, as agreed in the design. After each change, `focal-core` reparses the whole text and rebuilds the view of every line; the editor then diffs a hash per row and splices only changed rows into GPUI's virtualized `list`. The list keeps its scroll anchor as an item plus offset, so rows changing height above the viewport do not move what you are looking at. Each row is a GPUI `StyledText` whose display string omits hidden markers; the offset map translates caret, selection, clicks and IME ranges between display and source.

GPUI Kit is used for app setup, the root view and the reload banner's buttons. Its own `Editor` component is a code editor with a fixed line height, so it cannot show headings in larger type or hide syntax; the core editor is custom on raw GPUI.

## Measurements

Apple M4, macOS 27, release build, 5,007-line / 239 KB document with 50 tables and 50 code blocks (`mise run stress`):

| Step | Time |
| --- | --- |
| Parse and analyze the whole document | 2.0 ms |
| Build the display of every line | 2.0 ms |
| Per keystroke in the running app (analysis + restyle + row diff, from `FOCAL_TRACE=1`) | 3.7–4.5 ms |

That is inside the design's 8 ms keystroke budget without incremental parsing. It does not include GPUI's own layout and paint of the visible rows, which the spike did not instrument; typing and scrolling in the stress document felt immediate. The release binary is 20 MB. A clean debug build of the dependency tree takes about 3 minutes and `target/` reached 3.4 GB with debug and release builds.

## Problems found and how the spike handles them

1. **iA Writer Quattro S Bold cannot be selected.** The static Quattro S Bold and Bold Italic files in iA's repository report weight 400 in their OS/2 table, the same as Regular, and GPUI (through CoreText traits) selects faces by that weight. Duo S Bold correctly reports 700. The spike bundles Duo S Bold and Bold Italic for bold prose; their design matches Quattro closely, but it is not Quattro. Fixing it properly means an upstream GPUI change (also consider the font's bold style bit) or a renamed, corrected build of the fonts (the reserved font name forbids modifying them under the "iA Writer" name).
2. **No paragraph styles.** GPUI text has no hanging or first-line indent, so a wrapped list item continues under the bullet, not under its text. A fix is to draw the marker as its own fixed-width element beside the text layout.
3. **Run backgrounds fill the whole line height** and cannot be padded or rounded, so inline code backgrounds joined into bars across lines. Inline code is tinted instead; a pill background would need custom painting from glyph positions.
4. **Caret affinity at soft wraps** is not handled by GPUI: an offset at a wrap boundary is placed at the end of the earlier visual line. The editor corrects this itself, or Down gets stuck there.
5. **Absolutely positioned children need explicit insets.** Without `top_0()`/`left_0()`, Taffy placed the per-row overlay canvases after the text, which put every row's hit-test bounds one row too low. Clicks and drags landed a line above where they should; fixed.
6. **Revealing markers reflows text under the pointer.** While dragging a selection, only the markers at the caret are revealed; the whole selection reveals on mouse-up.
7. **`==highlight==` is not in pulldown-cmark 0.13.** The earlier parser decision assumed it was. The spike finds it inside text events, which misses a highlight split across events.

## What GPUI does not give Focal (compared with AppKit)

- **Accessibility.** GPUI has AccessKit support (`examples/a11y.rs`), but the custom text element exposes nothing to VoiceOver yet. Making an editor accessible is real work in either stack; AppKit's `NSTextView` provides it for free.
- **System text services:** spell checking, grammar, autocorrect, Look Up, Writing Tools and the services menu are absent. IME and the character palette work through the input handler.
- **Document infrastructure:** no `NSDocument`, so no versions browser, native tabs, recent documents or file coordination. The spike polls the file instead of using `NSFilePresenter`/FSEvents.
- **Mermaid and math** need new approaches: no `WKWebView` in GPUI and no SwiftMath. Candidates are rendering to SVG out of process (for example `mmdc`, or KaTeX/MathJax through a JavaScript engine) and showing the SVG with GPUI's `svg()`/`img()`, or a Rust math typesetter.
- **App shell:** single-instance forwarding (a second `focal file.md` bringing the window forward), an `.app` bundle, signing and notarization are not done; each `focal` call starts its own process.

## What GPUI does well here

- Rendering and scrolling are smooth, the virtualized list keeps position when rows above change height, and the stable-rendering failure of the Electron prototype did not appear.
- The whole editor model is ordinary, UI-free Rust with fast unit tests; the design's C interface between Swift and Rust disappears.
- IME, clipboard, key bindings, light/dark appearance and transparent title bars are available directly.

## Open questions for choosing a stack

- Is losing system spell checking and VoiceOver-by-default acceptable, or worth building (AccessKit for the text element; a spell-check service through AppKit bindings)?
- Is the Duo-for-bold workaround acceptable until the Quattro weight problem is fixed upstream?
- Is depending on `gpui-pre`/`gpui-kit` (a community-published snapshot of Zed's in-tree GPUI, pinned exactly) acceptable for a long-lived app?
