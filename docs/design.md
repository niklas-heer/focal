# Focal design

This document describes what Focal should become and how it is built. Each section is marked:

- **Agreed**: settled with Niklas during the design conversation on 2026-09-27 to 2026-09-29.
- **Proposed**: a recommended direction not yet reviewed. Change it freely; record the outcome.

Revised on 2026-09-30, when Niklas chose Rust and GPUI over Swift and AppKit ([decision](../decisions/2026-09-30_190238170_build-focal-in-rust-with-gpui.md)). Behavior agreed earlier still stands; the implementation sections describe the Rust stack.

Lasting choices are recorded in [`decisions/`](../decisions/).

## 1. Goal and principles (Agreed)

Focal is a native macOS Markdown editor for reading and writing plain `.md` files, launched from the terminal. It combines iA Writer's minimal, typographic calm with Bear's live rendering, and it should be the best Markdown editor on the Mac.

The primary use is opening one file to read or review it, often a file in a Git repository or one an agent just wrote. The secondary use is opening a folder and moving between a few files.

Principles, in priority order:

1. **The file is the document.** The text on disk is the source of truth. Opening and saving a file without editing it produces identical bytes. Focal changes how Markdown looks, not what it says.
2. **Native first.** Native Rust with GPUI, Zed's GPU-accelerated UI framework, using macOS services such as the system spell checker through `objc2` bindings. No web views, except an isolated Mermaid renderer (section 7).
3. **Quiet by default.** Chrome appears only when asked for: the formatting bar at the bottom edge, the file list on a shortcut, focus mode on a toggle.
4. **Correct Markdown.** CommonMark and GitHub-flavored Markdown are rendered the way GitHub renders them, as far as the parser allows.
5. **Stable while editing.** Rendering must never flicker, jump or lose styling while scrolling, typing or clicking. The Electron prototype failed here; the GPUI spike and Milestone 0 tested it first.

### Non-goals

- A notes library with its own database, tags or sync (Bear's model). Focal edits files in place.
- Cross-platform support.
- Reformatting Markdown to a canonical style (except edited tables, section 5).
- Mac App Store distribution and sandboxing, for now.

## 2. Feature scope (Agreed)

- CommonMark plus GitHub-flavored Markdown: tables, task lists, strikethrough, autolinks and footnotes.
- Fenced code blocks with syntax highlighting.
- Images: local relative paths and remote URLs, shown inline.
- GitHub alerts: `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]` and `[!CAUTION]`, shown as colored callouts.
- YAML front matter, shown as a quiet metadata block.
- Math: `$…$` inline and `$$…$$` display, typeset natively.
- Mermaid diagrams, rendered as images.
- Wiki links `[[name]]`, resolved against the open folder.
- `==highlight==`.
- Light and dark mode, following the system appearance.
- Focus mode, a bottom formatting bar, folder mode with a quick switcher, live reload and autosave.

## 3. App structure, files and the `focal` command (Agreed)

### Command line

| Command | Behavior |
| --- | --- |
| `focal notes.md` | Opens the file, creating it if it does not exist. |
| `focal .` or `focal ~/notes` | Opens the folder in folder mode. |
| `cat x.md \| focal` | Opens standard input as a new untitled document. |
| `focal --wait file.md` | Blocks until the window closes, so Focal works as `$EDITOR`. |
| `focal file.md` (already open) | Brings the existing window to the front. |

Today `focal` is the app itself: it relaunches in the background unless `--wait` is given. Proposed for distribution (M6): `Focal.app` contains the binary; a second `focal` call forwards the request to the running instance over a local socket instead of starting another process. The Homebrew cask in `niklas-heer/homebrew-tap` links it onto `PATH`, as with Keywink.

**Agreed (2026-09-30):** the app can install the command itself, for installs that do not come through Homebrew. A menu item ("Install Command Line Tool…") puts `focal` on `PATH`, so a file opens quickly from the terminal. Proposed details: a symbolic link from `/usr/local/bin/focal` to the binary inside `Focal.app`, asking for an administrator password only when that folder is not writable, and an "Uninstall" counterpart.

### Updates

**Agreed (2026-09-30):** Focal updates itself. A setting turns automatic update checks on or off. When a newer version exists, Focal asks whether to download it ("A new version of Focal is available. Do you want to download it?") and never installs without that consent.

Proposed details: Sparkle 2, as in Keywink and Spokn, with an appcast published per GitHub release and signed with the existing Sparkle key; Sparkle's standard prompt provides the question, release notes and "Skip This Version". Focal loads `Sparkle.framework` from the app bundle through `objc2`. Homebrew installs mark the cask `auto_updates true`, so Homebrew does not fight Sparkle.

### Documents

- One document per window, managed by Focal itself. There is no `NSDocument`, so macOS versions ("Revert to…"), native window tabs and the recent-documents menu are not provided unless built later.
- **Autosave**, no ⌘S required.
- **Live reload.** When the file changes on disk and there are no unsaved local edits, Focal reloads quietly and keeps the scroll position. The spike polls the file every second; M1 watches it through FSEvents. When both sides changed, a banner offers "Keep mine" or "Load theirs". Focal never silently overwrites.
- **Exact round-trip.** Line endings (LF or CRLF), the trailing newline and all whitespace are preserved. Files are read and written as UTF-8. The open question of non-UTF-8 input is in section 10.

### Folder mode

- One window with a sidebar of Markdown files. The sidebar is collapsed by default and toggled with a shortcut.
- The file list updates live through FSEvents.
- ⌘P opens a fuzzy quick switcher by file name.
- Switching files saves the current one first.
- Wiki links resolve against this folder.

### Stack

- Rust (pinned toolchain), a Cargo workspace. mise runs build, test, check, stress and clean tasks.
- `focal-core`: the UI-free editor model: Markdown analysis with `pulldown-cmark`, per-line display maps, the buffer with undo, Markdown-aware edits and accessibility text chunks. Unit tested.
- `focal`: the GPUI app, through `gpui-kit` (pinned exactly; it pins `gpui-pre`, a published snapshot of Zed's GPUI). macOS services come through `objc2` bindings.
- Developer ID signing and notarization reuse the existing setup (`keywink-notary` profile). Distribution is through the Homebrew tap.

## 4. Rendering engine (Agreed approach, Proposed details)

**Agreed:** the buffer holds the actual Markdown text. Focal draws it with syntax markers hidden or replaced away from the caret. Blocks that cannot be plain text are shown as embedded elements. See [decision: keep the Markdown source as the document model](../decisions/2026-09-28_221517982_keep-the-markdown-source-as-the-document-model.md).

### Pipeline (Agreed approach, Proposed details)

1. The buffer holds the file's exact text (`focal-core::Buffer`), with undo.
2. After each edit, `focal-core` parses the whole text with `pulldown-cmark` (`into_offset_iter()`) into styles, markers with reveal rules, line kinds, tables and links, all as byte ranges into the unchanged source.
3. For each line it builds a display: the text with hidden markers removed or replaced, styled runs, and a map between display and source offsets.
4. The editor shows one row per line (a table away from the caret is one row) in GPUI's virtualized `list`. After a change, rows are diffed by hash and only changed rows are spliced, so unchanged rows keep their measured heights and the scroll anchor holds.
5. Caret, selection, clicks, IME and accessibility translate between display and source offsets through the per-line maps.

Measured on an Apple M4 (release): full parse 2.0 ms and restyle of every line 2.0 ms on the 5,000-line fixture, inside the 8 ms budget. Incremental parsing is not needed yet; the accessibility tree (about 12 ms per frame on that fixture while an assistive app is connected) needs caching first.

### Line layout (Agreed)

The spike draws each line as one GPUI `StyledText`, which has no paragraph styles: wrapped list items do not hang under their text, bullets and task boxes are glyphs, and run backgrounds cannot be padded. M1 draws each line's prefix (its quote and list levels, from `focal-core`'s `LinePrefix`) as real elements in columns beside the content: a bar per quote level, and per list level a marker column holding a bullet, the item's number or a clickable checkbox. The content's text wraps inside its own column, so continuation lines hang under the text. Zed's Markdown renderer lays out list items the same way. Inline elements inside a line, which Zed's editor supports (`LineFragment::Element`), are not needed yet.

### Showing and hiding syntax (Proposed)

- Inline markers (`**`, `_`, `` ` ``, `~~`, `==`, link brackets and URLs) are hidden unless the caret or selection touches that span. They then appear dimmed.
- Block markers (`#`, `>`, list bullets, fences) are hidden or restyled unless the caret is on that line. A heading's `#` appears in the margin when the caret is on the heading.
- Arrow keys never stop on hidden characters. Moving across a hidden marker takes one keystroke.
- Copy puts the Markdown source on the pasteboard, plus rich text for other apps.
- Links open with ⌘-click; a plain click places the caret.

### Embedded views (Proposed)

Tables, display math, Mermaid diagrams, images and front matter are "islands": elements that take the place of their source lines (like blocks that replace rows in Zed's block map) while their source stays in the buffer. Clicking an island edits it in its own way: grid editing for tables (section 5), and source with a live preview for math and Mermaid.

### How the engine was chosen

Milestone 0 compared TextKit 1 and TextKit 2 in a throwaway AppKit spike ([findings](spikes/2026-09-29-m0-textkit.md)); it narrowly favored TextKit 1, with a 15 ms keystroke cost and table problems on both. In parallel, a GPUI spike built the same app in Rust ([findings](gpui-spike.md)), and a follow-up gate built spell checking and VoiceOver support on it. Niklas chose GPUI on 2026-09-30.

## 5. Tables (Agreed)

**Agreed:**

- Every table shows as a real grid, never as pipe characters.
- You edit directly in the grid: click a cell and type.
- Tab and Shift-Tab move between cells; Return at the end adds a row.
- Hover controls add, delete and drag rows and columns and set column alignment.
- Wide tables scroll horizontally inside their own frame, so the text column keeps its width.
- When a table is edited, its source is rewritten with **aligned columns**. Unedited tables are never rewritten.

**Serializer rules (Agreed, built in M2):**

- Pad each cell with spaces to the widest cell in its column. Width is measured in display columns; East Asian wide characters count as two.
- Delimiter row: dashes to the column width, with `:` for left, right or center alignment.
- Escape `|` inside cells as `\|`. Inline Markdown in cells (bold, code, links) stays as source text.
- Keep the table's original indentation and line endings.

**Editing details (Agreed, built in M2):**

- A cell shows rendered inline Markdown. The focused cell is edited as source in a plain input, without inline styling while editing.
- Each cell edit, and each row or column operation, is one undo step.
- The arrow keys move into a table and out of it at its first and last rows.
- "Edit as Markdown" in a cell's context menu shows the table's source until the caret leaves it.
- Columns are as wide as their widest cell, up to a limit at which cells wrap.
- A table the parser does not recognize stays plain text.

## 6. Chrome, typography and focus (Agreed behavior, Proposed details)

### Bottom formatting bar (Agreed)

- It lives at the **bottom edge**.
- It fades in when the pointer comes within about 40 pt of the bottom, and fades out shortly after the pointer leaves.
- It never appears while you type.
- It holds the file name, heading level, bold, italic, strikethrough, inline code, link, lists, task, quote, table, code block, math and a focus toggle.
- Proposed: word count and reading time on its trailing side. Liquid Glass material.

### Focus mode (Agreed)

- Off by default, toggled with a shortcut (proposed: ⌘D, as in iA Writer).
- Dims everything except the current sentence or paragraph; the unit is a setting.
- Typewriter scrolling keeps the current line vertically centered.
- Hides the sidebar and the bar.

### Typography and themes (Proposed)

- Prose in iA Writer Quattro; code in iA Writer Mono. Settings offer Quattro, Duo or Mono for prose. Bold prose currently uses Duo S Bold, because the static Quattro S Bold files report weight 400 and GPUI cannot select them.
- Fonts are bundled unmodified with their SIL Open Font License 1.1. "iA Writer" is a reserved font name, so the files must not be modified (for example subset) under that name.
- A text column of about 70 characters with generous margins that scale with the window.
- Light and dark palettes follow the system: iA-like warm off-white and near-black backgrounds, low-contrast syntax markers and restrained accent colors.

### Settings (Proposed)

Kept deliberately small: prose font, text size, column width, focus unit, typewriter scrolling, and whether to check for updates automatically (Agreed, see [Updates](#updates)).

## 7. Extras (Agreed scope, Proposed implementation)

| Feature | Proposed approach |
| --- | --- |
| Syntax highlighting | tree-sitter grammars through GPUI Kit's highlighter (its `tree-sitter-*` features), as Zed does. |
| GitHub alerts | Blockquote with alert kind from `pulldown-cmark`; colored bar, icon and title. |
| Front matter | Metadata block from `pulldown-cmark`; quiet collapsed key/value view that expands to source on click. |
| Footnotes | Superscript references; hover previews the note; click jumps and back returns. |
| Math | Open (section 10): typeset to SVG (for example MathJax in an embedded JavaScript engine, or a Rust typesetter) and draw the SVG with GPUI. Inline math sits in the line; display math is an island. |
| Mermaid | Rendered to SVG off the main thread by an isolated renderer (a hidden `WKWebView` through `objc2`, or an external renderer), only when a document contains a Mermaid block. SVGs are cached by content hash and appearance and shown as island images. |
| Wiki links | `[[name]]` and `[[name\|label]]` resolve to files in the open folder; unresolved links are styled differently; ⌘-click opens or creates. |
| Images | Relative paths resolve against the document; remote images load asynchronously with a cache; images scale to the column width. |

## 8. Milestones (Proposed)

| Milestone | Outcome |
| --- | --- |
| **M0 — Engine spikes** | Done: TextKit 1 vs 2, the GPUI spike and the spell-checking and VoiceOver gate. GPUI chosen. |
| **M1 — Core editor** | Done: line prefixes (quote bars, bullets, numbers, clickable checkboxes) drawn beside the text with hanging indents; the caret never stops inside a prefix; list editing (Return, Tab, Shift-Tab, Backspace at a marker); live reload through FSEvents; cached accessibility tree; code highlighting with tree-sitter; GPUI integration tests. |
| **M2 — Tables** | Done: every table is a grid with cells edited in place; Tab, Return and the arrow keys move between cells; hover buttons, a context menu and drag handles add, delete, move and align rows and columns; edited tables are rewritten aligned, and a cell edit is one undo step; wide tables scroll sideways. |
| **M3 — Chrome** | Bottom bar, focus mode with typewriter scrolling, folder mode with sidebar and quick switcher. |
| **M4 — Extras** | Alerts, front matter, footnotes, highlight, images, math and wiki links. |
| **M5 — Mermaid** | Lazy renderer, cache and island. |
| **M6 — Distribution** | `Focal.app` bundle, single-instance forwarding for the `focal` command, "Install Command Line Tool…", automatic updates with a setting and a download prompt, signing, notarization, Homebrew cask, release process. |

Each milestone gets its own implementation plan before work starts.

## 9. Testing (Proposed)

- **Parser:** Rust unit tests for span output, plus the CommonMark and GFM spec examples, checked for correct span ranges.
- **Round-trip:** property tests where opening and saving any unedited document produces identical bytes, including CRLF, missing trailing newline, tabs and non-ASCII text.
- **Tables:** golden tests for the serializer (alignment, escaping, wide characters) and grid-edit-to-source tests.
- **Rendering:** snapshot tests of rendered documents in light and dark mode.
- **Interaction:** GPUI integration tests through GPUI Kit's test harness (headless windows, simulated keys and mouse) for syntax reveal, caret movement across hidden markers and islands, list editing and undo.
- **Performance:** `mise run stress` on the 5,000-line document for parse and restyle time, plus in-app traces (`FOCAL_TRACE=1`); keystroke budget 8 ms.

## 10. Open questions

- Math typesetting without a web view: which renderer?
- Grammar checking, autocorrect and Writing Tools: wanted, and reachable through `objc2`?
- How to follow GPUI upgrades: `gpui-kit` pins `gpui-pre` exactly.
- Encodings other than UTF-8: refuse, or detect and preserve?
- Architecture: Apple Silicon only, or universal binaries (Intel)?
- Whether focus mode should also dim syntax markers and chrome colors, as iA Writer does.
- Printing and PDF export: wanted, and in which milestone?
- Name collision check for "Focal" before any public release.
