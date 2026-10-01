# Focal

A focused, native Markdown editor for macOS that you open from the terminal.

> **Status:** early. Focal is built in Rust with [GPUI](https://gpui.rs), Zed's GPU-accelerated UI framework. Milestone 1 is done: lists, checkboxes and quotes are drawn and edited properly, files reload live, code blocks are highlighted and VoiceOver can read the document. Milestone 2 made tables grids you edit in place. Milestone 3 added the chrome: the bottom formatting bar, focus mode with typewriter scrolling, folder mode with a sidebar and a quick switcher, settings and a menu bar. Milestone 6 made it an app: `Focal.app` installs its own `focal` command, keeps one instance that `focal` hands files to, opens Markdown files from Finder, and updates itself through Sparkle. Version 0.1.0 is released, also as a Homebrew cask (`brew install --cask niklas-heer/tap/focal`). Milestone 4 added the Markdown extras: images, typeset display math, collapsed front matter, footnote previews, and wiki links you can follow. Milestone 5 draws Mermaid diagrams natively, in Focal's colors. Milestone 7 added the everyday commands: find and replace, New, Open… and Open Recent, and a question before untitled text is lost. Milestone 8 added Go to Heading and HTML export. See [the design](docs/design.md).

## Try it

Requires macOS, [mise](https://mise.jdx.dev) and rustup (the toolchain is pinned in `rust-toolchain.toml`).

```sh
mise run install-app                   # build Focal.app and copy it to /Applications
                                       # then Focal › Install Command Line Tool… puts `focal` on PATH
focal notes.md                         # open a file from any terminal
mise run run -- examples/showcase.md   # run a development build and wait for the window to close
mise run check                         # formatting, Clippy and tests
mise run stress                        # timings on a 5,000-line document
```

Set `FOCAL_TRACE=1` to print parse and restyle times for every change.

| Shortcut | Does |
| --- | --- |
| ⌘N, ⌘O | New window, open a file or folder (File ▸ Open Recent lists recent ones) |
| ⌘F, ⌥⌘F | Find, find and replace |
| ⌘G, ⇧⌘G | Next match, previous match (also Return and ⇧Return in the find bar) |
| ⇧⌘O | Go to a heading |
| ⇧⌘E, ⌥⇧⌘C | Export as HTML, copy as HTML |
| ⌘B, ⌘I, ⇧⌘X, ⌘E | Bold, italic, strikethrough, inline code |
| ⌘K | Link |
| ⌘-click, ⌘↩ | Follow a link, wiki link or footnote |
| ⌘[ | Back |
| ⌘1 to ⌘6, ⌘0 | Heading level, paragraph |
| ⌘D | Focus mode |
| ⌃⌘S | Sidebar (folder mode) |
| ⌘P | Quick switcher (folder mode) |
| ⌘, | Settings |

Lists, tasks, quotes, tables, code and math blocks are in the Format menu and the bottom bar.

## The goal

Focal combines two things:

- **The calm of [iA Writer](https://ia.net/writer).** One quiet column of text, good typography (the iA Writer fonts) and nothing competing for attention.
- **The live rendering of [Bear](https://bear.app).** Markdown looks the way it would print. Click into a bold word or a heading and its Markdown syntax appears so you can edit it; move away and it disappears again.

It should be the best Markdown editor on the Mac for reading and writing plain `.md` files, especially files that live in Git repositories or that agents write.

```sh
focal notes.md          # open (or create) one file
focal .                 # open a folder and move between its files
cat draft.md | focal    # open piped text as a new document
focal --wait msg.md     # block until closed, usable as $EDITOR
```

## What it will do

- **Exact round-trip.** The file on disk is the document. Focal restyles Markdown on screen and never reformats your text; the only exception is a table you edited in the grid, which is saved with aligned columns.
- **Full GitHub-flavored Markdown**, including tables, task lists, strikethrough, autolinks, footnotes and syntax-highlighted code blocks.
- **Extras:** GitHub alerts (`> [!NOTE]`), YAML front matter, `$…$`/`$$…$$` math, Mermaid diagrams, `[[wiki links]]` and `==highlights==`.
- **Real tables.** Every table appears as a grid you edit in place: Tab between cells, add, delete or drag rows and columns, and set alignment.
- **Files and folders.** One window per file, or a folder with a hideable file list and a ⌘P quick switcher.
- **Live reload.** When an agent, Git or another editor changes the file, Focal reloads it. Edits save automatically.
- **Light and dark mode**, following the system.
- **Focus mode** (off by default): dims everything but the current sentence or paragraph and keeps your line centered.
- **A bottom formatting bar** that fades in only when the pointer reaches the bottom edge.

## Documentation

- [Design](docs/design.md): goals, architecture and milestones.
- [GPUI spike findings](docs/gpui-spike.md) and [Milestone 0 TextKit findings](docs/spikes/2026-09-29-m0-textkit.md): how the stack was chosen.
- [Decision records](decisions/): lasting choices and why they were made (managed with [vrdx](https://github.com/niklas-heer/vrdx)).
- [AGENTS.md](AGENTS.md): guidance for coding agents.

## History

An earlier Focal was an Electron, React and CodeMirror prototype. It stalled on rendering that broke while scrolling and never hid Markdown syntax reliably. It survives in Git history at commit `a72a464`.

## License

[MIT](LICENSE). The iA Writer fonts are licensed separately under the SIL Open Font License 1.1.
