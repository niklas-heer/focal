+++
schema_version = 1
id = "01M44C05RNRN1KEHYCQW7GSQB1"
title = "Keep review notes in a sidecar file beside the document"
date = "2026-10-04"
status = "accepted"
tags = ["review", "files"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Review notes for agents are kept in a sidecar file beside the document: `NAME.review.md` next to `NAME.md`, created with the first note and deleted with the last. The document itself is never changed by a note.

The file is plain Markdown that an agent can read on its own: a `# Review notes for NAME.md` heading, then one `## Line N` section per note holding the quoted passage (`> …`) and the note. Focal reads it back (unknown content is kept), re-reads it when it changes on disk, and anchors each note by searching for its quote nearest its recorded line; a note whose passage is gone stays listed as "passage changed". Edit › Copy Notes for Agent puts a numbered prompt on the pasteboard that names the sidecar and asks the agent to delete the notes it handled.

Sidecars are left out of the sidebar, the quick switcher, tags and backlinks (`focal_core::review::is_sidecar` in `folder::scan`); the Notes tab and View › Open Notes File reach them.

Chosen by Niklas on 2026-10-04 while designing Milestone 13 ("Sidecar file + copy", the recommended option).

## Context

Focal's primary use is reviewing a file, often one an agent wrote. Feedback has to reach the agent, and the invariant "the file is the document" rules out writing notes into it.

Alternatives considered:

- **Notes inside Focal only** (Application Support) **plus copying**: nothing appears in the repository, but an agent only sees the notes when they are pasted to it, and they are lost when the document moves.
- **Inline HTML comments** (`<!-- note -->`) in the document: agents see them directly, but this edits the file and every renderer and diff carries them.

## Consequences

- A `*.review.md` file appears beside reviewed documents and in `git status`; users may commit it or add `*.review.md` to `.gitignore`.
- Agents can read and update the notes without Focal; Focal follows their edits through live reload.
- A document named `x.review.md` is treated as a sidecar and is not listed.
