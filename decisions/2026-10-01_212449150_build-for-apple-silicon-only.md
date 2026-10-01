+++
schema_version = 1
id = "01M3WNP3BYJGJMGXVZ6DDH7QQ7"
title = "Build for Apple Silicon only"
date = "2026-10-01"
status = "accepted"
tags = ["distribution"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Focal is built for Apple Silicon only. Releases contain an `arm64` binary, and the Homebrew cask says `depends_on arch: :arm64`; there are no universal or Intel builds.

Niklas decided this on 2026-10-01: "Intel: no, I don't think we need them. They are phased out."

## Context

- Apple stopped selling Intel Macs in 2023, and macOS 26 is the last release that supports them.
- A universal binary roughly doubles the app's size (GPUI, tree-sitter grammars, QuickJS and the diagram renderers are all native code) and the release time, and needs Intel testing nobody does.

## Consequences

- `scripts/bundle` (which `scripts/release` uses) builds for the release Mac itself, `aarch64-apple-darwin`.
- Intel Mac users cannot run Focal; Homebrew refuses the cask there instead of installing something that cannot start.
