# AGENTS.md

Guidance for coding agents working in this repository.

## Project

Focal is a native macOS Markdown editor: iA Writer's minimal look with Bear-style live rendering, opened from the terminal. Read [README.md](README.md) for the goal and [docs/design.md](docs/design.md) for architecture, scope and milestones. Sections there are marked Agreed or Proposed; do not treat a proposal as settled.

**Current state:** Focal is built in Rust with GPUI ([decision](decisions/2026-09-30_190238170_build-focal-in-rust-with-gpui.md), accepted 2026-09-30). The code started as a spike ([findings](docs/gpui-spike.md)) and is now the product; Milestones 1 to 3 (core editor, tables, chrome) are done. Each milestone gets an implementation plan in `docs/superpowers/plans/` before work starts.

## Invariants

- **The file is the document.** Opening and saving an unedited file must produce identical bytes. Never normalize whitespace, line endings or Markdown style. The only permitted rewrite is an edited table, serialized with aligned columns.
- **Native first.** Native Rust with GPUI, using macOS services (spell checking, and later others) through `objc2` bindings where GPUI has none. No web views. (The principles allow an isolated Mermaid renderer, but Mermaid now renders natively with `merman`; math uses MathJax in an embedded QuickJS, not a web view.)
- **Stable rendering.** Styling must not flicker, jump or disappear while scrolling or typing. This is where the earlier Electron prototype failed (history at commit `a72a464`).
- **Accessible.** The editor's accessibility tree carries the rendered text, the selection and tables. Keep it in step with rendering changes.

## Stack

- Rust (pinned in `rust-toolchain.toml`), a Cargo workspace:
  - `crates/focal-core`: the UI-free editor model. Markdown analysis with `pulldown-cmark`, line display maps, the buffer with undo, Markdown-aware edits, accessibility text chunks. Put logic here and unit-test it.
  - `crates/focal`: the GPUI app through `gpui-kit` (pinned exactly; it pins `gpui-pre`). The window's root is a `Workspace` (`workspace.rs`) holding the `Editor` (`editor.rs`: text, caret, rendering), the bottom bar (`bar.rs`), folder mode (`folder.rs`, sidebar) and the quick switcher (`switcher.rs`). Tables are drawn in `table_view.rs` and edited through `grid.rs`; settings (`settings.rs`), menus (`menus.rs`), spelling, grammar and correction (`spell.rs`), printing and PDF (`print.rs`, from `export.rs`'s page for paper) and accessibility (`accessibility.rs`) have their own modules. The bar, menus and keys dispatch the same editor actions.
- Commands: `mise run build`, `mise run run -- FILE`, `mise run test`, `mise run check` (fmt, Clippy with `-D warnings`, tests), `mise run stress`, `mise run bundle` (ad hoc `build/Focal.app`), `mise run install-app`, `mise run release` (see [RELEASE.md](RELEASE.md)), `mise run clean`.
- Packaging lives in `packaging/` (`Info.plist`, the icon and its script, the Homebrew cask template) and `scripts/` (`bundle`, `sparkle`, `release`). Notarization and publishing a release are manual steps; never run `scripts/release --notarize` or `gh release create` unasked.
- `unsafe` code is denied everywhere except the macOS bridges named in the decision "Allow unsafe code only in named macOS bridges": `updates.rs` (Sparkle), `print.rs` (WebKit printing) and `SpellChecker::check` in `spell.rs`. Other macOS calls use `objc2`'s safe methods (`mac.rs`). A new bridge updates that decision.
- `FOCAL_TRACE=1` prints timings and the caret per change. `FOCAL_FOREGROUND=1` keeps `focal` in the terminal like `--wait`. `FOCAL_APPEARANCE=light` or `dark` overrides the system appearance for Focal alone, to check both palettes.
- GPUI pitfalls found so far are listed in [docs/gpui-spike.md](docs/gpui-spike.md); absolutely positioned elements need explicit `top_0()`/`left_0()`.
- Debug builds are large (several GB); run `mise run clean` when done.
- CI (`.github/workflows/check.yml`) runs `mise run check` on a macOS runner, because the app links AppKit. Dependabot proposes updates weekly; when one fails the check, `fix-dependency-update.yml` has Claude adapt the code on its branch (once the secret `CLAUDE_CODE_OAUTH_TOKEN` and the variable `CLAUDE_FIX_ENABLED=true` are set). Review those commits like any other.

## Conventions

- Conventional commits, such as `feat(tables): …` or `fix(render): …`.
- Record lasting choices as vrdx decision records in `decisions/`: run `vrdx guide` first, create with `vrdx new "<title>" --body-file <file>`, and check with `vrdx validate`. Use `accepted` only for choices Niklas made.
- Update `docs/design.md` when a Proposed section becomes Agreed or changes.
- The iA Writer fonts are licensed under the SIL Open Font License 1.1 with the reserved name "iA Writer". Bundle them unmodified, with their license (`assets/fonts/LICENSE.md`).
