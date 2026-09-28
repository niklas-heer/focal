+++
schema_version = 1
id = "01M3N1CC76F8J8BK28GQTPV63M"
title = "Parse Markdown with pulldown-cmark through a Rust library"
date = "2026-09-28"
status = "accepted"
tags = ["architecture", "parsing"]
supersedes = []
superseded_by = []
depends_on = []
related_to = ["01M3N1CC6YF89Y8YAY1QJW5ZWS"]
+++
## Decision

Parse Markdown with `pulldown-cmark` (0.13 at the time of this decision) inside a small Rust library, `focal-markdown`. It is exposed to Swift through a C interface and linked statically into the app. It returns a flat array of spans with byte ranges, content ranges and attributes, which Swift converts to UTF-16 offsets for styling.

## Context

Focal needs one parser that covers every agreed feature and reports exact source positions for each element, so styling can hide and reveal the right characters. Checked on 2026-09-29:

- `pulldown-cmark` natively supports tables, footnotes, strikethrough, task lists, YAML metadata blocks, `$`/`$$` math, GitHub alerts (`BlockQuoteKind`), wiki links and `==highlight==`. `into_offset_iter()` gives the byte range of every event.
- `tree-sitter-markdown` is incremental and has math, wiki-link and metadata extensions, but no footnotes. Its README says it has "lots of inaccuracies" and is "not recommended … where correctness is important"; its goal is syntax highlighting.
- Apple's `swift-cmark` (cmark-gfm) is GitHub's own parser and covers core GFM, but math, wiki links and alerts would need custom syntax extensions written in C, and inline source positions have historically been less precise.

Niklas's tooling defaults to Rust, and the parser is the part most worth testing thoroughly against the CommonMark and GFM spec examples.

## Consequences

- The build combines Cargo and Xcode: the Rust static library is built for each architecture before the Swift targets, and mise tasks must drive both.
- The whole document is reparsed after each edit. Milestone 0 measures whether this meets the keystroke budget; if not, reparse from the edited block onward.
- Rendering follows `pulldown-cmark`'s CommonMark interpretation, which can differ from github.com in rare edge cases.
- Revisit if the C interface's cost or complexity outweighs the parser's coverage, or if a Swift parser reaches the same extension coverage with exact ranges.
