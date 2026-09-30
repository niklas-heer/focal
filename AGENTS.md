# AGENTS.md

Guidance for coding agents working in this repository.

## Project

Focal is a native macOS Markdown editor: iA Writer's minimal look with Bear-style live rendering, opened from the terminal. Read [README.md](README.md) for the goal and [docs/design.md](docs/design.md) for architecture, scope and milestones. Sections there are marked Agreed or Proposed; do not treat a proposal as settled.

**Current state:** design only; no application code, build tooling or tests exist on `main` yet. Milestone 0 is done: the spike chose TextKit 1, pending Niklas's acceptance (see [the findings](docs/spikes/2026-09-29-m0-textkit.md) and [the decision record](decisions/2026-09-30_194203231_build-the-editor-on-textkit-1.md)). The next step is the Milestone 1 implementation plan. Each milestone gets an implementation plan before work starts.

## Invariants

- **The file is the document.** Opening and saving an unedited file must produce identical bytes. Never normalize whitespace, line endings or Markdown style. The only permitted rewrite is an edited table, serialized with aligned columns.
- **Native first.** Swift and AppKit. The Mermaid renderer is the only web view, and it loads only when a document contains Mermaid.
- **Stable rendering.** Styling must not flicker, jump or disappear while scrolling or typing. This is where the earlier Electron prototype failed (history at commit `a72a464`).

## Stack (planned)

- Swift 6 with strict concurrency and AppKit, minimum macOS 26, built with the Xcode-provided toolchain.
- `focal-markdown`: a Rust library wrapping `pulldown-cmark` behind a C interface, linked statically.
- The Xcode project is generated from `project.yml` (XcodeGen). mise provides tool versions and tasks. When tooling lands, document `mise run build`, `mise run test` and `mise run check` here.
- CI stays on macOS runners, because AppKit and Xcode cannot run in Linux containers.

## Conventions

- Conventional commits, such as `feat(tables): …` or `fix(render): …`.
- Record lasting choices as vrdx decision records in `decisions/`: run `vrdx guide` first, create with `vrdx new "<title>" --body-file <file>`, and check with `vrdx validate`. Use `accepted` only for choices Niklas made.
- Update `docs/design.md` when a Proposed section becomes Agreed or changes.
- The iA Writer fonts are licensed under the SIL Open Font License 1.1 with the reserved name "iA Writer". Bundle them unmodified, with their license.
