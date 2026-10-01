+++
schema_version = 1
id = "01M3V7ZEKYBDB9KVSFQE9DRM3Z"
title = "Render Mermaid natively with merman"
date = "2026-10-01"
status = "proposed"
tags = ["rendering", "mermaid"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Focal draws Mermaid diagrams natively with `merman` 0.7.0, a headless Rust implementation of Mermaid (parsing, layout and SVG) that targets Mermaid 11.15. A fenced `mermaid` block is an island: its SVG, themed from Focal's palette for the current appearance through `merman`'s host theme and its resvg-safe output, is drawn by GPUI and cached by source and palette; rendering runs on a background thread. The design's proposed hidden `WKWebView` is not needed.

Proposed on 2026-10-01 while building Milestone 5; the design marked the implementation as proposed and Niklas has not chosen.

## Context

The principles allow one exception to "no web views": an isolated Mermaid renderer. A spike on an Apple M4 compared two native renderers on a flowchart and a sequence diagram, rasterized with resvg as GPUI does:

- `merman` 0.7.0: layout matching Mermaid's, Focal's colors applied, about 8 ms for the first diagram and under 1 ms after, and parse errors with a position.
- `mermaid-rs-renderer` 0.3.1: fast after a 190 ms font load, but crossing edges in the flowchart and its own fixed colors.

A web view would have meant `objc2-web-kit`, more `unsafe` code, a hidden window and asynchronous JavaScript round trips.

## Consequences

- No web view or JavaScript is involved; the Mermaid exception in the principles stays unused.
- Diagram fidelity depends on `merman`'s parity with Mermaid; diagram types it does not support show its error message instead of a picture.
- Labels are drawn with system fonts through resvg, measured by `merman`'s bundled text metrics.
