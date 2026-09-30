+++
schema_version = 1
id = "01M3SV51ATT58APSGWMYVXBYJG"
title = "Build Focal in Rust with GPUI"
date = "2026-09-30"
status = "proposed"
tags = ["architecture", "platform"]
supersedes = []
superseded_by = []
depends_on = []
related_to = ["01M3N1CC685R2SEASNER5DE1ZT", "01M3N1CC76F8J8BK28GQTPV63M"]
+++
## Decision

Proposed, not accepted: build Focal in Rust with GPUI (through `gpui-kit`) instead of Swift and AppKit. One UI-free Rust crate, `focal-core`, holds the editor model (Markdown analysis with `pulldown-cmark`, per-line display maps, the buffer and undo); the app crate draws each source line as GPUI styled text with markers hidden or replaced away from the caret, in GPUI's virtualized list.

## Context

Niklas asked on 2026-09-30 to try the same app idea in Rust with GPUI, Zed's GPU-accelerated UI framework, on a branch. The `rust-gpui` branch holds a working spike; its findings are in `docs/gpui-spike.md`.

Evidence from the spike (Apple M4, release build):

- Live rendering works for inline styles, headings, lists and tasks, quotes and alerts, code blocks, front matter, rules and read-only table grids, with stable scrolling. The Electron prototype's failure mode did not appear.
- On a 5,007-line document with 50 tables and 50 code blocks, reparsing and restyling everything takes about 4 ms per keystroke, inside the 8 ms budget, before GPUI's own layout and paint.
- The Swift-to-Rust C interface in the accepted parser decision disappears; the whole model is unit-tested Rust.

Costs found:

- No system spell checking, grammar, autocorrect, Look Up or Writing Tools. VoiceOver needs AccessKit work for the custom text element.
- No `NSDocument` (versions, native tabs, file coordination); Mermaid and math need non-web, non-SwiftMath renderers.
- GPUI text lacks paragraph styles (no hanging indent for wrapped list items) and pads no run backgrounds.
- The static iA Writer Quattro S Bold files report weight 400, so GPUI cannot select them; the spike uses Duo S Bold for bold prose.
- `gpui-kit` pins `gpui-pre`, a community-published snapshot of Zed's in-tree GPUI, so upgrades follow that publisher.

The alternative is the accepted AppKit plan with its TextKit Milestone 0 spike.

## Consequences

- If accepted, this supersedes "Build Focal as a native AppKit application" and changes the parser decision's C interface into a plain Rust dependency. Milestones 1 to 6 would need replanning around the gaps above, especially accessibility and spell checking.
- If rejected, the `rust-gpui` branch remains a reference: `focal-core`'s analysis and display-map logic can still back the AppKit app through the planned C interface.
