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
2. **Native first.** Native Rust with GPUI, Zed's GPU-accelerated UI framework, using macOS services such as the system spell checker through `objc2` bindings. No web views in the editor (Mermaid renders natively since M5); only printing lays out its pages in a WebKit view that is never shown (M12).
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

Built in M6: `Focal.app` contains the binary, `Contents/MacOS/focal`. Every `focal` call hands its request to the running instance over a Unix socket in `$TMPDIR` (one per binary, so a development build never reaches the installed app), starting the instance in the background if none runs; `--wait` holds the connection until the window closes. Finder's Open With and the Dock reach the same instance. The Homebrew cask (template in `packaging/focal.rb`) also links `focal` onto `PATH`.

**Agreed (2026-09-30):** the app can install the command itself, for installs that do not come through Homebrew. A menu item ("Install Command Line Tool…") puts `focal` on `PATH`, so a file opens quickly from the terminal. Built in M6: a symbolic link from `/usr/local/bin/focal` to the binary inside `Focal.app`, asking for an administrator password only when that folder is not writable; a link to another Focal is replaced, any other file is left alone and reported; "Uninstall Command Line Tool…" removes only Focal's link.

### Updates

**Agreed (2026-09-30):** Focal updates itself. A setting turns automatic update checks on or off. When a newer version exists, Focal asks whether to download it ("A new version of Focal is available. Do you want to download it?") and never installs without that consent.

Built in M6: Sparkle 2.10, as in Keywink and Spokn, with an appcast attached to each GitHub release and signed with the shared Sparkle key; Sparkle's standard prompt asks "A new version of Focal is available! … Would you like to download it now?" and offers release notes and "Skip This Version". Focal loads `Sparkle.framework` from the app bundle through `objc2`, one of the few places `unsafe` code is allowed ([decision](../decisions/2026-09-30_225003772_update-focal-with-sparkle-through-objc2.md), accepted). The settings switch drives Sparkle's automatic checks; "Check for Updates…" is in the Focal menu. Homebrew installs mark the cask `auto_updates true`, so Homebrew does not fight Sparkle. The release steps are in [`RELEASE.md`](../RELEASE.md).

### Documents

- One document per window, managed by Focal itself. There is no `NSDocument`, so macOS versions ("Revert to…") and native window tabs are not provided unless built later.
- Built in M7: ⌘N opens an untitled window, ⌘O the open panel (files and folders, several at once; an open file's window comes forward). File ▸ Open Recent lists the last ten files and folders opened (kept in `recent.json` beside the settings, missing ones left out) and macOS's Dock menu gets them too.
- Untitled text is never dropped silently: closing an untitled document with unsaved text asks Save…, Don't Save or Cancel; quitting with such documents asks before discarding them. Logging out, a restart or a crash cannot ask, so untitled text is also kept as a draft in `~/Library/Application Support/Focal/Drafts` (with the autosave delay); the next launch reopens drafts as unsaved untitled documents, and saving or discarding removes them. Closing over edits that conflict with the disk asks too (Keep Mine, Discard Mine, Cancel).
- **Autosave**, no ⌘S required.
- **Live reload.** When the file changes on disk and there are no unsaved local edits, Focal reloads quietly and keeps the scroll position. The spike polls the file every second; M1 watches it through FSEvents. When both sides changed, a banner offers "Keep mine" or "Load theirs". Focal never silently overwrites.
- **Find and replace** (M7): ⌘F opens a find bar seeded with the selected text; matches ignore case (character by character, without full case folding), are highlighted as you type and follow edits. Return and ⇧Return, or ⌘G and ⇧⌘G, step through them and wrap; ⌘G still works after the bar closes. ⌥⌘F adds a replacement field: Replace changes the selected match only when one is selected, Replace All is one undo step.
- **Go to Heading** (M8): ⇧⌘O lists the headings, indented by level; typing narrows them and Return moves there, with ⌘[ to come back.
- **Export and copy** (M8): ⇧⌘E writes a standalone HTML page in Focal's typography (light and dark) with math and Mermaid diagrams as SVG, wiki links to the files they open, and relative images kept working when the page is saved elsewhere. ⌥⇧⌘C puts the selection (or the document) on the pasteboard as HTML and as Markdown text. Raw HTML in the document is exported as written.
- **Print and PDF** (M12): Print… (⌥⌘P; ⌘P stays the quick switcher) and Export as PDF… set the same page as Export as HTML for paper: light whatever the appearance, A4 or Letter with margins, headings kept with their text, folded callouts open, in the iA Writer typefaces. WebKit lays it out in a web view that is never shown and AppKit prints it; Print… shows the print panel with a preview, Export as PDF… writes the file without asking ([decision](../decisions/2026-10-01_194827966_print-and-export-pdf-through-webkit.md), accepted).
- **Spelling, grammar and correction** (M12): macOS's checker underlines misspelled words in red and what its grammar checker has a suggestion for in green, in prose only, never under the caret; right-click offers guesses, or the grammar note and its corrections. A finished misspelled word is corrected like other Mac apps do, as its own undo step, while macOS's "Correct spelling automatically" and Focal's setting are on. Each line is checked in the language it is written in. Apple's grammar checker is rule-based: it catches "a apple" but not "This are a test".
- **Writing Tools** (M12): Edit ▸ Writing Tools (Proofread, Rewrite, Make Friendly, Summarize, …) works on the selection, or on the whole document when nothing is selected. Apple shows the result in its panel; Replace puts it in as one undo step. Focal adds the Services methods Writing Tools asks for to GPUI's view class at runtime; inline rewriting with animations (`NSWritingToolsCoordinator`) is not built. Writing Tools sees the Markdown source, so it may touch syntax too.
- **Exact round-trip.** The trailing newline and all whitespace are preserved. Files keep their encoding (UTF-8 with or without BOM, UTF-16, or Windows-1252 for other bytes) and line endings (LF, CRLF or classic Mac CR); the bar names anything other than plain UTF-8, and a character the encoding cannot hold makes the file UTF-8, with a note.

### Folder mode

- One window with a sidebar of Markdown files. The sidebar is collapsed by default and toggled with ⌃⌘S (the macOS "Show Sidebar" shortcut; ⌘\\ belongs to 1Password on many Macs).
- The folder opens its most recently changed Markdown file, or a new `Untitled.md` in it. Hidden entries, `node_modules` and `target` are skipped, and scanning stops at 5,000 files.
- The file list updates live through FSEvents.
- ⌘P opens a fuzzy quick switcher by file name (and folder path); with no query, the newest files come first.
- Switching files saves the current one first, including a table cell being edited.
- Wiki links resolve against this folder.
- **Agreed (2026-10-04):** a window opened on a single file has the sidebar and the quick switcher too, listing the Markdown files beside it (not in subfolders), the same files its wiki links resolve against. The list is read the first time ⌃⌘S or ⌘P asks for it and follows changes on disk. Such a window does not stand for the folder: `focal .` there still opens folder mode.

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
- It holds the file name, heading level, bold, italic, strikethrough, inline code, link, lists, task, quote, table, code block, math and a focus toggle. The same actions are in the Format menu, several with shortcuts (see the README).
- Built in M3: word count and reading time on its trailing side ("12 of 340 words" with a selection); it fades in over 150 ms and out one second after the pointer leaves; it hides on any key press.
- Proposed: Liquid Glass material.
- **Redesigned 2026-10-04, after Bear:** a floating rounded strip centered above the bottom edge, with a soft shadow, holding Lucide icons (ISC license, `assets/icons`, drawn in the text color) in groups: heading level, inline styles (bold, italic, strikethrough, highlight, code, link), blocks (bullets, numbers, tasks, quote), inserts (table, code block, math) and focus mode, then the word count. The file name left the bar, since the title shows it; a file that is not plain UTF-8 still says how it is stored beside the word count.

### Focus mode (Agreed)

- Off by default, toggled with ⌘D, as in iA Writer.
- Dims everything except the current sentence or paragraph; the unit is a setting. In a list, a sentence ends with its item. List markers, checkboxes and highlights dim with their text.
- Typewriter scrolling keeps the current line vertically centered (a setting, on by default), except while a table cell is edited.
- Hides the sidebar and the bar. **Agreed (2026-10-04):** the bar still comes up when the pointer reaches the bottom edge, as outside focus mode; entering focus mode with ⌘D hides it like any key press.
- Built in M12: syntax markers, checkboxes, quote bars and rules dim in the bright paragraph too, as in iA Writer, and so does the title bar.

### Typography and themes (Proposed)

- Prose in iA Writer Quattro; code in iA Writer Mono. Settings offer Quattro, Duo or Mono for prose. **Added 2026-10-04:** typefaces every Mac has: Charter, Georgia and Palatino (serif), Avenir Next, Helvetica Neue and SF Pro (sans) and Menlo, chosen from tiles that show each face. Code stays in iA Writer Mono, and export and print keep the iA Writer faces. Bold prose currently uses Duo S Bold, because the static Quattro S Bold files report weight 400 and GPUI cannot select them.
- Fonts are bundled unmodified with their SIL Open Font License 1.1. "iA Writer" is a reserved font name, so the files must not be modified (for example subset) under that name.
- A text column of about 70 characters with generous margins that scale with the window.
- Light and dark palettes follow the system: iA-like warm off-white and near-black backgrounds, low-contrast syntax markers and restrained accent colors.

### Settings (Proposed)

Kept deliberately small: prose font, text size, column width, focus unit, typewriter scrolling, whether to check for updates automatically (Agreed, see [Updates](#updates)), grammar checking and automatic correction (M12), and which optional diagram tools are installed (M12, with how to install them).

Built in M3: a settings window (⌘,) with the prose typeface (Quattro, Duo or Mono), text size (16, 18, 21 or 24 pt), column width (about 60, 70 or 85 characters), focus unit and typewriter scrolling, applied at once and saved to `~/Library/Application Support/Focal/settings.json`. The updates setting comes with M6.

Redesigned 2026-10-04: the window has a toolbar of panes, as macOS preferences do (⌘1 to ⌘5, or ⌃Tab, switch them): **Text** (a live preview in the chosen typeface and size, then appearance, typeface, size and line length), **Writing** (grammar, automatic correction), **Focus** (what stays bright, typewriter scrolling), **Diagrams** (which optional tools are installed and how to install the others) and **General** (updates with Check Now, the `focal` command with Install or Uninstall, the version). Each pane groups its settings on cards with a sentence on what each does; choices are segmented controls and on/off settings are switches. An **appearance** setting (System, Light or Dark) sets Focal's appearance alone; `FOCAL_APPEARANCE` still overrides it.

**About (2026-10-04).** Focal ▸ About Focal opens Focal's own window instead of AppKit's standard panel: the icon, the version and build, the tagline, links to the website, this version's release notes and a new issue, Check for Updates… in builds that update themselves, and the credits (iA Writer typefaces and their license, pulldown-cmark, MathJax, merman, GPUI).

### Corner buttons and the info panel (2026-10-04, after Bear)

Three quiet buttons sit in the title bar's right corner, faded in focus mode: **Outline**, **Info** and **More** (a menu of Go to Heading, Focus Mode, Toggle Dark Mode, copy, export, print, Show in Finder and Settings). Outline and Info open a panel at the window's right edge (⌥⌘O, ⌥⌘I), hidden in focus mode:

- **Info:** statistics (words, characters with and without spaces, sentences, paragraphs, reading time, and the selection's words and characters); the document (name, folder, which shows it in Finder, when it changed, its size and storage); appearance (Light, Dark or Auto, the typeface tiles, text size, line length); writing (focus mode, typewriter scrolling, Mac, Vim or Helix keys); and All Settings….
- **Outline:** the headings, indented by level, the caret's own marked; clicking one moves there, and ⌘[ comes back.

Toggle Dark Mode (⇧⌘L, View menu) switches Focal's own appearance to the opposite of what it shows.

### Menus and small conveniences (2026-10-04)

A Window menu (Minimize ⌘M, Zoom) and a Help menu (Focal Help, Keyboard Shortcuts, Report an Issue…) join the menu bar. View has Bigger Text (⌘=), Smaller Text (⌘−) and Default Text Size, which step the text size setting. Format lists all six heading levels and Highlight (⇧⌘H, `==text==`). File ▸ Show in Finder (⌥⌘R) reveals the document.

### Vim and Helix modes (Agreed feature, Proposed implementation)

**Agreed (2026-10-04):** Vim mode, and Helix mode beside it, can be chosen (Settings ▸ Writing ▸ Editing keys, or Edit ▸ Editing Keys: Standard, Vim or Helix); Standard is the default. Helix mode is selection first: `w`, `e`, `b`, `x`, `%`, `mi`/`ma` and `f`/`t` select, `v` extends, and `d`, `c`, `y`, `p`, `r`, `~`, `>`, `<` and `J` act on the selection; `gg`, `ge`, `gh`, `gl`, `gs`, `mm`, `u`/`U`, `/`, `n`, `*`, `.` and `:w`/`:q` work as in Helix. One selection only: Helix's multiple cursors are not built. Both modes share one face in `focal-core` (`modal.rs`). Built 2026-10-04 as a state machine in `focal-core` that the editor feeds keys and whose edits it applies ([decision](../decisions/2026-10-04_013139470_build-vim-mode-as-a-state-machine-in-focal-core.md), proposed): normal, insert, visual and visual-line modes, counts, operators, motions, text objects, `.`, undo, search through the find bar and `:w`, `:q`, `:wq` and `:N`. Normal mode draws a block cursor; the mode, pending keys and the command line show quietly in the bottom-left corner. Keys with ⌘ stay Focal's. `j` and `k` move by screen line; a change and its typing undo as one step; yanks reach the system clipboard; `o` continues lists.

## 7. Extras (Agreed scope, Proposed implementation)

Built in M4 unless marked otherwise.

| Feature | Approach |
| --- | --- |
| Syntax highlighting | tree-sitter grammars through GPUI Kit's highlighter (its `tree-sitter-*` features), as Zed does. |
| GitHub alerts | Blockquote with alert kind from `pulldown-cmark`; colored bar, icon (ⓘ ✦ ! ⚠ ⊘) and title. |
| Front matter | Metadata block from `pulldown-cmark`; one quiet line of keys and values that shows its source on click or when the caret enters; a document opens with the caret at its body. |
| Footnotes | References stay inline and colored (GPUI text runs cannot raise or shrink text); hovering previews the note; ⌘-click or ⌘↩ jumps between reference and note, ⌘[ returns. |
| Math | Display math (`$$ … $$` alone on its lines) is an island typeset by MathJax 3 in an embedded QuickJS engine, no web view ([decision](../decisions/2026-10-01_071745636_typeset-math-with-mathjax-in-an-embedded-quickjs.md), accepted); editing shows the source with a live preview. Inline math cannot be an image inside a text run, so away from the caret it reads as Unicode (Greek letters, operators, super- and subscripts, `e^(iπ)` where Unicode has no superscript); TeX that Unicode cannot show stays source. |
| Mermaid | Built in M5: a fenced `mermaid` block is an island drawn by `merman`, a headless Rust implementation of Mermaid, on a background thread, in Focal's palette for the appearance; cached by source and palette; editing shows the source with a live preview, and a parse error shows the parser's message. No web view ([decision](../decisions/2026-10-01_080601086_render-mermaid-natively-with-merman.md), accepted). |
| Wiki links | `[[name]]` and `[[name\|label]]` resolve to files in the open folder (or beside a single file), without regard to case, same folder first; unresolved links are drawn quietly; ⌘-click or ⌘↩ opens or creates (`<name>.md`, written on first save). Relative Markdown links and `#heading` anchors follow too; ⌘[ goes back. |
| Images | An image alone on its line is an island scaled to the column; with the caret on the line the source shows with the image below. Relative paths resolve against the document; remote images are downloaded once in the background into `~/Library/Caches/Focal/images`. |

## 7a. Compatibility (Agreed goal, Proposed details)

**Agreed (2026-10-01):** Focal is compatible with the Markdown that is out there. Whatever a file was written for (GitHub, GitLab, Obsidian, Pandoc, MkDocs, Docusaurus, VitePress, Jekyll or Hugo, MultiMarkdown, an LLM's answer), opening it gives something nice to work with, and its diagrams draw.

Principles:

- **Read every dialect, write only what is there.** Compatibility never rewrites the file; constructs are recognized where they stand.
- **Nothing breaks.** A construct Focal does not render shows its source quietly, never garbled output; an unknown diagram language shows its source with a note.
- **Same dialect everywhere.** The editor, Go to Heading, Export as HTML and Copy as HTML recognize the same constructs.
- **Native first** still holds: renderers run in Focal (Rust, or JavaScript in the embedded QuickJS); tools the user installed (`dot`, `plantuml`, `d2`) are used when present; no web views, and no network services unless the user opts in.

### Dialects (Milestone 9)

| Construct | Seen in | Focal |
| --- | --- | --- |
| CommonMark core, tables, task lists, strikethrough, autolinks, footnotes, alerts `> [!NOTE]` | CommonMark, GFM | Built (M1 to M4). |
| Raw HTML | everywhere | See below. |
| Math `$…$`, `$$…$$` | GitHub, GitLab, Obsidian, Typora, Pandoc | Built (M4). |
| Math `\(…\)`, `\[…\]` | MathJax, Pandoc, LLM answers | Built (M9). |
| Math fences `` ```math `` and `` $`…`$ `` | GitHub, GitLab | Built (M9). |
| Callouts `> [!type] Title`, any type, `+`/`-` folding | Obsidian, GitHub (five types) | Built (M9): every type, mapped to the five colors, written titles; folding (M11). |
| Containers `::: type Title` … `:::` | Docusaurus, VitePress, Pandoc fenced divs, markdown-it | Built (M9): titled callouts. |
| Admonitions `!!! type "Title"` with indented body, `???` collapsible | MkDocs, Python-Markdown | Built (M9): titled callouts. |
| `>>>` multi-line quotes | GitLab | Built (M9). |
| Front matter YAML `---`, TOML `+++`, JSON `;;;` / `{…}` | Jekyll, Hugo, Zola, GitLab | Built (M4, M9). |
| Definition lists | PHP Markdown Extra, Pandoc, GitLab, kramdown | Built (M9). |
| Superscript `^x^` | Pandoc, MultiMarkdown, markdown-it | Built (M9), also inside words. Subscript `~x~` stays strikethrough, as on GitHub. |
| Heading attributes `{#id .class}` | Pandoc, kramdown, Markdown Extra | Built (M9): hidden away from the caret. |
| Abbreviations `*[HTML]: …` | Markdown Extra, MultiMarkdown, kramdown | Built (M9): quiet lines; export writes `<abbr>`. |
| Wiki links `[[note#heading\|alias]]`, embeds `![[image.png]]`, `![[note]]` | Obsidian, Foam, Logseq | Built (M4, M9): image embeds as images (found anywhere in the folder), note embeds as links. |
| Comments `%%…%%`, `<!-- … -->` | Obsidian, HTML | Built (M9): quiet; export leaves them out. |
| Tags `#tag/sub` | Obsidian, Bear | Built (M9). |
| Emoji shortcodes `:tada:` | GitHub, GitLab, Slack | Built (M9). |
| Table of contents `[TOC]`, `[[_TOC_]]`, `[[toc]]`, `{:toc}` | GitLab, Markdown Extra, kramdown, VitePress | Built (M9): the outline, each entry a link. |
| Pandoc grid and simple tables, line blocks | Pandoc | Shown as monospaced source (later: grids). |
| Template syntax `{{< … >}}`, `{% … %}`, MDX `import`/JSX | Hugo, Jekyll, MDX | Shown quietly as source. |
| Encodings: UTF-8 with BOM, UTF-16, legacy 8-bit (Windows-1252) | older files, Windows | Built (M9). |
| Line endings LF, CRLF, CR | all | Built (M1, M9). |

Raw HTML: CommonMark and GFM pass HTML through untouched, as blocks and inline; GFM's "tagfilter" extension escapes nine tags that could break a page (`title`, `textarea`, `style`, `xmp`, `iframe`, `noembed`, `noframes`, `script`, `plaintext`); GitHub and GitLab then sanitize the result with an allow-list (GitLab adds `span`, `abbr`, `details` and `summary`); Obsidian and Typora render HTML in their previews; Pandoc passes it through to HTML output. Focal (built in M9): the editor never runs HTML. It shows comments quietly, draws an `<img>` that stands alone as an image, styles inline formatting tags (`b`, `strong`, `i`, `em`, `u`, `ins`, `s`, `del`, `mark`, `kbd`, `sub`, `sup`, `code`, `br`, `a`) with the tags hidden away from the caret, hides layout wrappers (`<p align>`, `<div align>`, `<center>`) and shows `<details>`/`<summary>` as a titled block; other HTML stays quiet source. Export passes HTML through like CommonMark, with GFM's tagfilter applied.

### Diagrams (Milestone 10)

| Language | Seen in | Focal |
| --- | --- | --- |
| `mermaid` | GitHub, GitLab, Obsidian, Docusaurus, Typora | Built (M5, M10): all 22 diagram types, checked by a test. |
| `dot`, `graphviz` | GitLab (Kroki), Hugo, Pandoc filters | Built (M10): `dot` when installed, otherwise natively (`layout-rs`). |
| `plantuml`, `puml` | GitLab, MkDocs, Confluence exports | Built (M10): `plantuml` when installed, otherwise a code block. |
| `d2` | Terrastruct, docs sites | Built (M10): `d2` when installed, otherwise a code block. |
| `svgbob`, `bob` | Kroki, mdBook | Built (M10), natively. |
| `pikchr` | Fossil, Kroki | Built (M10), natively. |
| `wavedrom` | Kroki, hardware docs | Built (M10), natively (`wavedrom`). |
| `geojson`, `topojson` | GitHub | Built (M10): an outline map, no tiles or network. |
| `vega`, `vega-lite` | Kroki, Jupyter | Built (M11): Vega itself in QuickJS, as math (`scripts/vega`); text is measured in Helvetica Neue's own advances (M12), since QuickJS has no canvas. |
| `stl` | GitHub | Built (M11): a shaded still from an angle above (ASCII STL). |

## 8. Milestones (Proposed)

| Milestone | Outcome |
| --- | --- |
| **M0 — Engine spikes** | Done: TextKit 1 vs 2, the GPUI spike and the spell-checking and VoiceOver gate. GPUI chosen. |
| **M1 — Core editor** | Done: line prefixes (quote bars, bullets, numbers, clickable checkboxes) drawn beside the text with hanging indents; the caret never stops inside a prefix; list editing (Return, Tab, Shift-Tab, Backspace at a marker); live reload through FSEvents; cached accessibility tree; code highlighting with tree-sitter; GPUI integration tests. |
| **M2 — Tables** | Done: every table is a grid with cells edited in place; Tab, Return and the arrow keys move between cells; hover buttons, a context menu and drag handles add, delete, move and align rows and columns; edited tables are rewritten aligned, and a cell edit is one undo step; wide tables scroll sideways. |
| **M3 — Chrome** | Done: bottom bar with word count, focus mode by sentence or paragraph with typewriter scrolling, folder mode with a live sidebar and a ⌘P quick switcher, a settings window, a menu bar, and formatting shortcuts. |
| **M4 — Extras** | Done: islands (blocks drawn in place of their source while the caret is elsewhere) for front matter, images and display math; footnote previews and jumps; following wiki links, file links and anchors with ⌘[ to go back; alert icons. |
| **M5 — Mermaid** | Done: native rendering with `merman`, cache by source and appearance, island with live preview. |
| **M6 — Distribution** | Done: `Focal.app` bundle with an icon and Markdown document types, one running instance that `focal` forwards to (with `--wait`), Finder's Open With, "Install Command Line Tool…", Sparkle updates with a setting and a download prompt, About panel, Developer ID signing, and a release script with notarization and a Homebrew cask template. The first public release is still to be published. |
| **M7 — Everyday editing** | Done: find and replace with a find bar, New, Open… and Open Recent, and asking before untitled text is lost. |
| **M8 — Outline and sharing** | Done: Go to Heading, Export as HTML…, Copy as HTML. |
| **M9 — Dialects** | Done: math in every notation, callouts in every style, TOML and JSON front matter, Pandoc and Markdown Extra extensions, Obsidian syntax, emoji shortcodes, tables of contents, README-style HTML, and any encoding; see section 7a. |
| **M10 — Diagrams** | Done: Graphviz, Svgbob, Pikchr, WaveDrom, GeoJSON and TopoJSON natively, D2 and PlantUML through their tools, all Mermaid types; every drawing in Focal's palette; see section 7a. |
| **M11 — Completeness** | Done: folding callouts and `<details>`, Vega and Vega-Lite, STL, PlantUML's dark mode, WaveDrom in the dark, centered HTML, and export that renders callouts in one pass (footnotes and references reach into them; folding callouts export as `<details>`). |
| **M12 — Writing and output** | Done: grammar checking and automatic correction, Print… and Export as PDF…, HTML tables drawn as grids, folds remembered per file, Vega text measured, focus mode dimming syntax and chrome, the diagram tools in Settings, and CI on macOS with Dependabot and a Claude fix workflow. |

Each milestone gets its own implementation plan before work starts.

## 9. Testing (Proposed)

- **Parser:** Rust unit tests for span output, plus the CommonMark and GFM spec examples, checked for correct span ranges.
- **Round-trip:** property tests where opening and saving any unedited document produces identical bytes, including CRLF, missing trailing newline, tabs and non-ASCII text.
- **Tables:** golden tests for the serializer (alignment, escaping, wide characters) and grid-edit-to-source tests.
- **Rendering:** snapshot tests of rendered documents in light and dark mode.
- **Interaction:** GPUI integration tests through GPUI Kit's test harness (headless windows, simulated keys and mouse) for syntax reveal, caret movement across hidden markers and islands, list editing and undo.
- **Performance:** `mise run stress` on the 5,000-line document for parse and restyle time, plus in-app traces (`FOCAL_TRACE=1`); keystroke budget 8 ms.

## 10. Open questions

- ~~Math typesetting without a web view~~ Decided 2026-10-02: MathJax in QuickJS (accepted).
- ~~Apple's Writing Tools~~ Decided 2026-10-02: the panel version, built in M12 (section 3, Documents). Inline rewriting (`NSWritingToolsCoordinator`) stays open.
- ~~Updates: Sparkle or a Rust-native updater~~ Decided 2026-10-02: Sparkle. Rust-native updaters (`cargo-packager-updater`, Velopack) lack Sparkle's prompt, delta updates and Homebrew livecheck.
- ~~Grammar checking and autocorrect~~ Built in M12 through `NSSpellChecker` (section 3, Documents).
- ~~How to follow GPUI upgrades~~ Decided 2026-10-01: Dependabot proposes GPUI Kit updates in their own pull request and Claude fixes a failing one ([decision](../decisions/2026-10-01_195014376_check-on-macos-runners-and-let-claude-fix-dependency-updates.md), accepted 2026-10-02).
- ~~Encodings other than UTF-8: refuse, or detect and preserve?~~ Decided 2026-10-01 with the compatibility goal (section 7a): detect and preserve (M9).
- ~~Architecture: Apple Silicon only, or universal binaries (Intel)?~~ Decided 2026-10-01: Apple Silicon only ([decision](../decisions/2026-10-01_212449150_build-for-apple-silicon-only.md)).
- ~~Whether focus mode should also dim syntax markers and chrome colors~~ Decided 2026-10-01: yes, built in M12.
- ~~Printing and PDF export~~ Decided 2026-10-01: both, built in M12 (section 3, Documents).
- ~~Name collision check for "Focal"~~ Done 2026-10-01: no same-category product named Focal (an App Store planner "Focal" and the Markdown editor "Focused" are the closest; trademarks not screened). Niklas keeps the name.
