+++
schema_version = 1
id = "01M3WG5NPAHN0J0ZV95T94QM9Y"
title = "Allow unsafe code only in named macOS bridges"
date = "2026-10-01"
status = "proposed"
tags = ["architecture", "safety"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

The workspace keeps `unsafe_code = "deny"`. Code may allow `unsafe` only where it calls a macOS API that `objc2` marks `unsafe` and that has no safe binding, and only in these named places, each with a `SAFETY` comment on every block:

- `crates/focal/src/updates.rs` (module): Sparkle, loaded at runtime.
- `crates/focal/src/print.rs` (module): WebKit and AppKit printing.
- `SpellChecker::check` in `crates/focal/src/spell.rs` (one function): `NSSpellChecker`'s `checkString:range:types:…`, which grammar, spelling and correction share.

Adding a place means updating this record.

Proposed on 2026-10-01 while building Milestone 12. It widens the rule in "Update Focal with Sparkle through objc2", which named `updates.rs` as the only exception.

## Context

- `objc2` generates bindings from Apple's headers and marks a method `unsafe` when its safety has not been reviewed: all of WebKit, and AppKit methods taking raw pointers or untyped dictionaries.
- Grammar checking needs `checkString:…` (the older `checkGrammarOfString:` returns nothing on current macOS); it is also what checks spelling in the language a sentence is written in, which per-word checks get wrong.
- Printing a laid-out page needs `WKWebView` (see the decision on printing through WebKit).

Alternatives considered:

- **A small Swift or Objective-C shim** compiled into the app: the `unsafe` moves into another language and a second toolchain joins the build, without being any safer.
- **Allowing `unsafe` crate-wide**: simpler, but loses the review point that a new `#[allow(unsafe_code)]` creates.

## Consequences

- A new bridge needs a `#[allow(unsafe_code)]` that review sees, and an update here.
- Each bridge is small and tested where the platform allows (spelling and grammar are; WebKit printing is checked by hand).
