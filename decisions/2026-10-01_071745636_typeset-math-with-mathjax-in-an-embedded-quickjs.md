+++
schema_version = 1
id = "01M3V57314GE9TGXBX0KETEAM7"
title = "Typeset math with MathJax in an embedded QuickJS"
date = "2026-10-01"
status = "accepted"
tags = ["rendering", "math"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Focal typesets display math (`$$ … $$` blocks) with MathJax 3.2.2, TeX input and SVG output, running in an embedded QuickJS engine (`rquickjs`). The bundle is built from a pinned `mathjax-full` by `scripts/mathjax` and vendored as `assets/mathjax/mathjax.js` (1.8 MB, Apache License 2.0). One worker thread owns the engine and loads MathJax on first use; GPUI draws the resulting SVG, colored with the text color and rasterized at three times its drawn size. Inline math stays styled source within its line.

Proposed on 2026-10-01 while building Milestone 4; the design left the renderer open (section 10) and Niklas has not chosen.

Accepted by Niklas on 2026-10-02: "yes to mathjax".

## Context

The design requires native rendering without web views, except an isolated renderer for Mermaid. A spike measured MathJax in QuickJS on an Apple M4: loading the bundle takes about 67 ms once; a formula then takes 1 to 3 ms; invalid TeX returns MathJax's message ("Missing close brace").

Alternatives considered:

- **Typst with `mitex`** (LaTeX to Typst math): pure Rust and excellent typesetting, but `mitex` was last released in 2024 for an older Typst and adds Typst's whole compiler; LaTeX coverage depends on the translation.
- **A pure-Rust LaTeX math renderer**: none maintained exists (`ratex` is a placeholder; ReX is abandoned).
- **MathML through `pulldown-latex`**: macOS renders MathML only in WebKit.
- **MathJax 4**: newer fonts, but it loads font data dynamically, which a bundled QuickJS script cannot do simply.

## Consequences

- The binary grows by about 2 MB; MathJax's license ships in the app bundle.
- Upgrading MathJax means changing `packaging/mathjax/package.json` and running `scripts/mathjax` (needs bun).
- Inline math cannot be typeset inside a line: GPUI text runs cannot hold an image.
