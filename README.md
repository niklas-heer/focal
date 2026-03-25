# focal

A focused, distraction-free Markdown editor for the desktop. Built with Electron, React, and CodeMirror — with inline WYSIWYG rendering so your document always looks clean while you write.

![Electron](https://img.shields.io/badge/Electron-41-47848F?logo=electron&logoColor=white)
![React](https://img.shields.io/badge/React-19-61DAFB?logo=react&logoColor=black)
![TypeScript](https://img.shields.io/badge/TypeScript-6-3178C6?logo=typescript&logoColor=white)
![License](https://img.shields.io/badge/license-MIT-green)

---

## Features

- **WYSIWYG Markdown** — headings, bold, italic, links, and code blocks are rendered inline as you type; raw syntax is hidden until your cursor enters the span
- **Folder-based workspace** — open any folder and browse `.md`/`.markdown` files in a collapsible file tree
- **Outline panel** — live heading hierarchy with one-click jump-to-heading
- **Focus mode** — hides the sidebar and dims everything except the current paragraph
- **Command palette** — fuzzy-search all commands via `⌘K`
- **Auto-save** — debounced write-to-disk 500 ms after the last keystroke
- **Markdown linter** — real-time lint diagnostics via `markdownlint` with a one-click auto-fix button in the status bar
- **File tree actions** — rename, delete, and drag-and-drop files between folders via context menu
- **CLI open** — pass a folder path as a CLI argument to open it directly on launch

---

## Requirements

- **Node.js** ≥ 18 and **npm** ≥ 9
- **macOS** (the window chrome uses `hiddenInset` title bar; Linux/Windows work but may look different)

---

## Getting Started

```bash
# 1. Clone the repo
git clone git@github.com:niklas-heer/focal.git
cd focal

# 2. Install dependencies
npm install

# 3. Start in development mode (Vite + Electron, hot-reload)
npm run dev
```

The app opens automatically once the Vite dev server is ready on `localhost:5173`.

---

## Building for Production

```bash
# Compile the TypeScript main process and bundle the renderer
npm run build

# The compiled output lands in:
#   dist/          — renderer (Vite bundle)
#   electron-dist/ — main process + preload (tsc)
```

To package a distributable app (`.dmg`, `.exe`, etc.) using `electron-builder`:

```bash
npx electron-builder
```

---

## Project Structure

```
focal/
├── electron/
│   ├── main.ts        # Electron main process — window, IPC handlers, file I/O
│   └── preload.ts     # Context bridge — exposes electronAPI to the renderer
├── src/
│   ├── components/
│   │   ├── Editor.tsx         # CodeMirror 6 editor with WYSIWYG decorations
│   │   ├── FileTree.tsx       # Sidebar file browser with drag-and-drop
│   │   ├── Outline.tsx        # Heading hierarchy panel
│   │   ├── StatusBar.tsx      # Bottom bar (cursor, word count, lint, save status)
│   │   └── CommandPalette.tsx # ⌘K fuzzy command palette
│   ├── lib/
│   │   ├── markdownDecorations.ts # CodeMirror decoration plugin
│   │   ├── outline.ts             # Heading extraction from Markdown AST
│   │   ├── linter.ts              # markdownlint integration + auto-fix
│   │   └── theme.ts               # CodeMirror dark theme
│   ├── App.tsx        # Root component — layout, state, keyboard shortcuts
│   ├── types.ts       # Shared TypeScript types (FileNode, etc.)
│   └── index.css      # Global styles
├── vite.config.ts
├── tsconfig.json            # Renderer TypeScript config
└── tsconfig.electron.json   # Main process TypeScript config
```

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `⌘K` | Open command palette |
| `⌘\` | Toggle sidebar |
| `⌘⇧F` | Toggle focus mode |
| `Escape` | Close command palette |

---

## Opening a Folder from the CLI

You can pass a folder path directly when launching from the terminal:

```bash
# Development
npx electron . /path/to/your/notes

# Packaged app (macOS example)
open -a Focal /path/to/your/notes
```

---

## Development Notes

- **Type-check** without building: `npm run lint`
- Only `.md` and `.markdown` files are shown in the file tree; hidden files (dotfiles) are excluded
- The file tree traverses up to 5 levels deep
- Auto-save is debounced at 500 ms — closing the window before the timer fires will not lose changes (the save is triggered on every keystroke change, just delayed)

---

## License

MIT
