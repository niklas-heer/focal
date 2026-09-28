# Focal

A focused, native Markdown editor for macOS that you open from the terminal.

> **Status:** design phase. There is no application code yet. The next step is Milestone 0, a throwaway spike that settles the text engine. See [the design](docs/design.md).

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
- [Decision records](decisions/): lasting choices and why they were made (managed with [vrdx](https://github.com/niklas-heer/vrdx)).
- [AGENTS.md](AGENTS.md): guidance for coding agents.

## History

An earlier Focal was an Electron, React and CodeMirror prototype. It stalled on rendering that broke while scrolling and never hid Markdown syntax reliably. It survives in Git history at commit `a72a464`.

## License

[MIT](LICENSE). The iA Writer fonts are licensed separately under the SIL Open Font License 1.1.
