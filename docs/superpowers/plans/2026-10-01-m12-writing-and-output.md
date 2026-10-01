# Milestone 12 — Writing and Output Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Niklas's decisions of 2026-10-01: grammar checking and autocorrect, PDF export and printing, focus mode that dims syntax and chrome, CI with dependency updates, plus the leftovers: diagram-tool hints in Settings, raw HTML tables, remembered folds, measured Vega text.

**Spec:** [`docs/design.md`](../../design.md) sections 6, 7a, 8 and 10 (decisions recorded there by this milestone).

## Global Constraints

- The file is the document: grammar suggestions and autocorrect are ordinary edits, one undo step each; nothing changes the file unasked except autocorrect, which follows the macOS setting "Correct spelling automatically".
- No checking or correcting inside code, math, links, URLs, front matter, HTML or diagrams.
- PDF and printing render exactly what Export as HTML renders (one renderer for HTML, PDF and paper).
- Apple Silicon only (decided 2026-10-01): no universal binaries.
- `mise run check` after every task; core logic with unit tests; UI behavior with headless UI tests.
- Branch `m12-writing`; conventional commits with the session's `Co-Authored-By` line; `mise run clean` at the end.

## Tasks

1. **Settings: diagram tools.** A section listing `dot`, `d2`, `plantuml` as found or not, with the install command; README section.
2. **Folds remembered.** Opened and closed folds persist per file in Application Support.
3. **Vega measures text.** Text widths from the real font (Helvetica metrics through `ttf-parser`), not estimates.
4. **Raw HTML tables.** `<table>` blocks draw as a read-only grid island; the caret shows the source.
5. **Focus mode dims syntax and chrome.** Revealed markers, list markers and quote bars dim with the text; the title dims too.
6. **Grammar and autocorrect.** `NSSpellChecker` grammar ranges underlined in green with suggestions in the context menu; autocorrect at word boundaries when the system setting is on, undoable, outside code and links.
7. **PDF export and printing.** Through an offscreen WebKit view loading the exported HTML (decision record, proposed): Export as PDF… and Print….
8. **CI and dependency updates.** GitHub Actions on macOS runs `mise run check`; Dependabot proposes Cargo and Actions updates weekly; a Claude Code workflow repairs a failing dependency PR (decision record, proposed).
9. **Docs.** Design section 10 and milestones; README; decisions for Apple Silicon only and the name.
