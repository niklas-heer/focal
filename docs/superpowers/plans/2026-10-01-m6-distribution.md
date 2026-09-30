# Milestone 6 — Distribution Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Focal ships as a signed, notarized `Focal.app` that updates itself (with a setting and a download prompt), installs its own `focal` command, keeps one running instance that `focal` forwards files to, and opens Markdown files from Finder.

**Architecture:** A bundle script assembles `Focal.app` around the release binary (`Contents/MacOS/focal`), an `Info.plist`, an icon, and Sparkle 2.10.0 (downloaded once, checksum pinned) in `Contents/Frameworks`. The `focal` command is the app binary itself: when an instance is running, it hands its request to that instance over a Unix socket and exits, or waits for the window to close with `--wait`. Updates use Sparkle's `SPUStandardUpdaterController`, loaded at runtime through `objc2`; its standard prompt asks before downloading, and Focal's setting drives Sparkle's automatic checks. The command-line installer links `/usr/local/bin/focal` to the bundle's binary, asking for an administrator password only when needed. Releases reuse the Keywink/Spokn pipeline: Developer ID signing inside out, `notarytool` with the `keywink-notary` profile, a ZIP, and a one-item appcast signed with the shared Sparkle key and attached to the GitHub release.

**Tech Stack:** Rust, GPUI (`Application::on_open_urls`, multiple windows), `objc2` 0.6 / `objc2-app-kit` / `objc2-foundation` (NSBundle, NSApplication), Sparkle 2.10.0, `codesign`, `notarytool`, `stapler`, Sparkle's `generate_appcast`, mise tasks.

**Spec:** [`docs/design.md`](../../design.md) section 3 (command line, the `focal` command, updates), section 6 (settings), section 8 (M6).

## Global Constraints

- The file is the document: forwarding, multiple windows and updates never change a file's bytes.
- Updates: a setting turns automatic checks on or off; when a newer version exists Focal asks before downloading and never installs without consent (design 3, Agreed).
- Command-line tool: a menu item puts `focal` on `PATH`; an administrator password is asked only when `/usr/local/bin` is not writable; an uninstall counterpart exists (design 3).
- `focal file.md` for a file already open brings its window to the front; `focal --wait` blocks until that window closes, also when forwarded (design 3 table).
- Signing and publishing reuse the existing setup: "Developer ID Application: Niklas Heer (WL5ASJ82YX)", notary profile `keywink-notary`, Sparkle key account `ed25519` (public key `SL638YLPjJQuUkrCa0p26d1nfMoa69+ymt0mzNQDscA=`), feed `https://github.com/niklas-heer/focal/releases/latest/download/appcast.xml`.
- Notarization submits the app to Apple and a GitHub release publishes it: both stay manual, confirmed steps (`RELEASE.md`); scripts prepare everything up to them.
- `unsafe` code stays forbidden except in `updates.rs`, which calls Sparkle's Objective-C API; each `unsafe` block has a `SAFETY` comment (decision record, proposed).
- `mise run check` passes after every task; logic has unit tests; UI behavior has headless UI tests.
- Work on branch `m6-distribution` in `.worktrees/rust-gpui`; conventional commits with the session's `Co-Authored-By` line; `mise run clean` when done.

## Review Focus

1. **A second `focal` while the first instance is starting or has crashed**: a stale socket file must not make `focal` hang or fail; it starts a new instance and replaces the socket. Task 2 tests a stale socket.
2. **`focal --wait` forwarded to a running instance, then the window closes by quitting the app**: the waiting command returns instead of hanging. Task 2 tests the connection closing.
3. **Installing the command when `/usr/local/bin` is missing or owned by root**: the link is made with one administrator prompt, and an existing `focal` that is not Focal's link is not overwritten without saying so. Task 3 tests the unwritable and foreign-file cases.
4. **Running the plain `cargo run` binary (no bundle)**: no Sparkle, no crash; the menu offers "Focal Releases…" instead of "Check for Updates…". Task 4 tests the unbundled case.
5. **Finder opens several files at once, or a folder**: each gets its own window, a folder opens in folder mode. Task 2 tests multiple paths.

---

### Task 1: The app bundle

**Files:** Create `packaging/Info.plist`, `packaging/AppIcon.icns` (rendered by `packaging/make-icon.swift`), `scripts/bundle` (bash), `scripts/sparkle` (download and verify Sparkle 2.10.0 into `build/sparkle`); modify `crates/focal/Cargo.toml` (version 0.1.0), `mise.toml` (`bundle`, `install-app` tasks), `.gitignore` (`build/`).

- `Info.plist`: `CFBundleIdentifier` `com.niklasheer.focal`, `CFBundleExecutable` `focal`, `CFBundleName`/`CFBundleDisplayName` Focal, version placeholders filled by the script from `Cargo.toml` and a `FOCAL_BUILD` number, `LSMinimumSystemVersion` 14.0, `NSHighResolutionCapable`, `LSApplicationCategoryType` productivity, `CFBundleDocumentTypes` for Markdown (`net.daringfireball.markdown`, extensions md/markdown), `SUEnableAutomaticChecks` true (Focal's own setting replaces Sparkle's first-run question).
- `scripts/bundle [--sign IDENTITY] [--feed URL --public-key KEY]`: `cargo build --release -p focal`, assemble `build/Focal.app`, copy Sparkle, add the `@executable_path/../Frameworks` rpath (not needed for dlopen but harmless: skipped), sign inside out with the identity or ad hoc, `codesign --verify --deep --strict`.
- [ ] Step 1: write the files; Step 2: `mise run bundle`, then `plutil -lint`, `codesign --verify`, and launching `build/Focal.app/Contents/MacOS/focal examples/showcase.md` opens a window; Step 3: commit `feat(dist): assemble Focal.app with Sparkle and an icon`.

### Task 2: One running instance, forwarding and Finder

**Files:** Create `crates/focal/src/instance.rs`; modify `crates/focal/src/main.rs`, `crates/focal/src/workspace.rs` (window per request, find window by path).

- Socket at `$TMPDIR/focal-<uid>.sock`. Request: one JSON line `{"path": Option<String>, "untitled": Option<String>, "wait": bool}`; the instance answers `opened\n` and, for `wait`, `closed\n` when that window closes (the connection closing also ends the wait).
- `focal` (the detaching launcher) first tries to connect; on success it sends the request (and blocks for `wait`); on failure (no socket, or a stale one that refuses connections) it starts the app as today, which removes a stale socket and listens.
- The app opens a window per request, or activates the window already showing that file; folder paths open folder mode. `Application::on_open_urls` feeds Finder's file URLs into the same path.
- Tests: request round trip; a stale socket file is replaced; a client waiting on a window that closes returns; several paths open several windows (UI test with two workspaces).
- Commit `feat(dist): one running instance that focal forwards files to`.

### Task 3: Install Command Line Tool

**Files:** Create `crates/focal/src/cli_install.rs`; modify `crates/focal/src/menus.rs`.

- `install(link_dir, target) -> Result<Installed>`: when `link_dir/focal` is missing or already a link to a Focal binary, create or replace the link; a foreign file is refused with an error naming it. When `link_dir` is not writable (or missing), run one `osascript` "do shell script … with administrator privileges" that makes the folder and the link. `uninstall(link_dir)` removes the link only if it points into a `Focal.app`.
- Menu items "Install Command Line Tool…" and "Uninstall Command Line Tool…" (Focal menu), with an alert reporting the result; only in a bundled app (the binary's path contains `Focal.app/Contents/MacOS`).
- Tests (temp dirs): fresh install, reinstall over an old link, a foreign file refused, uninstall leaves foreign files alone.
- Commit `feat(dist): install and uninstall the focal command`.

### Task 4: Updates through Sparkle

**Files:** Create `crates/focal/src/updates.rs`, `decisions/…sparkle-through-objc2.md` (proposed); modify `Cargo.toml` (`unsafe_code = "deny"` with the one module allowed), `crates/focal/src/settings.rs` (`check_updates: bool`, default true), `crates/focal/src/menus.rs`, `crates/focal/src/main.rs`.

- `Updater::start(cx)`: when running from a bundle whose `Info.plist` has `SUFeedURL` and `Contents/Frameworks/Sparkle.framework` exists, load the framework with `NSBundle`, create `SPUStandardUpdaterController` (starting the updater) and keep it for the app's life; set `updater.automaticallyChecksForUpdates` from the setting at start and on every change. Otherwise `None`.
- Menu: "Check for Updates…" calls `checkForUpdates:` (Sparkle's prompt: "A new version of Focal is available… Would you like to download it now?"); without an updater the item reads "Focal Releases…" and opens the releases page. "About Focal" shows the standard about panel.
- Settings window: a "Check for updates automatically" switch.
- Tests: settings default and round trip; the unbundled case yields no updater and the releases menu item; manual end-to-end check with two locally signed builds and a local appcast.
- Commit `feat(dist): updates through Sparkle with a setting`.

### Task 5: Release process and documentation

**Files:** Create `scripts/release`, `RELEASE.md`, `packaging/focal.rb` (cask template); modify `README.md` (install), `docs/design.md` (sections 3 and 8), `AGENTS.md`, `mise.toml` (`release`).

- `scripts/release`: refuses a dirty tree; bundles with the Developer ID and feed; zips; notarizes (`--notarize` flag, off by default so a dry run never uploads), staples, zips again; writes `appcast.xml` with `generate_appcast --account ed25519` and verifies it with `sign_update --verify`; prints the `gh release create` command.
- Cask `focal` (`auto_updates true`, `binary "#{appdir}/Focal.app/Contents/MacOS/focal"`, livecheck `:sparkle`), to be added to `niklas-heer/homebrew-tap` by pull request after the first release.
- Run `scripts/release` without `--notarize` (a signed dry run), `mise run check`, commit `docs: record the release process`, `mise run clean`.
