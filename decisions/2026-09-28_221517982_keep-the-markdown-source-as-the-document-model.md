+++
schema_version = 1
id = "01M3N1CC6YF89Y8YAY1QJW5ZWS"
title = "Keep the Markdown source as the document model"
date = "2026-09-28"
status = "accepted"
tags = ["architecture", "rendering", "files"]
supersedes = []
superseded_by = []
depends_on = ["01M3N1CC685R2SEASNER5DE1ZT"]
related_to = []
+++
## Decision

Focal's document model is the Markdown source text itself. One native text view holds the file's exact text. Rendering adds attributes, hides syntax markers until the caret enters them, and places embedded native views ("islands") for tables, display math, Mermaid diagrams, images and front matter, whose source stays in the text storage.

Opening and saving an unedited file must produce identical bytes, including line endings, the trailing newline and whitespace. The one permitted rewrite is a table edited in its grid, which is serialized with aligned columns; unedited tables are never touched. Niklas chose the aligned output knowing it produces larger diffs than preserving the original spacing.

## Context

Focal is mainly used to open and review Markdown files in Git repositories, often files that agents write. Editors that parse Markdown into a model and serialize it back (Typora, MarkText) quietly reformat files, which produces noisy diffs and changes other people's text. Bear and iA Writer both keep the source and change only how it looks.

Alternatives considered:

- A block-based editor (Notion-style stack of per-block editors): natural for tables and diagrams, but selection across blocks, undo, find and copy of several paragraphs must all be rebuilt by hand, and it loses the feel of one continuous page.
- A model-and-serialize WYSIWYG editor: simplest rendering, but breaks the exact round-trip.

## Consequences

- System text features (selection, undo, find, spell check, input methods, accessibility) work across the whole document for free.
- Hiding markers and embedding live views inside a text view is the hard part. Whether TextKit 1 or TextKit 2 does this better is settled by the Milestone 0 spike and its own decision record.
- Caret movement, copy and selection must treat hidden markers deliberately: no stuck caret positions, and copy puts Markdown source on the pasteboard.
- Round-trip byte identity becomes a tested invariant.
