+++
schema_version = 1
id = "01M3SV51ATT58APSGWMYVXBYJG"
title = "Build Focal in Rust with GPUI"
date = "2026-09-30"
status = "accepted"
tags = ["architecture", "platform"]
supersedes = ["01M3N1CC685R2SEASNER5DE1ZT"]
superseded_by = []
depends_on = []
related_to = ["01M3N1CC685R2SEASNER5DE1ZT", "01M3N1CC76F8J8BK28GQTPV63M"]
+++
## Decision

Build Focal in Rust with GPUI (through `gpui-kit`) instead of Swift and AppKit. One UI-free Rust crate, `focal-core`, holds the editor model (Markdown analysis with `pulldown-cmark`, per-line display maps, the buffer and undo); the app crate draws each source line as GPUI text with markers hidden or replaced away from the caret, in GPUI's virtualized list. macOS services GPUI lacks, starting with spell checking, are reached through `objc2` bindings.

Niklas accepted this on 2026-09-30, after the TextKit Milestone 0 spike and after a gate showed spell checking and VoiceOver support could be built on GPUI. It supersedes the AppKit decision; the TextKit 1 proposal from Milestone 0 was rejected.

## Context

Niklas asked on 2026-09-30 to try the same app idea in Rust with GPUI, Zed's GPU-accelerated UI framework, on a branch. The `rust-gpui` branch holds a working spike; its findings are in `docs/gpui-spike.md`.

Evidence from the spike (Apple M4, release build):

- Live rendering works for inline styles, headings, lists and tasks, quotes and alerts, code blocks, front matter, rules and read-only table grids, with stable scrolling. The Electron prototype's failure mode did not appear.
- On a 5,007-line document with 50 tables and 50 code blocks, reparsing and restyling everything takes about 4 ms per keystroke, inside the 8 ms budget, before GPUI's own layout and paint.
- The Swift-to-Rust C interface in the accepted parser decision disappears; the whole model is unit-tested Rust.

Costs found:

- No system grammar checking, autocorrect, Look Up or Writing Tools. Spell checking and VoiceOver have to be built; a gate on 2026-09-30 built both (macOS's `NSSpellChecker` through `objc2-app-kit`; AccessKit text runs, selection and tables) and checked them through the accessibility API, with the limits listed in `docs/gpui-spike.md`.
- No `NSDocument` (versions, native tabs, file coordination); Mermaid and math need non-web, non-SwiftMath renderers.
- GPUI text lacks paragraph styles (no hanging indent for wrapped list items) and pads no run backgrounds.
- The static iA Writer Quattro S Bold files report weight 400, so GPUI cannot select them; the spike uses Duo S Bold for bold prose.
- `gpui-kit` pins `gpui-pre`, a community-published snapshot of Zed's in-tree GPUI, so upgrades follow that publisher.

The alternative is the AppKit plan. Its Milestone 0 spike (on `main`, `docs/spikes/2026-09-29-m0-textkit.md`) chose TextKit 1 by a narrow margin, measured a 15 ms keystroke cost against the same 8 ms budget, and found that neither TextKit engine kept the caret out of hidden table source or exposed table grids to VoiceOver.

## Consequences

- Supersedes "Build Focal as a native AppKit application". The parser decision stands, but its C interface is no longer needed: `pulldown-cmark` is an ordinary Rust dependency.
- Milestones are replanned on the Rust stack (design section 8). Grammar checking, autocorrect, Look Up and Writing Tools stay unavailable unless built through `objc2`.
- Revisit if GPUI (or `gpui-pre`/`gpui-kit`) stops being maintained, or if accessibility or text services turn out to need more than the bindings can give.
