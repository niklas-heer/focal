+++
schema_version = 1
id = "01M3SXD6YZ698MVJ6QXR2QKFTH"
title = "Build the editor on TextKit 1"
date = "2026-09-30"
status = "rejected"
tags = ["architecture", "rendering"]
supersedes = []
superseded_by = []
depends_on = ["01M3N1CC6YF89Y8YAY1QJW5ZWS"]
related_to = ["01M3SV51ATT58APSGWMYVXBYJG"]
+++
## Decision

Proposed: build Focal's main text view on TextKit 1 (`NSLayoutManager`). Markdown markers are hidden with null glyphs; tables are islands whose source stays in storage, drawn as grid subviews over a reserved line while the table's other lines collapse through paragraph styles.

## Context

Milestone 0 asked which TextKit version can carry live Markdown rendering: hidden and revealed markers, embedded table grids whose height changes, and smooth scrolling of a 5,000-line document within an 8 ms keystroke budget. A throwaway spike built both engines against the same controller and measured them (branch `spike/m0-textkit`, commit `6ffb924`, local). Findings: [`docs/spikes/2026-09-29-m0-textkit.md`](../docs/spikes/2026-09-29-m0-textkit.md).

Applying the plan's decision rule, neither engine had a blocker, and TextKit 1 had no stability problem while TextKit 2 had one:

- Revisiting a scroll offset shows different text on TextKit 2 (`scrollDriftChars` 248,886; 134 height re-estimates) and the same text on TextKit 1 (0; 83).
- Scroll steps: TextKit 1 p95 2.0 ms, TextKit 2 8.3 ms.
- Restyle after a keystroke: p95 15.2 ms (TextKit 1) and 14.8 ms (TextKit 2), both over the 8 ms budget; the cost is the shared controller's full reparse and restyle.

TextKit 2 handled the islands more cleanly (grid placed correctly on first layout, no leftover fence lines, focus kept across view rebuilds) and opened the document faster (123 ms vs 302 ms). TextKit 1 needed a fix so table line breaks keep their control glyphs, still places the grid one line too high until the first edit, and logs `invalid glyph index` warnings. The choice rests on scroll stability, the design's fifth principle, which the Electron prototype failed.

Some manual results are provisional because an agent ran them with synthetic events: the scroll rating (no flicker can be judged from screenshots) and VoiceOver (the accessibility tree was inspected, not listened to).

## Consequences

- Milestone 1 builds on `NSLayoutManager` with non-contiguous layout, owns grid placement from laid-out line rectangles, and must investigate the `invalid glyph index` warnings.
- Milestone 1 must keep the caret out of hidden table source (↑/↓ and clicks next to a grid), reparse from the edited block to meet the budget, and expose grid cells to accessibility.
- TextKit 1 is Apple's older path; revisit if a future macOS makes TextKit 2 keep stable heights while scrolling, or deprecates TextKit 1.

## Outcome

Rejected on 2026-09-30: Niklas chose to build Focal in Rust with GPUI instead (decision `01M3SV51ATT58APSGWMYVXBYJG`). The Milestone 0 findings remain the record of how TextKit 1 and 2 compared.
