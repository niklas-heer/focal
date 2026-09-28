# Focal design

This document describes what Focal should become and how it is built. Each section is marked:

- **Agreed**: settled with Niklas during the design conversation on 2026-09-27 to 2026-09-29.
- **Proposed**: a recommended direction not yet reviewed. Change it freely; record the outcome.

Lasting choices are recorded in [`decisions/`](../decisions/).

## 1. Goal and principles (Agreed)

Focal is a native macOS Markdown editor for reading and writing plain `.md` files, launched from the terminal. It combines iA Writer's minimal, typographic calm with Bear's live rendering, and it should be the best Markdown editor on the Mac.

The primary use is opening one file to read or review it, often a file in a Git repository or one an agent just wrote. The secondary use is opening a folder and moving between a few files.

Principles, in priority order:

1. **The file is the document.** The text on disk is the source of truth. Opening and saving a file without editing it produces identical bytes. Focal changes how Markdown looks, not what it says.
2. **Native first.** Swift and AppKit. No web views, except the isolated Mermaid renderer (section 7).
3. **Quiet by default.** Chrome appears only when asked for: the formatting bar at the bottom edge, the file list on a shortcut, focus mode on a toggle.
4. **Correct Markdown.** CommonMark and GitHub-flavored Markdown are rendered the way GitHub renders them, as far as the parser allows.
5. **Stable while editing.** Rendering must never flicker, jump or lose styling while scrolling, typing or clicking. The Electron prototype failed here, so Milestone 0 tests it before anything else.

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

`focal` is a small helper executable inside `Focal.app`. It forwards requests to the running app, launching it if needed. The Homebrew cask in `niklas-heer/homebrew-tap` links it onto `PATH`, as with Keywink.

### Documents

- One `NSDocument` per file. This provides autosave in place, macOS versions ("Revert to…"), recent documents, native window tabs and notice of external changes (`NSFilePresenter`).
- **Autosave**, no ⌘S required.
- **Live reload.** When the file changes on disk and there are no unsaved local edits, Focal reloads quietly and keeps the scroll position. When both sides changed, a banner offers "Keep mine" or "Load theirs". Focal never silently overwrites.
- **Exact round-trip.** Line endings (LF or CRLF), the trailing newline and all whitespace are preserved. Files are read and written as UTF-8. The open question of non-UTF-8 input is in section 10.

### Folder mode

- One window with a sidebar of Markdown files. The sidebar is collapsed by default and toggled with a shortcut.
- The file list updates live through FSEvents.
- ⌘P opens a fuzzy quick switcher by file name.
- Switching files saves the current one first.
- Wiki links resolve against this folder.

### Stack

- Swift 6 with strict concurrency, AppKit, minimum macOS 26, so the bottom bar can use Liquid Glass.
- `focal-markdown`: a Rust library wrapping `pulldown-cmark`, exposed through a C interface and linked statically (section 4).
- The Xcode project is generated from `project.yml` with XcodeGen; mise runs build, test, check and release tasks, following Keywink.
- Developer ID signing and notarization reuse the existing setup (`keywink-notary` profile). Distribution is through the Homebrew tap.

## 4. Rendering engine (Agreed approach, Proposed details)

**Agreed:** one native text view holds the actual Markdown text. Focal styles it with attributes and hides syntax markers. Blocks that cannot be plain text are shown as embedded native views. See [decision: keep the Markdown source as the document model](../decisions/2026-09-28_221517982_keep-the-markdown-source-as-the-document-model.md).

### Pipeline (Proposed)

1. The text view's storage holds the file's exact text.
2. After each edit, Swift passes the full text to `focal-markdown`, which parses it with `pulldown-cmark` (`Parser::new_ext(...).into_offset_iter()`). All needed options are enabled: tables, footnotes, strikethrough, task lists, YAML metadata blocks, math, GFM alerts, wiki links and highlight.
3. `focal-markdown` returns a flat array of spans: kind, byte range, content range and attributes such as heading level, alignment or alert kind. Marker ranges are the span range minus the content range.
4. Swift converts UTF-8 byte offsets to UTF-16 offsets, the unit `NSString` uses, and computes a style diff against the previous pass. Only changed ranges are restyled.
5. Styling applies fonts, colors and paragraph styles, hides markers and places embedded views.

Parsing the whole document on each keystroke is expected to be fast enough. Milestone 0 measures it; if it is not, reparse only from the edited block onward.

### Showing and hiding syntax (Proposed)

- Inline markers (`**`, `_`, `` ` ``, `~~`, `==`, link brackets and URLs) are hidden unless the caret or selection touches that span. They then appear dimmed.
- Block markers (`#`, `>`, list bullets, fences) are hidden or restyled unless the caret is on that line. A heading's `#` appears in the margin when the caret is on the heading.
- Arrow keys never stop on hidden characters. Moving across a hidden marker takes one keystroke.
- Copy puts the Markdown source on the pasteboard, plus rich text for other apps.
- Links open with ⌘-click; a plain click places the caret.

### Embedded views (Proposed)

Tables, display math, Mermaid diagrams, images and front matter are "islands": native views embedded in the text flow while their source stays in storage. Clicking an island edits it in its own way: grid editing for tables (section 5), and source with a live preview for math and Mermaid.

### Text engine choice: Milestone 0 (Agreed)

TextKit 2 is better at embedding live views (`NSTextAttachmentViewProvider`) and lays out only the visible area. TextKit 1 hides characters cleanly (null glyphs through the layout manager delegate) and has proven, stable scrolling. The choice is made by a throwaway spike that builds both against the same cases:

- Hide and reveal markers as the caret moves, with no stuck caret positions, and correct copy.
- An editable table grid embedded in the text, whose height changes without scroll jumps.
- A 5,000-line document with 50 tables and 50 code blocks: smooth scrolling, no layout jumps, and restyling after a keystroke in under 8 ms on Niklas's Mac.
- Undo and redo across text edits and grid edits.
- VoiceOver still reads the document.

The result is a decision record naming the engine, with measurements. The spike code is not kept.

## 5. Tables (Agreed behavior, Proposed details)

**Agreed:**

- Every table shows as a real grid, never as pipe characters.
- You edit directly in the grid: click a cell and type.
- Tab and Shift-Tab move between cells; Return at the end adds a row.
- Hover controls add, delete and drag rows and columns and set column alignment.
- Wide tables scroll horizontally inside their own frame, so the text column keeps its width.
- When a table is edited, its source is rewritten with **aligned columns**. Unedited tables are never rewritten.

**Proposed serializer rules:**

- Pad each cell with spaces to the widest cell in its column. Width is measured in display columns; East Asian wide characters count as two.
- Delimiter row: dashes to the column width, with `:` for left, right or center alignment.
- Escape `|` inside cells as `\|`. Inline Markdown in cells (bold, code, links) stays as source text.
- Keep the table's original indentation and line endings.

**Proposed editing details:**

- A cell shows rendered inline Markdown. While editing it shows source with styling, like the main text.
- Grid edits register with the document's undo manager as single, named actions.
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

- Prose in iA Writer Quattro; code in iA Writer Mono. Settings offer Quattro, Duo or Mono for prose.
- Fonts are bundled unmodified with their SIL Open Font License 1.1. "iA Writer" is a reserved font name, so the files must not be modified (for example subset) under that name.
- A text column of about 70 characters with generous margins that scale with the window.
- Light and dark palettes follow the system: iA-like warm off-white and near-black backgrounds, low-contrast syntax markers and restrained accent colors.

### Settings (Proposed)

Kept deliberately small: prose font, text size, column width, focus unit and typewriter scrolling.

## 7. Extras (Agreed scope, Proposed implementation)

| Feature | Proposed approach |
| --- | --- |
| Syntax highlighting | Open question (section 10): tree-sitter grammars (SwiftTreeSitter with Neon) or Highlightr (highlight.js in JavaScriptCore). Decide in Milestone 1. |
| GitHub alerts | Blockquote with alert kind from `pulldown-cmark`; colored bar, icon and title. |
| Front matter | Metadata block from `pulldown-cmark`; quiet collapsed key/value view that expands to source on click. |
| Footnotes | Superscript references; hover previews the note; click jumps and back returns. |
| Math | SwiftMath (native LaTeX math typesetting for macOS). Inline math is drawn in the line; display math is an island. |
| Mermaid | A single hidden `WKWebView` with a bundled, offline `mermaid.js`, created only when a document contains a Mermaid block. Renders SVG, which is cached by content hash and appearance and shown as an island image. |
| Wiki links | `[[name]]` and `[[name\|label]]` resolve to files in the open folder; unresolved links are styled differently; ⌘-click opens or creates. |
| Images | Relative paths resolve against the document; remote images load asynchronously with a cache; images scale to the column width. |

## 8. Milestones (Proposed)

| Milestone | Outcome |
| --- | --- |
| **M0 — Engine spike** | Throwaway TextKit 1 vs TextKit 2 comparison against section 4's cases. Output: a decision record with measurements. |
| **M1 — Core editor** | Project setup (XcodeGen, mise, the Rust library), `focal` command, `NSDocument` with autosave, live reload and exact round-trip, parsing, inline and block styling with syntax reveal, code blocks with highlighting, fonts, light and dark mode. Usable daily for plain prose. |
| **M2 — Tables** | Grid island, cell editing and navigation, row and column operations, aligned serialization, undo. |
| **M3 — Chrome** | Bottom bar, focus mode with typewriter scrolling, folder mode with sidebar and quick switcher. |
| **M4 — Extras** | Alerts, front matter, footnotes, highlight, images, math and wiki links. |
| **M5 — Mermaid** | Lazy renderer, cache and island. |
| **M6 — Distribution** | Signing, notarization, Homebrew cask, release process. |

Each milestone gets its own implementation plan before work starts.

## 9. Testing (Proposed)

- **Parser:** Rust unit tests for span output, plus the CommonMark and GFM spec examples, checked for correct span ranges.
- **Round-trip:** property tests where opening and saving any unedited document produces identical bytes, including CRLF, missing trailing newline, tabs and non-ASCII text.
- **Tables:** golden tests for the serializer (alignment, escaping, wide characters) and grid-edit-to-source tests.
- **Rendering:** snapshot tests of rendered documents in light and dark mode, following Keywink's `mise run snapshots` pattern.
- **Interaction:** tests for syntax reveal, caret movement across hidden markers, and undo.
- **Performance:** a benchmark on the 5,000-line document for open time, keystroke restyle time and scrolling, with budgets set from the M0 measurements.

## 10. Open questions

- Code highlighting: tree-sitter (accurate, incremental, one compiled grammar per language) or Highlightr (about 190 languages, JavaScript-based). Decide in M1.
- Encodings other than UTF-8: refuse, or detect and preserve?
- Architecture: Apple Silicon only, or universal binaries (Intel)?
- Whether focus mode should also dim syntax markers and chrome colors, as iA Writer does.
- Printing and PDF export: wanted, and in which milestone?
- Name collision check for "Focal" before any public release.
