# AGENTS.md

Guidance for coding agents working in this repository.

## Project

Focal is a native macOS Markdown editor: iA Writer's minimal look with Bear-style live rendering, opened from the terminal. Read [README.md](README.md) for the goal and [docs/design.md](docs/design.md) for architecture, scope and milestones. Sections there are marked Agreed or Proposed; do not treat a proposal as settled.

**Current state:** on `main`, design only; the next step there is Milestone 0, a throwaway spike comparing TextKit 1 and TextKit 2 (design section 4). Each milestone gets an implementation plan before work starts.

**This branch (`rust-gpui`)** holds a working Rust and GPUI spike of the same app, an alternative under evaluation (see [docs/gpui-spike.md](docs/gpui-spike.md) and the proposed decision record). On this branch, the "Native first" invariant means native Rust with GPUI rather than Swift and AppKit, and the planned stack below does not apply.

## Invariants

- **The file is the document.** Opening and saving an unedited file must produce identical bytes. Never normalize whitespace, line endings or Markdown style. The only permitted rewrite is an edited table, serialized with aligned columns.
- **Native first.** Swift and AppKit. The Mermaid renderer is the only web view, and it loads only when a document contains Mermaid.
- **Stable rendering.** Styling must not flicker, jump or disappear while scrolling or typing. This is where the earlier Electron prototype failed (history at commit `a72a464`).

## Stack (planned)

- Swift 6 with strict concurrency and AppKit, minimum macOS 26, built with the Xcode-provided toolchain.
- `focal-markdown`: a Rust library wrapping `pulldown-cmark` behind a C interface, linked statically.
- The Xcode project is generated from `project.yml` (XcodeGen). mise provides tool versions and tasks. When tooling lands, document `mise run build`, `mise run test` and `mise run check` here.
- CI stays on macOS runners, because AppKit and Xcode cannot run in Linux containers.

## Stack on this branch

- Rust (pinned in `rust-toolchain.toml`), a Cargo workspace:
  - `crates/focal-core`: the UI-free editor model. Markdown analysis with `pulldown-cmark`, line display maps, the buffer with undo, Markdown-aware edits. Put logic here and unit-test it.
  - `crates/focal`: the GPUI app through `gpui-kit` (pinned exactly, it pins `gpui-pre`). Keep it to rendering, input and files.
- Commands: `mise run build`, `mise run run -- FILE`, `mise run test`, `mise run check` (fmt, Clippy with `-D warnings`, tests), `mise run stress`, `mise run clean`.
- `FOCAL_TRACE=1` prints timings and the caret per change. `FOCAL_FOREGROUND=1` keeps `focal` in the terminal like `--wait`.
- GPUI pitfalls found so far are listed in [docs/gpui-spike.md](docs/gpui-spike.md); absolutely positioned elements need explicit `top_0()`/`left_0()`.
- Debug builds are large (several GB); run `mise run clean` when done.

## Conventions

- Conventional commits, such as `feat(tables): …` or `fix(render): …`.
- Record lasting choices as vrdx decision records in `decisions/`: run `vrdx guide` first, create with `vrdx new "<title>" --body-file <file>`, and check with `vrdx validate`. Use `accepted` only for choices Niklas made.
- Update `docs/design.md` when a Proposed section becomes Agreed or changes.
- The iA Writer fonts are licensed under the SIL Open Font License 1.1 with the reserved name "iA Writer". Bundle them unmodified, with their license (`assets/fonts/LICENSE.md` on this branch).
