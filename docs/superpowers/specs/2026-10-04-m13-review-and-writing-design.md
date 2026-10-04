# Milestone 13 — Review and Writing: Design

**Status:** design agreed with Niklas on 2026-10-04 (all four open choices as recommended). This spec turns it into something to plan against.

**Why:** Focal's primary use is reading and reviewing a file, often one an agent just wrote (design section 1). Today a rewrite reloads silently, pasting is plain text only, links only go one way, and there is no way to hand feedback back to an agent. These five features close those gaps, in this order:

1. Smarter paste
2. Backlinks
3. What changed
4. Style tools
5. Review notes

Two requirements hold for all of them (Niklas, 2026-10-04):

- **They must not interfere with what exists.** Section 7 lists the rules each feature is checked against.
- **They must be obvious.** Someone who never read the docs should find each feature where they would look for it. Section 6 lists every entry point.

## 1. Smarter paste

**What it does**

- **URL over a selection.** ⌘V with text selected and a single URL (`http`, `https`, `mailto`) on the pasteboard writes `[selection](url)`. A URL with nothing selected pastes as written.
- **Rich text.** When the pasteboard holds HTML (`public.html`), it is converted to Markdown with the `htmd` crate and pasted. The plain text is pasted instead when:
  - the HTML is only plain text in spans or divs (code editors, terminals), or
  - the caret is in a code block, inline code, math, front matter or a table cell.
- **Paste as Plain Text** (Edit menu, ⌥⇧⌘V, the macOS "Paste and Match Style" keys) always pastes the plain text.
- **Images.** A pasted image (`ClipboardEntry::Image`), or image files dropped onto the editor (`ExternalPaths`), are written into the assets folder and inserted as `![](assets/name.png)`, one per line when several.
  - **Folder:** `assets/` beside the document by default, a setting (Settings ▸ Writing ▸ Images folder).
  - **Names:** a dropped file keeps its name; a pasted image is named `pasted-YYYY-MM-DD-HHMMSS.png`. An existing file is never overwritten (`-2`, `-3` …).
  - **Paths** are written with `/` and percent-encoded spaces, so other renderers follow them.
  - **An untitled document** asks to be saved first (the standard Save panel), since a relative path needs a folder.
- **Other dropped files.** A Markdown or other non-image file inserts a relative link, `[name](path)`. A dropped folder does nothing.
- **Undo:** each paste or drop is one undo step. Writing the image file is not undone (the file stays, as in Bear and Obsidian).

**Where:** link and HTML decisions in `focal-core/src/paste.rs` (pure, unit tested); pasteboard reads in `mac.rs` (`NSPasteboard` `stringForType`, already a safe call); file writing and drop handling in the editor.

## 2. Backlinks

**What it does:** The Info panel gets a **Linked from** section listing the files whose links lead to the open file:

- wiki links `[[name]]`, resolved as `links::resolve_wiki` already resolves them;
- relative Markdown links `[x](path.md)`, `#anchor`s ignored.

Each entry shows the file's name and the line holding the link, with the link text emphasised. Clicking opens the file with the caret on the link, and ⌘[ comes back. An empty list says "No other file links here." The section shows only when the window has a file list (folder mode, or the files beside a single file).

**How:** Links are extracted during the background scan that already reads tags (same file list, same 2 MB limit, same FSEvents updates), into `focal-core/src/links.rs`:

- `outgoing_links(text) -> Vec<(Range<usize>, Target)>`
- an index `HashMap<PathBuf, Vec<Backlink>>` built from them.

`*.review.md` sidecars are skipped (section 5).

## 3. What changed

**Baselines**

- **Since opened (default, always on).** The editor keeps a baseline text that follows your own edits, so they are never marked.
  - When the file is reloaded from disk (live reload, or "Load theirs"), the text before the reload stays as the baseline.
  - From then on, the differences show, and your own later edits show with them.
  - **Dismiss Changes** sets the baseline to the current text and the marks disappear. Opening another file starts fresh.
- **Since last commit (toggle).** View ▸ Changes ▸ Compare with Last Commit uses `git show HEAD:<path>` (run with the file's folder as working directory, off the main thread) as the baseline until it is turned off.
  - It is offered only when the file is inside a git work tree.
  - A file not yet in HEAD shows as entirely new.
  - The setting is per window, not saved.

**The diff:** `focal-core/src/diff.rs`, with the `similar` crate. It diffs lines first, then words within changed lines (word = the same classes the editor's word motions use), and returns:

- `inserted: Vec<Range<usize>>` in the current text;
- `deleted: Vec<(usize, String)>` (an offset in the current text, and the removed text);
- `hunks: Vec<Range<usize>>` (whole changed regions, for navigation).

It runs in the background after a reload or a commit-baseline load, and after edits when changes are showing (debounced like spelling). CRLF and CR files diff on their own line endings; offsets stay on character boundaries.

**Drawing (stable, like spelling underlines)**

- **Inserted words:** a soft green tint behind the text (`theme.inserted`, new, light and dark).
- **Deleted text:** a small red wedge at its position. Hovering shows the deleted text in a tooltip-style card.
- **Margin:** a 3 px bar left of the text column beside every line holding a change (green for inserted or changed, red for a deletion only).
- **Hidden markers and islands:** a change inside them marks the line's bar only.

**Moving and status**

- ⌥⌘↓ and ⌥⌘↑ go to the next and previous change, wrapping. Vim `]c`/`[c`; Helix `]g`/`[g`.
- **Changes pill:** while changes show, a pill at the bottom right (mirroring the mode pill, never over the bar's buttons) says "3 changes" (or "3 changes since last commit"), with ↑ ↓ and ✕ (Dismiss). Clicking the text goes to the next change.
- Esc with Standard keys, when nothing else uses it (no find bar, no selection), dismisses as well.

**Accessibility:** inserted ranges are exposed as "inserted" text attributes in the accessibility chunks, and deleted text as a "deleted: …" annotation at its offset. The pill is a labelled button group.

## 4. Style tools

**Parts of speech (View ▸ Syntax Highlight ▸ …).** Check items for Adjectives, Nouns, Adverbs, Verbs and Conjunctions; all off by default.

- **Tagging:** `NLTagger` with the `lexicalClass` scheme (macOS NaturalLanguage, through `objc2-natural-language`), per paragraph, in the paragraph's dominant language, English and German included.
- **Colors (iA Writer's):** adjectives brown, nouns red, adverbs purple, verbs blue, conjunctions green; light and dark variants in `Theme`.
- **Prose only:** never code, math, links' URLs, front matter, HTML, diagrams or table syntax.
- **Performance:** paragraphs are tagged in the background and cached by content hash, so typing re-tags only the edited paragraph and stays within the 8 ms keystroke budget.

**Style Check (View ▸ Style Check).** Off by default.

- **Word lists** in `focal-core/src/style.rs`, in two languages (English and German), each split into filler words (`very`, `really`, `just` / `eigentlich`, `halt`, `wirklich` …), clichés and redundancies (`end result`, `past history` / `neu renoviert`, `bereits schon` …).
- **Matching:** whole words or phrases, ignoring case, in prose only. Each line uses the language spelling already detects for it.
- **Drawing:** a dotted gray underline (distinct from spelling's red and grammar's green), as iA Writer does.
- **Right-click** on one shows "Filler word", "Cliché" or "Redundancy", plus **Remove** (for fillers) and **Ignore** (for this document, kept in memory only).

**Settings ▸ Writing** gets both, mirroring the View menu. The Info panel's Writing section (which already toggles focus mode and the keys) shows them too.

**Bridge:** if `NLTagger`'s bindings need `unsafe`, it is confined to `style.rs`'s tagger, which then joins the named macOS bridges, and the decision "Allow unsafe code only in named macOS bridges" is updated.

## 5. Review notes

**Adding a note**

- Select a passage, then use any of:
  - right-click ▸ **Add Note…**;
  - Edit ▸ **Add Note…** (⌥⌘N);
  - Vim `\n`, Helix `space n`, `:note`.
- A small card opens under the selection with a text field; Return saves and Esc cancels.
- With no selection, the note is about the caret's line.

**Storage:** `NAME.review.md` beside `NAME.md`, created with the first note and deleted with the last. The format is plain Markdown an agent can read on its own:

```markdown
# Review notes for notes.md

## Line 12

> The fox jumps over the lazy dog.

Make this sentence about the cat instead.
```

- **Parsing:** headings of the form `## Line N` start a note, the quote is the anchor, and the rest is the note. Unknown content is kept as written (the sidecar round-trips too).
- **Live reload:** the sidecar reloads when it changes on disk (an agent may delete notes it handled).

**Anchoring:** a note is placed by finding its quote, preferring the occurrence nearest its recorded line. When the quote is gone (an agent rewrote the passage), the note stays in the list marked **passage changed**, attached to its recorded line. It is never silently dropped.

**Drawing**

- A soft yellow underline under the noted passage, and a small note dot in the right margin beside its line.
- Hovering the dot or the passage shows the note; clicking the dot opens it for editing.

**Notes tab:** a fourth panel tab, **Notes**, beside Info, Outline and Recent.

- Each note shows its line, quote and text; clicking goes there.
- Each note has Edit and Delete.
- At the top: **Copy for Agent** and **Delete All**.
- The tab's title shows a count when notes exist ("Notes 3").

**Copy for Agent** puts a prompt on the pasteboard:

```text
Please revise notes.md according to these review notes. The notes are also saved in notes.review.md; delete each note there once it is handled.

1. Line 12: "The fox jumps over the lazy dog."
   Make this sentence about the cat instead.
```

**With the change view:** after an agent rewrites the file, its changes show (section 3), and notes whose passage changed say so. Reviewing a round therefore takes one look.

**Sidecars stay out of the way:** `*.review.md` files are left out of the sidebar, the quick switcher, tags and backlinks, and are reached through the Notes tab ("Open Notes File"). This adds a file to the user's folder, so a decision record (proposed) documents the format and location.

## 6. Making them obvious (discoverability)

Every feature has a menu item; menus are where Mac users look. Each also has its in-context entry point and a line in the key reference (Help ▸ Editing Keys and `?` in the space menu).

| Feature | Where it shows itself |
| --- | --- |
| Paste | Edit ▸ Paste as Plain Text (⌥⇧⌘V) beside Paste. While files are dragged over the editor, a drop overlay says "Add 2 images to assets/" or "Link 1 file". Pasting a URL over a selection simply makes the link. |
| Backlinks | Info panel section "Linked from" with its count; View ▸ Linked From… opens the Info panel at that section; space menu `l` and `:links`. |
| Changes | The changes pill appears by itself after an outside change; it is the main entry point. View ▸ Changes submenu: Next Change, Previous Change, Dismiss Changes, Compare with Last Commit (checked). Space menu `c`, `:changes`. |
| Style | View ▸ Syntax Highlight submenu and View ▸ Style Check; Settings ▸ Writing; the Info panel's Writing section. Right-clicking an underlined word explains it. |
| Notes | Right-click ▸ Add Note… on any selection; Edit ▸ Add Note… (⌥⌘N); the Notes tab, which when empty explains: "Select text and choose Add Note (⌥⌘N). Notes are saved beside the file in notes.review.md, and Copy for Agent hands them to an agent." Space menu `n`, `:note`. |

The README's feature list and the in-app help gain a short section for each. Tooltips name every new button and show its shortcut, as the bar's do.

## 7. Not interfering (existing functionality)

**The file is the document.**

- Paste and drop are ordinary edits, one undo step each.
- Style **Remove** is an ordinary edit.
- Nothing else touches the document: changes, style marks, backlinks and notes are drawn only.
- The sidecar is a separate file. A round-trip property test still passes with every feature on.

**Keys.** New shortcuts were checked against every existing binding (2026-10-04):

- ⌥⌘↓ ⌥⌘↑ ⌥⌘N ⌥⇧⌘V are free;
- `[` and `]` are unused in both Vim and Helix;
- the space and `\` menu keys `c`, `l` and `n` are free.

Esc keeps its existing meanings first (find bar, completions, Vim/Helix). Dismiss Changes takes it only with Standard keys and only when it would do nothing else.

**Layers.** Drawing order, bottom to top:

1. change tint
2. note underline
3. style underline
4. find matches
5. selection
6. spelling/grammar

Each uses a distinct shape (tint, solid yellow, dotted gray, find tint, selection, wavy), so none hides another. Change bars sit in the left margin, note dots in the right; neither overlaps list prefixes, quote bars or fold toggles, and both shift with them in focus mode, which dims all marks with the text.

**Conflicts.** The existing "Keep mine / Load theirs" banner keeps its priority. Changes show after "Load theirs", never instead of the banner.

**Pills.** The mode pill (bottom left) and the changes pill (bottom right) never cover the floating bar, which stays centred. Both keep clear of its show zone.

**Tables and islands.** Paste into a table cell stays plain text, with images allowed. Changes inside an island or table mark only the margin.

**Performance.** Diffing, tagging, link scanning and sidecar parsing run off the main thread. `mise run stress` stays within the 8 ms keystroke budget with everything on (checked in the last task).

**Accessibility.** The new marks and pills are in the accessibility tree (section 3; notes as annotations).

**Existing tests** all keep passing. Each feature adds UI tests that exercise it together with the features it touches (find, Vim/Helix, tables, live reload).

## 8. Out of scope

- Inline (Writing Tools–style) diff acceptance or rejection per hunk. Dismiss is all or nothing.
- Comparing with commits other than HEAD, or staged versions.
- Image resizing, or uploading pasted images anywhere.
- Style tools for languages other than English and German (parts of speech still work where macOS supports them).
- Threaded discussions or authors in review notes.
