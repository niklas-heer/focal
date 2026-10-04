+++
schema_version = 1
id = "01M42B02B0CRG6SA9NA8JJ8MFJ"
title = "Add Helix mode beside Vim behind one modal interface"
date = "2026-10-04"
status = "accepted"
tags = ["editing", "helix"]
supersedes = []
superseded_by = []
depends_on = []
related_to = ["01M428KGJEA3K3BKYQDDSY51DY"]
+++
## Decision

Helix mode is a second state machine in `focal-core` (`helix.rs`), beside Vim's, speaking the same keys and commands. `modal.rs` puts both behind one type, `Modal`, which the editor drives exactly as it drove Vim; the setting `keyboard` chooses Standard, Vim or Helix.

Helix's selection is kept as an anchor and a head, both on characters and both included, and shown to the editor as an ordinary selection; a selection of one character shows as the block cursor alone. Motions select (`w` the word and its space, `x` the line), `v` extends instead, and actions work on the selection. It reuses Vim's text helpers and text objects.

Proposed on 2026-10-04 while building it, after Niklas asked for Helix mode beside Vim mode.

Accepted by Niklas on 2026-10-04: "I like your idea and also the Helix approach, I think."

## Context

Helix inverts Vim's grammar (selection, then action), so sharing Vim's parser would have meant a mode flag through every operator. Its own small parser and executor stay readable and are unit-tested the same way, through a simulated editor.

## Consequences

- One selection only. Helix's multiple cursors (`C`, `s`, `S`, `alt-s`) would need the editor to hold several selections, which it does not.
- Not built: space mode and its pickers, registers by name, `alt-` bindings (`alt-;`, `alt-d`), tree-sitter selections (`alt-o`), and goto mode beyond `gg ge gh gl gs gj gk`.
- Plain `j` and `k` move by screen line, as in Vim mode.
