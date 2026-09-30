+++
schema_version = 1
id = "01M3T85F3W9NFB7RZBP9FPV23R"
title = "Update Focal with Sparkle through objc2"
date = "2026-09-30"
status = "proposed"
tags = ["distribution", "updates"]
supersedes = []
superseded_by = []
depends_on = []
related_to = []
+++
## Decision

Focal updates itself with Sparkle 2, the updater Keywink and Spokn use. Release builds carry `Sparkle.framework` in `Contents/Frameworks` and the feed `https://github.com/niklas-heer/focal/releases/latest/download/appcast.xml` with the shared Sparkle public key in `Info.plist`. At launch Focal loads the framework through `objc2` and creates Sparkle's `SPUStandardUpdaterController`; Focal's "Check for updates automatically" setting drives the updater's automatic checks, and the menu item "Check for Updates…" asks for a check.

Calling Sparkle's Objective-C API from Rust needs `unsafe` (`msg_send!` and `NSBundle::load`). The workspace lint moves from `unsafe_code = "forbid"` to `"deny"`, and only `crates/focal/src/updates.rs` allows it, with a `SAFETY` comment on every block.

Proposed on 2026-10-01 while building Milestone 6; Niklas asked for self-updates with a setting and a prompt ("Hey, there's a new update. Do you want to download it?") but did not choose the mechanism.

## Context

- Sparkle's standard prompt asks before downloading ("A new version of Focal is available! … Would you like to download it now?") and offers release notes and "Skip This Version", which covers the agreed behavior (design section 3, Updates).
- The same appcast format, key and GitHub Releases hosting already work for Keywink and Spokn (hub fact `sparkle-updates-from-github-releases`), and Homebrew's `livecheck` understands it.
- Sparkle verifies the EdDSA signature of every update and installs it with its own helper, including administrator authorization when the app folder needs it.

Alternatives considered:

- **A Swift helper app that links Sparkle** and updates `Focal.app` from outside. No `unsafe` in Rust, but a second toolchain in the build, a second process and bundle to sign, and Sparkle's "update another bundle" mode.
- **An updater written in Rust** (fetch the appcast, verify the Ed25519 signature, replace the bundle, relaunch), as Zed does. No `unsafe`, but it reimplements security-sensitive installation code that Sparkle already gets right.
- **No self-updates; Homebrew only.** Rejected: Niklas asked for updates inside the app.

## Consequences

- One module holds a few lines of `unsafe` Objective-C messaging; a mistake there can crash Focal at launch, so it only runs when the bundle carries a feed and the framework, and every call is checked for `nil`.
- Development builds (`cargo run`, ad hoc bundles without a feed) have no updater; their menu offers "Focal Releases…" instead.
- Releases must attach `appcast.xml`, or installed copies stop seeing updates; the Sparkle private key must stay in the release Mac's Keychain (and a backup), as for Keywink and Spokn.
