+++
schema_version = 1
id = "01M428KGJEA3K3BKYQDDSY51DY"
title = "Build Vim mode as a state machine in focal-core"
date = "2026-10-04"
status = "proposed"
tags = ["editing", "vim"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Focal's Vim mode is a state machine of its own in `focal-core` (`vim.rs`): keys go in, and selections, edits and requests for the editor come out. The editor applies them through its usual edits and moves, so undo, autosave, tables, folding and Markdown-aware typing behave as without Vim. Keys reach it through a GPUI keystroke interceptor before Focal's bindings; keys with ⌘ stay Focal's, and insert mode types through Focal's own input path.

It covers normal, insert, visual and visual-line modes; counts; `d`, `c`, `y`, `>`, `<`, `gu`, `gU`, `g~`; the common motions (`hjkl`, `w b e ge` and their WORD forms, `0 ^ $`, `gg G`, `{ }`, `f t F T ; ,`, `%`); text objects for words, sentences, paragraphs, quotes and brackets; `x X s S C D Y p P r J ~ . u ⌃R`; `/ ? n N * #` through the find bar; and `:w`, `:q`, `:wq`, `:x` and `:N`.

Choices made for a Markdown editor rather than for faithfulness:

- `j` and `k` alone move by screen line through the editor's own vertical move, which also enters tables and steps over diagrams; with an operator or in visual mode they take lines of text, as Vim does.
- A change and the text typed after it undo as one step, through undo groups added to `Buffer`.
- Yanks go to the system clipboard; `p` pastes Vim's register unless the clipboard was copied elsewhere since.
- `o` continues lists and quotes, as Return does; `>` and `<` nest list items as Tab and ⇧Tab do.

Proposed on 2026-10-04 while building it. Niklas asked for a Vim mode that can be toggled on; the approach is not yet chosen by him.

## Context

GPUI has no Vim layer for a custom editor; Zed's Vim mode is written against Zed's own editor and is not a reusable crate. A text-in, commands-out machine can be unit-tested without a window (a small simulated editor in its tests) and keeps the editor the one place that changes text, which the round-trip invariant needs.

Alternatives considered: key bindings per Vim key in GPUI's keymap (counts, operators and pending keys do not fit bindings), and editing the buffer from Vim directly (would bypass list continuation, table grids and the editor's undo selections).

## Consequences

- Missing for now: registers by name, marks, macros, replace mode `R`, blockwise visual mode, `H M L`, `zz` and scrolling by real page height (⌃D and ⌃U move 15 lines).
- The engine counts lines of source text; hidden Markdown syntax is visible to it, so `0` on a list item lands after the marker, where Focal puts the caret.
- Every new editor feature reached by keys should be checked with Vim mode on.
