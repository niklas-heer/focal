# Milestone 13 — Review and Writing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Five features for reviewing and writing, built in this order:

1. smarter paste
2. backlinks
3. a "what changed" view
4. style tools
5. review notes for agents

Each must be obvious to find and must leave existing behaviour alone.

**Architecture:** As in earlier milestones:

- **`focal-core`** holds each feature's logic as pure functions with unit tests: `paste.rs`, `links.rs` (extended), `diff.rs`, `style.rs` and `review.rs`.
- **The editor** draws the results through the mechanisms that already exist:
  - `mark_runs` with new `InlineStyle` bits, for text colours and underlines;
  - the row canvas that paints find matches, for tints and wedges;
  - absolutely placed row children, for margin marks.
- **The workspace** owns the cross-file parts: the backlink index (built beside the tag index), the Notes tab and the changes pill.
- **macOS services** come through `objc2`:
  - `NSPasteboard` for HTML;
  - NaturalLanguage's `NLTagger` for parts of speech.

**Tech Stack:**

- Rust and GPUI through `gpui-kit`;
- `similar` (word diff);
- `htmd` (HTML to Markdown);
- `objc2-natural-language` (`NLTagger`);
- `git` through `std::process::Command`.

**Spec:** [`docs/superpowers/specs/2026-10-04-m13-review-and-writing-design.md`](../specs/2026-10-04-m13-review-and-writing-design.md). Read it first; section numbers below refer to it.

## Global Constraints

- **The file is the document.** Only paste, drop and Style ▸ Remove edit the document, each as one undo step. Everything else is drawn only. Review notes live in `NAME.review.md` beside `NAME.md`.
- **New shortcuts** (checked free on 2026-10-04):
  - ⌥⇧⌘V Paste as Plain Text
  - ⌥⌘↓ Next Change, ⌥⌘↑ Previous Change
  - ⌥⌘N Add Note
  - Vim `]c`/`[c`, Helix `]g`/`[g`
  - space and `\` menu keys `c` (changes), `l` (linked from), `n` (add note)
  - `:changes`, `:links`, `:note`
- **Esc keeps its existing meanings first.** Dismiss Changes takes it only with Standard keys when it would do nothing else.
- **Drawing order**, bottom to top:
  1. change tint
  2. note underline
  3. style underline
  4. find matches
  5. selection
  6. spelling and grammar
- **Margins:** change bars in the left margin, note dots in the right.
- **Pills:** the mode pill stays bottom left and the changes pill goes bottom right. Neither covers the centred bar.
- **Performance:** diffing, tagging, link scanning and sidecar parsing run off the main thread. The keystroke budget is 8 ms (`mise run stress`).
- **Discoverability:** every feature has a menu item and a key-reference line (spec section 6), and tooltips name every new button with its shortcut.
- **Unsafe code:** `unsafe` only in named macOS bridges. If `NLTagger` needs it, `style.rs` joins the decision "Allow unsafe code only in named macOS bridges", which is updated.
- **Checks:** `mise run check` after every task. Logic goes in `focal-core` with unit tests; UI behaviour gets headless UI tests in `crates/focal/src/ui_tests.rs`.
- **Branch and commits:** branch `m13-review`; one conventional commit per task with the session's `Co-Authored-By` line; `mise run clean` at the end.
- **Screenshots:** after each UI task, screenshots in light and dark, following the memory "Screenshot Focal safely" (temp `HOME`, per-window capture only).

## Review Focus

1. **HTML from a code editor or terminal** (VS Code and Terminal put `<span style>` HTML on the pasteboard) pastes the plain text, not escaped or styled Markdown. *Task 1 test `code_editor_html_pastes_plain`.*
2. **An agent writing a file in several steps** (two reloads before you look) keeps the first baseline, so all its changes show together. *Task 3 test `two_reloads_keep_the_first_baseline`.*
3. **Non-ASCII and CRLF text** in diffs, style matches and note anchors stays on character boundaries and never panics. *Tests in Tasks 3, 4 and 5 (`umlauts_and_crlf`).*
4. **A noted passage that occurs more than once** anchors to the occurrence nearest its recorded line. *Task 5 test `nearest_occurrence_wins`.*
5. **Links to the open file written differently** (`[[Name]]`, `[[name.md]]`, `[x](./Name.md)`, `[x](Name%20Two.md#part)`, and the file linking to itself) all count once each, and the file never lists itself. *Task 2 test `link_spellings_resolve_and_self_is_skipped`.*

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/focal-core/src/paste.rs` (new) | Deciding what a paste inserts: link over selection, HTML conversion or plain text, asset names and relative paths. |
| `crates/focal-core/src/links.rs` | Adds `outgoing_links` and `Backlink`. |
| `crates/focal-core/src/diff.rs` (new) | Word-level `Changes` between a baseline and the current text. |
| `crates/focal-core/src/style.rs` (new) | English and German filler, cliché and redundancy lists; `issues(text, lang)`. |
| `crates/focal-core/src/review.rs` (new) | Sidecar format: parse, write, anchor, prompt. |
| `crates/focal-core/src/analysis.rs` | New `InlineStyle` bits: `NOTED`, `STYLE`, `ADJECTIVE`, `NOUN`, `ADVERB`, `VERB`, `CONJUNCTION`. |
| `crates/focal-core/src/keys.rs` | New `AppCommand`s, menu keys, `:` commands and reference lines. |
| `crates/focal-core/src/vim.rs`, `helix.rs` | `]c`/`[c` and `]g`/`[g`. |
| `crates/focal/src/mac.rs` | `pasteboard_html()`. |
| `crates/focal/src/editor.rs` | Paste and drop; drawing changes, notes and style marks; context-menu items. |
| `crates/focal/src/editor/changes.rs` (new) | The editor's baseline, change computation, navigation and pill. |
| `crates/focal/src/editor/notes.rs` (new) | The editor's notes: sidecar load and save, the add-note card, marks. |
| `crates/focal/src/style.rs` (new) | `NLTagger` wrapper (and bridge, if needed). |
| `crates/focal/src/folder.rs` | Link index beside tags; `*.review.md` excluded from `scan`. |
| `crates/focal/src/workspace.rs`, `workspace/panel.rs` | Backlinks section, Notes tab, Writing toggles. |
| `crates/focal/src/settings.rs` | `images_folder`, `pos_highlight`, `style_check`. |
| `crates/focal/src/theme.rs` | `inserted`, `deleted`, `note`, `style_issue` and the five part-of-speech colours. |
| `crates/focal/src/menus.rs` | Menu items. |
| `docs/design.md`, `README.md`, `decisions/` | Documentation. |

---

### Task 1: Smarter paste (spec section 1)

**Files:**
- Create: `crates/focal-core/src/paste.rs`; register `pub mod paste;` in `crates/focal-core/src/lib.rs`
- Modify:
  - `crates/focal-core/Cargo.toml` (add `htmd = "0.5"`)
  - `crates/focal/src/mac.rs` (`pasteboard_html`)
  - `crates/focal/src/editor.rs` (`paste`, new `paste_plain`, drop handling, drop overlay)
  - `crates/focal/src/settings.rs` (`images_folder`)
  - `crates/focal/src/menus.rs`
- Test: `paste.rs` unit tests; `ui_tests.rs`

**Interfaces:**
- Produces:
  - `paste::is_url(text: &str) -> bool`
  - `paste::link_over(label: &str, url: &str) -> String`
  - `paste::html_to_markdown(html: &str, plain: &str) -> Option<String>` (`None` = paste `plain`)
  - `paste::asset_name(original: Option<&str>, extension: &str, now: (i32,u8,u8,u8,u8,u8), taken: impl Fn(&str) -> bool) -> String`
  - `paste::relative_link(document_dir: &Path, file: &Path) -> String`
  - `paste::image_markdown(path: &str) -> String`
  - `paste::file_link(name: &str, path: &str) -> String`
  - `mac::pasteboard_html() -> Option<String>`
  - `Settings.images_folder: String` (default `"assets"`)
  - action `PasteAsPlainText`

- [ ] **Step 1: Write the failing core tests** in `paste.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn urls_are_single_links() {
        assert!(is_url("https://example.com/a?b=c"));
        assert!(is_url("mailto:me@example.com"));
        assert!(!is_url("https://a.com and more"));
        assert!(!is_url("example.com"));
        assert!(!is_url("https://a.com\nhttps://b.com"));
    }

    #[test]
    fn a_url_over_a_selection_makes_a_link() {
        assert_eq!(link_over("the docs", "https://a.com"), "[the docs](https://a.com)");
        assert_eq!(link_over("a [b] c", "https://a.com"), "[a \\[b\\] c](https://a.com)");
    }

    #[test]
    fn rich_html_becomes_markdown() {
        let html = "<p>Hello <b>bold</b> and <a href=\"https://a.com\">link</a></p>";
        assert_eq!(
            html_to_markdown(html, "Hello bold and link").as_deref(),
            Some("Hello **bold** and [link](https://a.com)")
        );
    }

    #[test]
    fn code_editor_html_pastes_plain() {
        let html = "<div style=\"color:#000\"><span style=\"color:#00f\">fn</span> main() {}</div>";
        assert_eq!(html_to_markdown(html, "fn main() {}"), None);
    }

    #[test]
    fn asset_names_never_overwrite() {
        let taken = |name: &str| name == "pasted-2026-10-04-101500.png";
        assert_eq!(
            asset_name(None, "png", (2026, 10, 4, 10, 15, 0), taken),
            "pasted-2026-10-04-101500-2.png"
        );
        assert_eq!(
            asset_name(Some("My Photo.JPG"), "jpg", (2026, 10, 4, 0, 0, 0), |_| false),
            "My Photo.JPG"
        );
    }

    #[test]
    fn relative_links_encode_spaces() {
        assert_eq!(
            relative_link(Path::new("/n"), Path::new("/n/assets/My Photo.png")),
            "assets/My%20Photo.png"
        );
        assert_eq!(image_markdown("assets/a.png"), "![](assets/a.png)");
        assert_eq!(file_link("Notes Two", "Notes%20Two.md"), "[Notes Two](Notes%20Two.md)");
    }
}
```

- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p focal-core paste`. Expected: compile errors (the functions are missing).

- [ ] **Step 3: Implement `paste.rs`:**

```rust
//! What a paste or drop inserts: a link when a URL lands on selected text,
//! Markdown for rich text, and paths for images and files saved or dropped
//! beside the document.

use std::path::Path;

/// One URL and nothing else.
pub fn is_url(text: &str) -> bool {
    let text = text.trim();
    !text.contains(char::is_whitespace)
        && (text.starts_with("https://") || text.starts_with("http://") || text.starts_with("mailto:"))
        && text.len() > "http://".len()
}

/// `[label](url)`, with brackets in the label escaped.
pub fn link_over(label: &str, url: &str) -> String {
    let label = label.replace('[', "\\[").replace(']', "\\]");
    format!("[{label}]({})", url.trim())
}

/// Tags that carry Markdown meaning; HTML with none of them is styling
/// only (code editors, terminals) and pastes as its plain text.
const MEANINGFUL: [&str; 16] = [
    "<b>", "<b ", "<strong", "<i>", "<i ", "<em", "<a ", "<h1", "<h2", "<h3", "<ul", "<ol",
    "<blockquote", "<table", "<img", "<code",
];

/// Markdown for pasted HTML, or `None` when the plain text should be pasted.
pub fn html_to_markdown(html: &str, plain: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    if !MEANINGFUL.iter().any(|tag| lower.contains(tag)) {
        return None;
    }
    let markdown = htmd::convert(html).ok()?;
    let markdown = markdown.trim().to_owned();
    (!markdown.is_empty() && markdown != plain.trim()).then_some(markdown)
}

/// A file name for an asset that no file in the folder has yet: the
/// dropped file's own name, or `pasted-<date>-<time>`, with `-2`, `-3` …
pub fn asset_name(
    original: Option<&str>,
    extension: &str,
    (y, mo, d, h, mi, s): (i32, u8, u8, u8, u8, u8),
    taken: impl Fn(&str) -> bool,
) -> String {
    let (stem, ext) = match original {
        Some(name) => match name.rsplit_once('.') {
            Some((stem, ext)) => (stem.to_owned(), ext.to_owned()),
            None => (name.to_owned(), extension.to_owned()),
        },
        None => (format!("pasted-{y:04}-{mo:02}-{d:02}-{h:02}{mi:02}{s:02}"), extension.to_owned()),
    };
    let mut name = format!("{stem}.{ext}");
    let mut n = 2;
    while taken(&name) {
        name = format!("{stem}-{n}.{ext}");
        n += 1;
    }
    name
}

/// `file` relative to `document_dir`, with `/` and `%20` for spaces (and
/// `%25`, `%28`, `%29` for `%`, `(`, `)`), as Markdown renderers expect.
pub fn relative_link(document_dir: &Path, file: &Path) -> String {
    let relative = file.strip_prefix(document_dir).unwrap_or(file);
    let parts: Vec<String> = relative
        .components()
        .map(|part| {
            part.as_os_str()
                .to_string_lossy()
                .replace('%', "%25")
                .replace(' ', "%20")
                .replace('(', "%28")
                .replace(')', "%29")
        })
        .collect();
    parts.join("/")
}

pub fn image_markdown(path: &str) -> String {
    format!("![]({path})")
}

pub fn file_link(name: &str, path: &str) -> String {
    link_over(name, path)
}
```

If `htmd`'s output differs on the bold/link test (e.g. `__bold__`), configure `htmd::HtmlToMarkdown::builder()` with `strong_delimiter("**")` and use it instead of `htmd::convert`.

- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test -p focal-core paste`. Expected: 6 passed.

- [ ] **Step 5: Read HTML from the pasteboard.** In `crates/focal/src/mac.rs`, beside the existing `NSPasteboard` code:

```rust
/// The general pasteboard's HTML, when it has any (browsers, Docs, Mail).
pub fn pasteboard_html() -> Option<String> {
    let pasteboard = NSPasteboard::generalPasteboard();
    let html = pasteboard.stringForType(unsafe_free_html_type())?;
    Some(html.to_string())
}
```

`unsafe_free_html_type()` stands for whatever this needs: `NSPasteboardTypeHTML` is an `extern static` in `objc2-app-kit`, so reading it may need `unsafe`. If it does, use `NSString::from_str("public.html")` instead, which is the same UTI and needs no `unsafe`. Add the `NSPasteboard` features `NSPasteboardTypeHTML` or `NSString` as the compiler asks.

- [ ] **Step 6: Paste in the editor.** Replace `Editor::paste` in `editor.rs`:

```rust
fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
    let Some(item) = cx.read_from_clipboard() else { return };
    // An image on its own (a screenshot): save it beside the document.
    if item.text().is_none()
        && let Some(gpui_kit::ClipboardEntry::Image(image)) = item.entries().first()
    {
        self.insert_image_bytes(image.bytes(), image.format.extension(), window, cx);
        return;
    }
    let Some(plain) = item.text() else { return };
    let selected = self.selection_text();
    let in_code = self.caret_in_code_or_cell();
    let text = if !selected.is_empty() && !selected.contains('\n') && focal_core::paste::is_url(&plain) {
        focal_core::paste::link_over(&selected, &plain)
    } else if in_code {
        plain
    } else {
        crate::mac::pasteboard_html()
            .and_then(|html| focal_core::paste::html_to_markdown(&html, &plain))
            .unwrap_or(plain)
    };
    self.insert(&text, EditKind::Other, cx);
}

fn paste_plain(&mut self, _: &PasteAsPlainText, _: &mut Window, cx: &mut Context<Self>) {
    if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
        self.insert(&text, EditKind::Other, cx);
    }
}
```

Add the helpers next to `selected_line_text`:

- `selection_text(&self) -> String`: the selected source text.
- `caret_in_code_or_cell(&self) -> bool`: `self.grid.is_some()`, or the caret line's `LineKind` is `Code`, `CodeFence`, `Math`, `FrontMatter`, `FrontMatterFence` or `Html`, or the caret is inside an inline `CODE` or `MATH` run of its line view.

Add the action:

- Add `PasteAsPlainText` to the editor's `actions!`.
- Bind `KeyBinding::new("cmd-alt-shift-v", PasteAsPlainText, context)`.
- Register `.on_action(cx.listener(Self::paste_plain))`.
- In `menus.rs`, add `MenuItem::action("Paste as Plain Text", PasteAsPlainText)` right after Paste.

- [ ] **Step 7: Save images and link files.** In `editor.rs`, add:

```rust
/// Writes `bytes` into the images folder beside the document and inserts
/// `![](…)`; an untitled document asks to be saved first.
fn insert_image_bytes(&mut self, bytes: &[u8], extension: &str, window: &mut Window, cx: &mut Context<Self>) {
    let Some(dir) = self.asset_dir_or_ask_to_save(window, cx) else { return };
    let name = focal_core::paste::asset_name(None, extension, crate::drafts::now_parts(), |n| dir.join(n).exists());
    if let Err(error) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(dir.join(&name), bytes)) {
        self.show_error(format!("Could not save the image: {error}"), cx);
        return;
    }
    let link = focal_core::paste::relative_link(self.document_dir().unwrap(), &dir.join(name));
    self.insert(&focal_core::paste::image_markdown(&link), EditKind::Other, cx);
}
```

- `asset_dir_or_ask_to_save`: returns `document_dir().join(&settings.images_folder)`. For an untitled document it calls the existing Save As flow, then returns `None`; the user pastes again after saving.
- `now_parts`: use the existing local time helper in `drafts.rs` if one exists; otherwise add `now_parts()` there with `time::OffsetDateTime::now_local()`, falling back to UTC. Check `Cargo.toml` for the time crate already in use and reuse it; do not add a second one.

For drops:

- Register `.on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| this.drop_paths(paths.paths(), window, cx)))` on the editor's root element.
- `drop_paths` moves the caret to the drop point (`hit_test(position)`, via the last mouse position kept by `on_drag_move::<ExternalPaths>`).
- Then, for each path: images (`png jpg jpeg gif webp svg heic tiff`) are copied into the assets folder with `asset_name(Some(file_name), …)`; other files get `file_link(stem, relative_link(…))`; folders are skipped.
- Everything is joined with `\n` and inserted as one edit.

- [ ] **Step 8: Show what a drop will do.** While `ExternalPaths` are dragged over the editor, show a centred overlay card in the editor's `render`, id `drop-overlay`, with text "Add 2 images to assets/" or "Link 1 file" (counted from the dragged paths). Show it while `on_drag_move::<ExternalPaths>` reports a position inside the editor; hide it on drop or when the drag leaves.

- [ ] **Step 9: Add the setting.**
- Add `images_folder: String` (default `"assets"`) to `Settings` (serde default so old files load).
- Show it in Settings ▸ Writing as a text field labelled "Images folder" with the hint "Pasted and dropped images are saved here, beside the document".

- [ ] **Step 10: UI tests** in `ui_tests.rs`:

```rust
#[gpui_kit::test]
fn pasting_a_url_over_a_selection_makes_a_link(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "see the docs");
    editor.update(cx, |e, cx| e.select(4..12, cx));
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("https://a.com".into()));
    act(cx, window, |window, cx| window.press("cmd-v", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.text(), "see [the docs](https://a.com)"));
    act(cx, window, |window, cx| window.press("cmd-z", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.text(), "see the docs", "one undo step"));
}

#[gpui_kit::test]
fn a_url_without_a_selection_pastes_as_written(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "");
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("https://a.com".into()));
    act(cx, window, |window, cx| window.press("cmd-v", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.text(), "https://a.com"));
}

#[gpui_kit::test]
fn paste_in_a_code_block_stays_plain(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "```\n\n```");
    editor.update(cx, |e, cx| e.move_to(4, cx));
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("https://a.com".into()));
    act(cx, window, |window, cx| window.press("cmd-v", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.text(), "```\nhttps://a.com\n```"));
}
```

HTML conversion depends on the real pasteboard, so its logic is covered by the core tests.

- [ ] **Step 11: Check.** Run `mise run check`. Expected: pass. Then take screenshots of the drop overlay (drag a file with a CGEvent helper, or skip it if dragging cannot be simulated; the UI test covers its visibility through a direct state call).

- [ ] **Step 12: Commit:** `feat(paste): links over selections, rich text as Markdown, images into assets/`

---

### Task 2: Backlinks (spec section 2)

**Files:**
- Modify:
  - `crates/focal-core/src/links.rs`
  - `crates/focal/src/folder.rs`
  - `crates/focal/src/workspace.rs`
  - `crates/focal/src/workspace/panel.rs`
  - `crates/focal-core/src/keys.rs`
  - `crates/focal/src/menus.rs`
  - `crates/focal/src/editor/modal.rs` (`app_action`)
- Test: `links.rs`, `folder.rs` tests, `ui_tests.rs`

**Interfaces:**
- Produces:
  - `links::OutgoingLink { range: Range<usize>, target: LinkTarget }`, `enum LinkTarget { Wiki(String), Path(String) }`
  - `links::outgoing_links(text: &str) -> Vec<OutgoingLink>`
  - `folder::Backlink { file: PathBuf, offset: usize, line: String, label: Range<usize> }`
  - `folder::backlinks(root: &Path, files: &[PathBuf]) -> HashMap<PathBuf, Vec<Backlink>>`, keyed by the target's path relative to `root`
  - `Workspace::backlinks_for(&self, path: &Path) -> &[Backlink]`
  - `AppCommand::Links`
  - action `ShowLinkedFrom`

- [ ] **Step 1: Write the failing core tests** in `links.rs` tests:

```rust
#[test]
fn outgoing_links_find_wiki_and_relative_links_outside_code() {
    let text = "See [[Design]] and [x](./notes/Two.md#part).\n`[[not]]`\n```\n[[no]]\n```\n[web](https://a.com)";
    let found: Vec<_> = outgoing_links(text).into_iter().map(|l| l.target).collect();
    assert_eq!(found, [LinkTarget::Wiki("Design".into()), LinkTarget::Path("./notes/Two.md".into())]);
}
```

And in `folder.rs` tests (use the existing `temp_folder` helper):

```rust
#[test]
fn link_spellings_resolve_and_self_is_skipped() {
    let root = temp_folder("backlinks");
    std::fs::write(root.join("Name Two.md"), "[[Name Two]] links itself").unwrap();
    std::fs::write(root.join("a.md"), "[[Name Two]] and [[name two.md]]").unwrap();
    std::fs::write(root.join("b.md"), "[x](./Name%20Two.md#part)").unwrap();
    let files = scan(&root, Depth::Tree, SCAN_LIMIT);
    let index = backlinks(&root, &files);
    let from: Vec<_> = index[Path::new("Name Two.md")].iter().map(|b| b.file.clone()).collect();
    assert_eq!(from, [PathBuf::from("a.md"), PathBuf::from("a.md"), PathBuf::from("b.md")]);
}
```

- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p focal-core outgoing_links` and `cargo test -p focal link_spellings`. Expected: compile errors.

- [ ] **Step 3: Implement `outgoing_links`.** Use `crate::analysis::analyze(text)` so code, math and front matter are excluded exactly as the editor excludes them:

- Wiki links are the analysis's wiki-link ranges; take the inner text through `wiki_target`.
- Markdown links are the analysis's `LINK` destination ranges. Keep a destination when it is not a URL (no `://`, not `mailto:`), not anchor-only (does not start with `#`), and ends in `.md`/`.markdown` or has no extension (before any `#`). Strip the `#` part.

If the analysis does not yet expose link destinations, add `pub fn link_destinations(&self) -> impl Iterator<Item = (Range<usize>, &str)>` on `Analysis`. It is built from the `pulldown_cmark::Tag::Link { dest_url, .. }` events the analysis already walks.

- [ ] **Step 4: Implement `folder::backlinks`.**
- Read each file under the 2 MB limit, skipping `*.review.md`, and run `outgoing_links` on it.
- Resolve each link to a target:
  - `Wiki(name)` through `links::resolve_wiki(&name, file, files)`;
  - `Path(dest)` through `links::relative_target(&links::percent_decode(&dest), root.join(file).parent())`, made relative to `root`.
- Skip links whose target equals `file`.
- Record a `Backlink`:
  - `offset` is `range.start`;
  - `line` is the source line holding it, trimmed and shortened to 120 characters at a character boundary;
  - `label` is the link's range within `line`.

- [ ] **Step 5: Run the tests and see them pass.** Run the commands from Step 2. Expected: PASS.

- [ ] **Step 6: Index in the background.**
- Add `backlinks: HashMap<PathBuf, Vec<Backlink>>` to `Folder` in `workspace.rs`.
- In `index_tags`, compute `(folder::tags(&root, &files), folder::backlinks(&root, &files))` in the same background task and store both. Rename the method to `index_folder` and update its callers.
- Add `backlinks_for(&self, path)`. It strips `folder.root` from `path` and looks the result up.

- [ ] **Step 7: Add the panel section.** In `panel.rs` `render_info`, after the document section and before Writing, add the section "Linked from" with a count, id `panel-backlinks`. When a window has no file list, leave the section out.

Each row (id `("backlink", ix)`, `aria_label` "{file}: {line}") shows the file name in the text colour and the line in the marker colour, with the label range in the link colour.

- Clicking a row calls `this.open_file(root.join(&b.file), window, cx)` and then `editor.move_to(b.offset)`. It pushes history first, so ⌘[ returns.
- With no backlinks, the section shows "No other file links here."

- [ ] **Step 8: Make it discoverable.**
- Add `actions!(focal, [ShowLinkedFrom])` in `panel.rs`. It opens the Info panel (`self.panel = Some(PanelTab::Info)`) and scrolls to `panel-backlinks`, using the panel's scroll handle (or simply opens Info if scrolling to a child isn't available).
- Add View ▸ "Linked From…" in `menus.rs` after Outline.
- In `keys.rs`:
  - add `AppCommand::Links`;
  - add the menu entry `('l', AppCommand::Links, "linked from")` and its `MENU_HINTS` line;
  - add the ex command `(&["links", "backlinks"], Ex::App(AppCommand::Links), "files linking here")`.
- In `modal.rs`, map `AppCommand::Links` in `app_action` to `Box::new(crate::workspace::ShowLinkedFrom)`.

- [ ] **Step 9: UI test:**

```rust
#[gpui_kit::test]
fn the_info_panel_lists_files_linking_here(cx: &mut TestAppContext) {
    let root = notes_folder("backlinks-ui", "# Target");
    std::fs::write(root.join("other.md"), "See [[note]].").unwrap();
    let (window, _) = open_folder(cx, &root);
    act(cx, window, |window, cx| window.dispatch_action(Box::new(crate::workspace::ShowLinkedFrom), cx));
    cx.run_until_parked();
    act(cx, window, |window, _| {
        assert_eq!(window.find(("backlink", 0usize)).label(), Some("other.md: See [[note]]."));
    });
}
```

Adjust `"note"` to the file name `notes_folder` creates (read that helper first).

- [ ] **Step 10: Check.** Run `mise run check`, then take screenshots of the Info panel with backlinks in light and dark.

- [ ] **Step 11: Commit:** `feat(links): backlinks in the info panel`

---

### Task 3: What changed (spec section 3)

**Files:**
- Create: `crates/focal-core/src/diff.rs`, `crates/focal/src/editor/changes.rs`
- Modify:
  - `crates/focal-core/Cargo.toml` (add `similar = "2.7"`)
  - `crates/focal-core/src/lib.rs`
  - `crates/focal/src/editor.rs` (`load_theirs`, row painting, render, actions)
  - `crates/focal/src/theme.rs`
  - `crates/focal-core/src/keys.rs`, `vim.rs`, `helix.rs`
  - `crates/focal/src/menus.rs`, `crates/focal/src/editor/modal.rs`
  - `crates/focal/src/accessibility.rs`
- Test: `diff.rs`, `vim.rs`, `helix.rs`, `ui_tests.rs`

**Interfaces:**
- Produces:
  - `diff::Changes { inserted: Vec<Range<usize>>, deleted: Vec<(usize, String)>, hunks: Vec<Range<usize>> }`, with `Changes::count(&self) -> usize` (the number of hunks)
  - `diff::changes(old: &str, new: &str) -> Changes`
  - `diff::next_hunk(hunks: &[Range<usize>], from: usize, forward: bool) -> Option<usize>`
  - editor fields `baseline: Option<Baseline>`, `changes: Rc<diff::Changes>`, where `enum Baseline { Opened(String), Commit(String) }`
  - actions `NextChange`, `PreviousChange`, `DismissChanges`, `CompareWithLastCommit`
  - `AppCommand::{NextChange, PreviousChange, Changes}`

- [ ] **Step 1: Write the failing core tests** in `diff.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_words_and_deleted_text() {
        let c = changes("The fox jumps.\nSame line.\n", "The quick fox leaps.\nSame line.\n");
        let new = "The quick fox leaps.\nSame line.\n";
        let inserted: Vec<&str> = c.inserted.iter().map(|r| &new[r.clone()]).collect();
        assert_eq!(inserted, ["quick ", "leaps"]);
        assert_eq!(c.deleted, [(14, "jumps".to_owned())]);
        assert_eq!(c.count(), 1, "one changed line is one hunk");
    }

    #[test]
    fn deleted_lines_and_added_lines() {
        let c = changes("a\nb\nc\n", "a\nc\nd\n");
        assert_eq!(c.deleted, [(2, "b\n".to_owned())]);
        assert_eq!(c.inserted, [4..6]);
        assert_eq!(c.count(), 2);
    }

    #[test]
    fn umlauts_and_crlf() {
        let c = changes("Grüße\r\nzwei\r\n", "Grüße, Welt\r\nzwei\r\n");
        let new = "Grüße, Welt\r\nzwei\r\n";
        for r in &c.inserted {
            assert!(new.is_char_boundary(r.start) && new.is_char_boundary(r.end));
        }
        assert_eq!(c.count(), 1);
    }

    #[test]
    fn identical_texts_have_no_changes() {
        assert_eq!(changes("x\n", "x\n").count(), 0);
    }

    #[test]
    fn hunks_are_found_in_order_and_wrap() {
        let hunks = [2..4, 10..12];
        assert_eq!(next_hunk(&hunks, 0, true), Some(0));
        assert_eq!(next_hunk(&hunks, 2, true), Some(1), "from inside one, the next");
        assert_eq!(next_hunk(&hunks, 11, true), Some(0), "wraps");
        assert_eq!(next_hunk(&hunks, 10, false), Some(0));
        assert_eq!(next_hunk(&hunks, 2, false), Some(1), "wraps back");
    }
}
```

- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p focal-core diff`. Expected: compile errors.

- [ ] **Step 3: Implement `diff.rs`:**

```rust
//! What changed between a baseline and the current text, word by word, for
//! drawing an agent's rewrite: inserted ranges in the current text, deleted
//! text at its place, and the changed regions to step through.

use std::ops::Range;

use similar::{ChangeTag, TextDiff};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub inserted: Vec<Range<usize>>,
    pub deleted: Vec<(usize, String)>,
    pub hunks: Vec<Range<usize>>,
}

impl Changes {
    pub fn count(&self) -> usize {
        self.hunks.len()
    }
}

pub fn changes(old: &str, new: &str) -> Changes {
    let mut out = Changes::default();
    let lines = TextDiff::configure().diff_lines(old, new);
    for group in lines.grouped_ops(0) {
        let (mut start, mut end) = (usize::MAX, 0);
        for op in group {
            let old_range = byte_range(&lines, op.old_range(), true);
            let new_range = byte_range(&lines, op.new_range(), false);
            match op.tag() {
                similar::DiffTag::Equal => {}
                similar::DiffTag::Insert => out.inserted.push(new_range.clone()),
                similar::DiffTag::Delete => out.deleted.push((new_range.start, old[old_range].to_owned())),
                similar::DiffTag::Replace => {
                    words(&old[old_range], &new[new_range.clone()], new_range.start, &mut out);
                }
            }
            if op.tag() != similar::DiffTag::Equal {
                start = start.min(new_range.start);
                end = end.max(new_range.end);
            }
        }
        if start != usize::MAX {
            out.hunks.push(start..end.max(start));
        }
    }
    merge(&mut out.inserted);
    out
}

/// Word-level changes inside replaced lines, offset into the new text.
fn words(old: &str, new: &str, base: usize, out: &mut Changes) {
    let diff = TextDiff::configure().diff_unicode_words(old, new);
    let mut at = base;
    for change in diff.iter_all_changes() {
        let value = change.value();
        match change.tag() {
            ChangeTag::Equal => at += value.len(),
            ChangeTag::Insert => {
                if !value.trim().is_empty() || value.len() > 0 && out.inserted.last().is_some_and(|r| r.end == at) {
                    out.inserted.push(at..at + value.len());
                }
                at += value.len();
            }
            ChangeTag::Delete => {
                if !value.trim().is_empty() {
                    out.deleted.push((at, value.to_owned()));
                }
            }
        }
    }
}

/// The byte range a span of line indices covers in the old or new text.
fn byte_range(diff: &TextDiff<'_, '_, '_, str>, lines: Range<usize>, old: bool) -> Range<usize> {
    let slices = if old { diff.old_slices() } else { diff.new_slices() };
    let start: usize = slices[..lines.start].iter().map(|s| s.len()).sum();
    let len: usize = slices[lines.clone()].iter().map(|s| s.len()).sum();
    start..start + len
}

/// Joins touching ranges, so "quick " is one tint, not two.
fn merge(ranges: &mut Vec<Range<usize>>) {
    ranges.sort_by_key(|r| r.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges.drain(..) {
        match merged.last_mut() {
            Some(last) if last.end >= r.start => last.end = last.end.max(r.end),
            _ => merged.push(r),
        }
    }
    *ranges = merged;
}

/// The hunk after `from` going forward (one that starts after it), or the
/// one before going backward, wrapping around.
pub fn next_hunk(hunks: &[Range<usize>], from: usize, forward: bool) -> Option<usize> {
    if hunks.is_empty() {
        return None;
    }
    Some(if forward {
        hunks.iter().position(|h| h.start > from).unwrap_or(0)
    } else {
        hunks.iter().rposition(|h| h.start < from).unwrap_or(hunks.len() - 1)
    })
}
```

The insert condition in `words` keeps whitespace-only inserts only when they extend a previous insert. Simplify it if the test `inserted_words_and_deleted_text` expects otherwise; the test decides. `"quick "` must come out as one range.

- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test -p focal-core diff`. Expected: 5 passed. Fix the offset logic until `umlauts_and_crlf` passes too.

- [ ] **Step 5: The editor's baseline** (`editor/changes.rs`, `mod changes;` in `editor.rs`):

```rust
pub(crate) enum Baseline {
    /// The text before an outside change; your edits since show too.
    Opened(String),
    /// `git show HEAD:<path>`.
    Commit(String),
}
```

- Editor fields: `baseline: Option<Baseline>`, `changes: Rc<focal_core::diff::Changes>`, `changes_task: Option<Task<()>>`, `comparing_commit: bool`.
- `load_theirs`: before `self.buffer.replace_all(…)`:

  ```rust
  if self.baseline.is_none() { self.baseline = Some(Baseline::Opened(self.text().to_owned())); }
  ```

  After it, call `self.recompute_changes(cx)`. This is what keeps the first baseline across several reloads (Review Focus 2).
- `recompute_changes`: clone the baseline text and the current text, run `focal_core::diff::changes` on the background executor, store the result and notify. It is also called (debounced 150 ms, like spelling) from the edit path when `self.baseline.is_some()`.
- `dismiss_changes`: under `Opened`, set `baseline = None` and clear `changes`. Under `Commit`, do nothing; the toggle turns it off.
- `compare_with_last_commit` (toggle):
  - When off, run `git -C <dir> show HEAD:./<file name>` on the background executor. On success set `Baseline::Commit(text)`. On a non-zero exit with "exists on disk, but not in" set `Commit(String::new())`. Otherwise show the error banner "Not in a git repository".
  - When on, set `baseline = None`.
  - Offer the menu item enabled only when `git -C <dir> rev-parse --is-inside-work-tree` succeeded once for this document. Cache it on the editor as `in_git: Option<bool>`, computed in the background on open.
- `next_change` and `previous_change`: `diff::next_hunk(&self.changes.hunks, self.head(), forward)` and `self.move_to(hunks[ix].start)`, which reveals the line.

- [ ] **Step 6: Drawing.**
- In `theme.rs` add:
  - `inserted` (light `hsla(140°, 50%, 45%, 0.18)`, dark `hsla(140°, 45%, 55%, 0.22)`);
  - `deleted` (light `hsla(0°, 70%, 50%, 1)`, dark `hsla(0°, 70%, 62%, 1)`).
- In the row canvas in `editor.rs`, before the `found` loop, paint `self.inserted_on(&line_range)` (clipped like `found_on`) with `paint_selection(…, inserted_color, …)`.
- For each `deleted` offset on the line, paint a 6×6 px red triangle at the offset's x position, at the baseline, pointing up. Get the position the same way the caret position is computed from `layout` and `view.map.to_display`.
- Margin bar: if the row's source range intersects a hunk, add an absolute child `div().absolute().top_0().bottom_0().left(px(-18.)).w(px(3.)).rounded(px(2.)).bg(color)` to the row container (the element holding the prefix and content). Green is `theme.inserted.opacity(1.)` blended; red is used when the row has only deletions. Read how list prefixes are placed first, so the bar sits outside them. Use id `("change-bar", line)`.
- Deletion tooltip: give the wedge an id `("deleted", offset)` and `.tooltip(…)` showing the deleted text, shortened to 200 characters.

- [ ] **Step 7: The changes pill.** In the editor's render, when `self.changes.count() > 0`, add an absolute element at the bottom right (`right(px(16.))`, `bottom(px(12.))`), styled like the mode pill (`theme.surface`, border, 12 px mono). It contains:

- a text button `changes-count` reading "3 changes" or "1 change", plus " since last commit" under `Commit`; clicking it goes to the next change;
- `changes-previous` (↑), `changes-next` (↓) and `changes-dismiss` (✕), the last hidden under `Commit`;
- an `aria_label` and a tooltip with its shortcut on each button.

It sits outside the bar's show zone because the bar is centred. Check this in the screenshots at the narrowest window size (`min_width`).

- [ ] **Step 8: Keys and menus.**
- In the editor's `actions!`, add `NextChange`, `PreviousChange`, `DismissChanges` and `CompareWithLastCommit`.
- Bind `cmd-alt-down` and `cmd-alt-up`.
- Escape: bind `escape` to `DismissChanges` only in a key context `"FocalEditor && changes && !modal"`. Set the context bits in the editor's `key_context`; read how `CONTEXT` is built first. Without a modal and with no find bar open, Esc has no other meaning in the editor.
- View ▸ Changes submenu: Next Change, Previous Change, Dismiss Changes, then a separator, then Compare with Last Commit (`.checked(comparing)`).
- In `keys.rs`:
  - add `AppCommand::NextChange`, `PreviousChange` and `Changes` (Changes = next change, or a "No changes" message when there are none);
  - add the menu entry `('c', AppCommand::Changes, "next change")`;
  - add `(&["changes"], Ex::App(AppCommand::Changes), "go to the next change")`;
  - add reference lines in both references ("`]c [c`" / "`]g [g`", "next, previous change").
- In `vim.rs` `parse`: `Key::Char(']')` followed by `Key::Char('c')` → `Action::App(AppCommand::NextChange)`; `[` then `c` → `PreviousChange`. A lone `]` or `[` is pending, and anything else is invalid.
- In `helix.rs`, do the same with `g`.
- In `app_action`, map them to the editor actions.

Tests in `vim.rs`:

```rust
#[test]
fn brackets_c_step_through_changes() {
    let cx = Context { text: "a", anchor: 0, head: 0, clipboard: None };
    let mut vim = Vim::new();
    assert_eq!(vim.key(Key::Char(']'), &cx), Some(vec![]));
    assert_eq!(vim.key(Key::Char('c'), &cx), Some(vec![Command::App(crate::keys::AppCommand::NextChange)]));
    vim.key(Key::Char('['), &cx);
    assert_eq!(vim.key(Key::Char('c'), &cx), Some(vec![Command::App(crate::keys::AppCommand::PreviousChange)]));
}
```

The same test goes in `helix.rs` with `g`.

- [ ] **Step 9: Accessibility.** In `accessibility.rs`, where text chunks are built, add each inserted range as an `"inserted"` annotation and each deletion as `"deleted: <text>"` at its offset, using the mechanism misspellings use. Read the existing code first. If there is no annotation mechanism, set the changes pill's `aria_label` to "3 changes; next change ⌥⌘↓" and note the gap in the commit message.

- [ ] **Step 10: UI tests.**

```rust
#[gpui_kit::test]
fn an_outside_change_shows_and_dismisses(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "The fox jumps.\n");
    editor.update(cx, |e, cx| e.load_theirs_for_test("The quick fox jumps.\n", cx));
    cx.run_until_parked();
    act(cx, window, |window, _| assert_eq!(window.find("changes-count").label(), Some("1 change")));
    act(cx, window, |window, cx| window.click("changes-dismiss", cx));
    act(cx, window, |window, _| assert!(window.try_find("changes-count").is_none()));
}

#[gpui_kit::test]
fn two_reloads_keep_the_first_baseline(cx: &mut TestAppContext) {
    let (_, editor) = open_editor(cx, "one\n");
    editor.update(cx, |e, cx| e.load_theirs_for_test("one\ntwo\n", cx));
    editor.update(cx, |e, cx| e.load_theirs_for_test("one\ntwo\nthree\n", cx));
    cx.run_until_parked();
    editor.read_with(cx, |e, _| assert_eq!(e.changes_for_test().inserted, [4..14]));
}

#[gpui_kit::test]
fn own_typing_is_not_a_change(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "x");
    act(cx, window, |window, cx| window.input("abc", cx));
    cx.run_until_parked();
    editor.read_with(cx, |e, _| assert_eq!(e.changes_for_test().count(), 0));
}

#[gpui_kit::test]
fn alt_cmd_down_goes_to_the_next_change(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "a\nb\nc\nd\n");
    editor.update(cx, |e, cx| e.load_theirs_for_test("a\nB\nc\nD\n", cx));
    cx.run_until_parked();
    editor.update(cx, |e, cx| e.move_to(0, cx));
    act(cx, window, |window, cx| window.press("cmd-alt-down", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 2..2));
    act(cx, window, |window, cx| window.press("cmd-alt-down", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 6..6));
}
```

Add `#[cfg(test)] pub(crate) fn load_theirs_for_test(&mut self, text: &str, cx)` (it calls `load_theirs` with the current format and no stamp) and `changes_for_test(&self) -> &Changes`.

Also check that Esc with the find bar open still closes the find bar while changes show. Write this as a UI test with ⌘F and Esc that asserts the find bar closes and `changes-count` is still present.

- [ ] **Step 11: Check.** Run `mise run check`. For screenshots, write the sample file, then rewrite it from the shell while Focal is open; capture the pill, the bars and the tints in both palettes.

- [ ] **Step 12: Commit:** `feat(changes): show what changed since opened or since the last commit`

---

### Task 4: Style tools (spec section 4)

**Files:**
- Create: `crates/focal-core/src/style.rs`, `crates/focal/src/style.rs`
- Modify:
  - `crates/focal/Cargo.toml` (`objc2-natural-language = { version = "0.3.2", default-features = false, features = ["std", "NLTagger", "NLTagScheme", "NLTag", "NLLanguage"] }`; add features as the compiler asks)
  - `crates/focal-core/src/analysis.rs` (`InlineStyle` bits)
  - `crates/focal/src/editor.rs`, `settings.rs`, `theme.rs`, `menus.rs`, `workspace/panel.rs`
- Test: `style.rs` unit tests (core), `ui_tests.rs`

**Interfaces:**
- Produces:
  - `focal_core::style::{Issue { range: Range<usize>, kind: IssueKind, suggestion: Option<&'static str> }, IssueKind::{Filler, Cliche, Redundancy}, issues(text: &str, german: bool) -> Vec<Issue>}`
  - `crate::style::parts_of_speech(text: &str) -> Vec<(Range<usize>, Part)>`, with `Part::{Adjective, Noun, Adverb, Verb, Conjunction}`
  - `Settings.pos_highlight: PosSet`, a `u8` bit set with `serde(default)`
  - `Settings.style_check: bool`
  - `InlineStyle::{STYLE, ADJECTIVE, NOUN, ADVERB, VERB, CONJUNCTION}` (bits 19–24)

- [ ] **Step 1: Write the failing core tests** in `focal-core/src/style.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, german: bool) -> Vec<(&str, IssueKind)> {
        issues(text, german).into_iter().map(|i| (&text[i.range], i.kind)).collect()
    }

    #[test]
    fn english_fillers_cliches_and_redundancies() {
        assert_eq!(
            found("It was very late. The end result, at the end of the day, was fine.", false),
            [("very", IssueKind::Filler), ("end result", IssueKind::Redundancy), ("at the end of the day", IssueKind::Cliche)]
        );
    }

    #[test]
    fn whole_words_only_ignoring_case() {
        assert_eq!(found("Every veryfine thing. VERY good.", false), [("VERY", IssueKind::Filler)]);
    }

    #[test]
    fn german_lists_and_umlauts() {
        assert_eq!(
            found("Das ist eigentlich bereits schon fertig, ganz schön übel.", true),
            [("eigentlich", IssueKind::Filler), ("bereits schon", IssueKind::Redundancy)]
        );
    }
}
```

- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p focal-core style`. Expected: compile errors.

- [ ] **Step 3: Implement the core lists and matching.**
- Constants `EN_FILLERS`, `EN_CLICHES`, `EN_REDUNDANCIES`, `DE_FILLERS`, `DE_CLICHES` and `DE_REDUNDANCIES` as `&[&str]`, 25–60 entries each.
  - English fillers: very, really, just, quite, actually, basically, literally, simply, totally, somewhat, rather, pretty (as an adverb is not detectable; leave it out), …
  - German fillers: eigentlich, halt, eben, wirklich, ziemlich, quasi, sozusagen, irgendwie, total, …
  - Redundancies: end result, past history, free gift, added bonus, final outcome / bereits schon, neu renoviert, weißer Schimmel, nochmals wiederholen, …
  - Clichés: at the end of the day, in this day and age, think outside the box / am Ende des Tages, unterm Strich, …
- `issues` lowercases a copy of the text character by character (as `find::find_all` does, so offsets stay on the original's boundaries).
- For every phrase, longest first, it finds whole-word matches: the characters before and after are not alphanumeric. It skips ranges overlapping an earlier match and returns them sorted by start.
- Reuse `crate::find::find_all` for the case-insensitive search, and filter by word boundary.

- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test -p focal-core style`. Expected: 3 passed.

- [ ] **Step 5: Tag parts of speech** in `crates/focal/src/style.rs`:

```rust
//! Parts of speech from macOS's NaturalLanguage framework, for iA
//! Writer-style syntax highlighting.

use std::ops::Range;

use objc2_foundation::{NSRange, NSString};
use objc2_natural_language::{NLTagScheme, NLTagger, NLTokenUnit, NLTaggerOptions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part { Adjective, Noun, Adverb, Verb, Conjunction }

/// The words of `text` (one paragraph) that are one of the parts, as byte
/// ranges.
pub fn parts_of_speech(text: &str) -> Vec<(Range<usize>, Part)> {
    // NLTagger with NLTagSchemeLexicalClass over the paragraph; enumerate
    // word tokens (omit punctuation and whitespace), map NLTagAdjective,
    // NLTagNoun, NLTagAdverb, NLTagVerb, NLTagConjunction to `Part`, and
    // convert each UTF-16 NSRange to byte offsets with `utf16_to_byte`.
    todo_replace_with_binding_calls(text)
}
```

The body above is a sketch. Write the real calls once the generated API is visible in `objc2-natural-language`'s docs (`cargo doc -p objc2-natural-language --open`, or read its source in `~/.cargo/registry`):

- `NLTagger::initWithTagSchemes(NLTagger::alloc(), &NSArray::from_slice(&[NLTagSchemeLexicalClass]))`
- `tagger.setString(Some(&NSString::from_str(text)))`
- `tagger.enumerateTagsInRange_unit_scheme_options_usingBlock(…)` with a `block2::RcBlock`; if block enumeration is awkward, use `tagsInRange_unit_scheme_options_tokenRanges`.

If any of these is `unsafe`:

- keep it in a single `#[allow(unsafe_code)]` function in this file;
- add `style.rs` to the bridges named in `decisions/2026-10-01_194827978_allow-unsafe-code-only-in-named-macos-bridges.md` and to `CLAUDE.md`/`AGENTS.md`'s list.

Write a small `utf16_to_byte(text, utf16_offset) -> usize` helper with a unit test using `"Grüße 🦊 fox"`.

- [ ] **Step 6: Cache and draw.**
- Add `pos_cache: RefCell<HashMap<String, Rc<[(Range<usize>, Part)]>>>` to the editor, keyed by the line view's text like `spell_cache`. Fill it from a background task that tags uncached prose lines, then notifies, so drawing never waits; an uncached line draws without colours for one frame.
- In the line render, after grammar:

  ```rust
  let marked = mark_runs(&marked, &self.style_ranges(line, &view, &info.kind), InlineStyle::STYLE);
  for (part, style) in enabled_parts { marked = mark_runs(&marked, &ranges_of(part), style); }
  ```

  Ranges are display ranges filtered through `self.markable(line, &view, false)` (prose only).
- In `text_runs`:
  - the colour for a part-of-speech style applies only where the colour would otherwise be `base`, so links, code and markers keep theirs;
  - `STYLE` adds `UnderlineStyle { color: Some(theme.style_issue), thickness: px(1.), wavy: false }` only when neither `MISSPELLED` nor `GRAMMAR` is set (drawing order).
- In `theme.rs`, add `style_issue` (marker grey at 60%) and the part-of-speech colours:
  - adjective `#a0702a` / `#d4a259`
  - noun `#c4413b` / `#e8746e`
  - adverb `#8a4fb3` / `#b98ae0`
  - verb `#2f6fb3` / `#6aa5e6`
  - conjunction `#3b8a4f` / `#6cc283`

  Use Focal's existing `rgb` helper style.

- [ ] **Step 7: Settings, menus and the context menu.**
- In `settings.rs`, add `pos_highlight: PosSet` (bits per `Part`) and `style_check: bool`, both default off. In Settings ▸ Writing, add "Syntax highlight" with five small toggle chips and "Style check" as a switch, with the hint "Filler words, clichés and redundancies, in English and German".
- In `menus.rs`, View gets a "Syntax Highlight" submenu with checked items for the five parts plus "None", and a checked "Style Check" item.
- In `panel.rs`, the Info panel's Writing section gets a "Style check" switch and a "Syntax highlight" row that opens the same native menu.
- In the context menu built at `editor.rs` (around line 2854, the spelling menu), when the clicked offset is inside a style issue, add a disabled title item ("Filler word" / "Cliché" / "Redundancy", plus " — consider X" when there is a suggestion), then **Remove** for fillers (an edit deleting the word plus one adjacent space) and **Ignore**, which adds it to an in-memory `ignored_style: HashSet<String>` on the editor.
- Choose German per line: use the language the spell checker detects for the line (`spell.rs` already detects the language per line; expose `language_of(text) -> Option<String>` and test `starts_with("de")`).

- [ ] **Step 8: UI test:**

```rust
#[gpui_kit::test]
fn style_check_marks_fillers_only_when_on(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "It was very late.");
    editor.read_with(cx, |e, _| assert!(e.style_issues_for_test(0).is_empty(), "off by default"));
    set_settings(cx, |s| s.style_check = true);
    act(cx, window, |_, _| {});
    editor.read_with(cx, |e, _| assert_eq!(e.style_issues_for_test(0), [7..11]));
}
```

Add `style_issues_for_test(line) -> Vec<Range<usize>>`, returning display ranges.

- [ ] **Step 9: Check.** Run `mise run check`, then `mise run stress` with `pos_highlight` all on and `style_check` on, set in the stress task's settings if it reads them; otherwise measure with `FOCAL_TRACE=1` while typing in the 5,000-line document. Expected: keystrokes under 8 ms. Then take screenshots of the colours and underlines in both palettes.

- [ ] **Step 10: Commit:** `feat(style): parts of speech and a style check, in English and German`

---

### Task 5: Review notes (spec section 5)

**Files:**
- Create: `crates/focal-core/src/review.rs`, `crates/focal/src/editor/notes.rs`
- Modify:
  - `crates/focal/src/editor.rs` (marks, context menu, render)
  - `crates/focal/src/workspace.rs`, `workspace/panel.rs` (Notes tab)
  - `crates/focal/src/folder.rs` (exclude sidecars)
  - `crates/focal/src/switcher.rs` (via `folder::scan`)
  - `crates/focal-core/src/keys.rs`, `menus.rs`, `editor/modal.rs`
  - `crates/focal-core/src/analysis.rs` (`InlineStyle::NOTED`, bit 25)
  - `theme.rs` (`note`)
- Test: `review.rs` unit tests, `folder.rs` test, `ui_tests.rs`

**Interfaces:**
- Produces:
  - `review::Note { line: usize, quote: String, text: String }`
  - `review::Notes { title: String, notes: Vec<Note>, extra: String }`
  - `review::parse(text: &str) -> Notes`
  - `review::write(file_name: &str, notes: &[Note], extra: &str) -> String`
  - `review::anchor(text: &str, note: &Note) -> Anchor`, with `enum Anchor { Found(Range<usize>), Changed(usize /* line start */) }`
  - `review::prompt(file_name: &str, notes: &[Note]) -> String`
  - `review::sidecar_path(document: &Path) -> PathBuf`
  - `review::is_sidecar(path: &Path) -> bool`
  - `PanelTab::Notes`
  - actions `AddNote`, `CopyNotesForAgent`, `DeleteAllNotes`, `OpenNotesFile`
  - `AppCommand::AddNote`

- [ ] **Step 1: Write the failing core tests** in `review.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SIDECAR: &str = "# Review notes for notes.md\n\n## Line 12\n\n> The fox jumps.\n\nMake it a cat.\n\n## Line 3\n\n> Two\n> lines\n\nFirst line.\nSecond line.\n";

    #[test]
    fn parse_and_write_round_trip() {
        let notes = parse(SIDECAR);
        assert_eq!(notes.notes.len(), 2);
        assert_eq!(notes.notes[0], Note { line: 12, quote: "The fox jumps.".into(), text: "Make it a cat.".into() });
        assert_eq!(notes.notes[1].quote, "Two\nlines");
        assert_eq!(write("notes.md", &notes.notes, &notes.extra), SIDECAR);
    }

    #[test]
    fn unknown_content_is_kept() {
        let text = "# Review notes for notes.md\n\nSome preface.\n\n## Line 1\n\n> a\n\nb\n";
        let notes = parse(text);
        assert_eq!(write("notes.md", &notes.notes, &notes.extra), text);
    }

    #[test]
    fn nearest_occurrence_wins() {
        let text = "fox\nx\nx\nx\nfox\n";
        let note = Note { line: 5, quote: "fox".into(), text: String::new() };
        assert_eq!(anchor(text, &note), Anchor::Found(12..15));
    }

    #[test]
    fn a_rewritten_passage_is_changed_not_lost() {
        let note = Note { line: 2, quote: "gone".into(), text: "x".into() };
        assert_eq!(anchor("a\nb\n", &note), Anchor::Changed(2));
    }

    #[test]
    fn umlauts_and_crlf() {
        let text = "Grüße\r\nWelt\r\n";
        let note = Note { line: 2, quote: "Welt".into(), text: String::new() };
        assert_eq!(anchor(text, &note), Anchor::Found(8..12));
    }

    #[test]
    fn the_prompt_numbers_notes_and_names_the_sidecar() {
        let notes = [Note { line: 12, quote: "The fox jumps.".into(), text: "Make it a cat.".into() }];
        assert_eq!(
            prompt("notes.md", &notes),
            "Please revise notes.md according to these review notes. The notes are also saved in notes.review.md; delete each note there once it is handled.\n\n1. Line 12: \"The fox jumps.\"\n   Make it a cat.\n"
        );
    }

    #[test]
    fn sidecar_paths() {
        assert_eq!(sidecar_path(Path::new("/a/notes.md")), Path::new("/a/notes.review.md"));
        assert!(is_sidecar(Path::new("notes.review.md")));
        assert!(!is_sidecar(Path::new("review.md")));
    }
}
```

- [ ] **Step 2: Run them and see them fail.** Run `cargo test -p focal-core review`. Expected: compile errors.

- [ ] **Step 3: Implement `review.rs`.**
- `parse`: lines up to the first `## Line N` (other than the `# Review notes for …` heading) form `extra`. Each `## Line N` section holds the `> ` lines (the quote, joined with `\n`) and, after a blank line, the rest (the note, trimmed of the surrounding blank lines). A section that does not match is appended to `extra` verbatim.
- `write`: emits `# Review notes for {file_name}\n\n`, then `extra` (if any, followed by a blank line), then each note as `## Line {line}\n\n` + the quote lines prefixed with `> ` + `\n\n{text}\n`, with a blank line between notes. It matches the test fixture byte for byte.
- `anchor`: find all occurrences with `str::match_indices` (exact case). Take the one whose line (`text[..start].matches('\n').count() + 1`) is nearest `note.line`; with none, return `Changed(start of note.line, clamped)`.
- `prompt`: as in the test.
- `sidecar_path`: `with_extension("")` plus `.review.md`.
- `is_sidecar`: the file name ends with `.review.md` and has a non-empty stem before it.

- [ ] **Step 4: Run the tests and see them pass.** Run `cargo test -p focal-core review`. Expected: 7 passed.

- [ ] **Step 5: Keep sidecars out of lists.** In `folder::scan`, skip files whose name passes `focal_core::review::is_sidecar`. Add to the folder tests:

```rust
#[test]
fn review_sidecars_are_not_listed() {
    let root = temp_folder("sidecars");
    std::fs::write(root.join("a.md"), "").unwrap();
    std::fs::write(root.join("a.review.md"), "").unwrap();
    assert_eq!(scan(&root, Depth::Tree, SCAN_LIMIT), [PathBuf::from("a.md")]);
}
```

Tags, backlinks and the switcher all read `scan`'s list, so they follow.

- [ ] **Step 6: The editor's notes** (`editor/notes.rs`).
- Fields: `notes: Vec<Note>`, `notes_extra: String`, `note_draft: Option<NoteDraft { range: Range<usize>, editing: Option<usize>, input: Entity<InputState> }>`.
- Load on open and on reload: `review::parse(fs::read_to_string(sidecar_path)?)`.
- Watch the sidecar through the editor's existing file watcher. Read how the document's FSEvents watch is set up and add the sidecar path to it; a change reloads `notes`.
- `save_notes`: write `review::write(…)`, or delete the sidecar when `notes` is empty and `extra` is blank. Write through the same atomic save helper the document uses.
- `add_note(&AddNote)`:
  - The range is the selection, or the caret's line when it is empty.
  - It opens the draft card: a GPUI Kit `Input` under the selection, positioned from the painted layout like the spelling menu, with the placeholder "Note for the agent…".
  - Return saves the note: `Note { line, quote: text[range], text }`, sorted by line, then `save_notes`. Esc cancels.
  - Untitled documents show "Save the document to add notes" in the error banner.
- Marks:
  - `notes_on(line_range)` anchors every note (cached per buffer version) and returns display ranges, drawn with `mark_runs(…, InlineStyle::NOTED)`.
  - In `text_runs`, `NOTED` adds an underline `theme.note` (light `#d9a400`, dark `#e6c04a`, 2 px, not wavy), unless `MISSPELLED`, `GRAMMAR` or `STYLE` is set.
  - A right-margin dot (`div().absolute().right(px(-18.)).top(px(8.)).size(px(7.)).rounded_full().bg(theme.note)`, id `("note-dot", ix)`) shows on the first line of each anchored note. It has a tooltip with the note and opens the draft for editing on click.
  - A `Changed` note shows the dot hollow (a border only) on its recorded line.

- [ ] **Step 7: The Notes tab.**
- Add `PanelTab::Notes`, with a tab button "Notes" and, when there are notes, a count ("Notes 3"), id `panel-tab-notes`.
- The body, when empty, shows the explanation text from spec section 6, with the sidecar's actual name.
- Otherwise it shows a header row with the buttons `notes-copy` "Copy for Agent", `notes-open` "Open Notes File" and `notes-delete-all` "Delete All" (which confirms with `mac::alert`), then one row per note (`("note", ix)`): "Line 12", the quote in italics (two lines at most), the text, and Edit and Delete icons. A `Changed` note shows "passage changed" in the marker colour.
- Clicking a row moves the caret there.
- `CopyNotesForAgent` writes `review::prompt(file_name, &notes)` to the clipboard and shows "Copied 3 notes" in the tab for 2 s.

- [ ] **Step 8: Make it discoverable.**
- Add `actions!(focal, [AddNote, CopyNotesForAgent, DeleteAllNotes, OpenNotesFile])` and bind `cmd-alt-n` to `AddNote`.
- In the Edit menu, add "Add Note…" after Find, plus "Copy Notes for Agent".
- In the View menu, add "Notes" (opens the tab).
- In the editor's context menu (the native menu built around `editor.rs:2854` and `:2970`), add **Add Note…** at the top whenever there is a selection, and always as the last item.
- In `keys.rs`:
  - add `AppCommand::AddNote` with the menu key `('n', AppCommand::AddNote, "add a note")` (`n` must be free in both menus; it is);
  - add `(&["note"], Ex::App(AppCommand::AddNote), "add a review note")`;
  - add the reference line "add a review note".
- Map `AddNote` in `app_action`.

- [ ] **Step 9: UI tests.**

```rust
#[gpui_kit::test]
fn adding_a_note_writes_the_sidecar_and_copy_makes_a_prompt(cx: &mut TestAppContext) {
    let root = notes_folder("review-notes", "The fox jumps.\n");
    let (window, workspace) = open_folder(cx, &root);
    let editor = workspace.read_with(cx, |w, _| w.editor().clone());
    editor.update(cx, |e, cx| e.select(4..7, cx));
    act(cx, window, |window, cx| window.press("cmd-alt-n", cx));
    act(cx, window, |window, cx| window.input("Make it a cat", cx));
    act(cx, window, |window, cx| window.press("enter", cx));
    let sidecar = std::fs::read_to_string(root.join("note.review.md")).unwrap();
    assert!(sidecar.contains("## Line 1\n\n> fox\n\nMake it a cat\n"), "{sidecar}");
    editor.read_with(cx, |e, _| assert_eq!(e.text(), "The fox jumps.\n", "document untouched"));
    act(cx, window, |window, cx| window.dispatch_action(Box::new(crate::editor::CopyNotesForAgent), cx));
    let copied = cx.read_from_clipboard().and_then(|i| i.text()).unwrap();
    assert!(copied.starts_with("Please revise note.md"));
}

#[gpui_kit::test]
fn deleting_the_last_note_removes_the_sidecar(cx: &mut TestAppContext) {
    // As above, then click ("note-delete", 0) and assert the sidecar is gone.
}
```

Write the second test fully, following the first: add a note, then `act(cx, window, |w, cx| w.click(("note-delete", 0usize), cx))` and `assert!(!root.join("note.review.md").exists())`. Adjust file names to `notes_folder`'s.

- [ ] **Step 10: Decision record.** Run `vrdx guide`, then create `vrdx new "Keep review notes in a sidecar file beside the document" --body-file /tmp/review-decision.md` with status `accepted` (Niklas chose this option on 2026-10-04). Cover:
  - Context: agents need the notes and the document must stay untouched.
  - Decision: `NAME.review.md`, the format above, hidden from lists.
  - Alternatives: Application Support only; inline HTML comments.
  - Consequences: a file appears in the repo, which a user may gitignore.

  Then run `vrdx validate`.

- [ ] **Step 11: Check.** Run `mise run check`, then take screenshots of a noted passage, the dot, the card and the Notes tab in light and dark.

- [ ] **Step 12: Commit:** `feat(review): notes for agents in a sidecar file, with a Notes tab`

---

### Task 6: Docs, end-to-end check, finish

**Files:** `docs/design.md`, `README.md`, the key reference (already updated through `keys.rs`), `docs/superpowers/specs/…` (status line).

- [ ] **Step 1: Update the design doc.** In `docs/design.md`, add an "M13 — Review and writing" milestone row and a short "Agreed (2026-10-04)" entry per feature in section 3 (Documents and Folder mode), linking the spec.
- [ ] **Step 2: Update the README.** Add five feature bullets in its features list (read its current structure first), each naming its menu item and shortcut.
- [ ] **Step 3: Check the round trip with everything on.** Open the round-trip property test (search `round_trip` in `focal-core`). If it is pure core, add one UI test: open a file, turn on style check and syntax highlight, add and delete a note, dismiss changes, close, and assert the file's bytes are unchanged.
- [ ] **Step 4: Run the full checks.** Run `mise run check` and `mise run stress`. Expected: pass, keystrokes under 8 ms.
- [ ] **Step 5: Review the branch** with superpowers:requesting-code-review, and fix what it finds.
- [ ] **Step 6: Clean up and finish.** Run `mise run clean`, commit `docs: milestone 13, review and writing`, push the branch, and use superpowers:finishing-a-development-branch to decide whether and how to merge into `main`.
