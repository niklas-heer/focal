import {
  ViewPlugin,
  DecorationSet,
  Decoration,
  EditorView,
  ViewUpdate,
  WidgetType,
} from '@codemirror/view'
import { RangeSetBuilder, Extension, Compartment } from '@codemirror/state'
import { syntaxTree } from '@codemirror/language'
import type { SyntaxNode } from '@lezer/common'

class HrWidget extends WidgetType {
  toDOM() {
    const el = document.createElement('div')
    el.className = 'cm-md-hr'
    return el
  }
  eq() { return true }
  ignoreEvent() { return false }
}

class BulletWidget extends WidgetType {
  constructor(private readonly ordered: boolean, private readonly index: number) { super() }
  toDOM() {
    const el = document.createElement('span')
    el.className = 'cm-md-list-bullet'
    el.textContent = this.ordered ? `${this.index}.` : '•'
    return el
  }
  eq(other: BulletWidget) { return other.ordered === this.ordered && other.index === this.index }
  ignoreEvent() { return false }
}

class HeadingWidget extends WidgetType {
  constructor(private readonly level: number) { super() }
  toDOM() {
    const el = document.createElement('span')
    el.className = `cm-md-heading-chip cm-md-heading-chip-${this.level}`
    el.textContent = `H${this.level}`
    return el
  }
  eq(other: HeadingWidget) { return other.level === this.level }
  ignoreEvent() { return false }
}

class CodeBlockHeaderWidget extends WidgetType {
  constructor(private readonly lang: string) { super() }
  toDOM() {
    const el = document.createElement('span')
    el.className = 'cm-md-codeblock-chip'
    el.textContent = this.lang || 'Code'
    return el
  }
  eq(other: CodeBlockHeaderWidget) { return other.lang === this.lang }
  ignoreEvent() { return false }
}

class CopyButtonWidget extends WidgetType {
  constructor(private readonly codeText: string, private readonly lang: string) { super() }
  toDOM() {
    const btn = document.createElement('button')
    btn.className = 'cm-md-copy-btn'
    btn.textContent = 'copy'
    btn.title = 'Copy code'
    btn.setAttribute('data-code', this.codeText)
    btn.addEventListener('click', (e) => {
      e.preventDefault()
      e.stopPropagation()
      navigator.clipboard.writeText(this.codeText).then(() => {
        btn.textContent = 'copied!'
        btn.classList.add('copied')
        setTimeout(() => {
          btn.textContent = 'copy'
          btn.classList.remove('copied')
        }, 1500)
      })
    })
    return btn
  }
  eq(other: CopyButtonWidget) { return other.codeText === this.codeText && other.lang === this.lang }
  ignoreEvent() { return true }
}

class TablePipeWidget extends WidgetType {
  toDOM() {
    const el = document.createElement('span')
    el.className = 'cm-md-table-pipe'
    el.textContent = '|'
    return el
  }
  eq() { return true }
  ignoreEvent() { return false }
}

interface PendingDeco {
  from: number
  to: number
  deco: Decoration
}

function buildDecorations(view: EditorView): DecorationSet {
  const pending: PendingDeco[] = []
  const { selection } = view.state

  const doc = view.state.doc
  const tree = syntaxTree(view.state)

  function selectionTouches(from: number, to: number) {
    const { from: selFrom, to: selTo } = selection.main
    return (selFrom >= from && selFrom <= to) || (selTo >= from && selTo <= to) || (selFrom <= from && selTo >= to)
  }

  function cursorAt(pos: number) {
    return selection.main.empty && selection.main.head === pos
  }

  function showMarker(from: number, to: number) {
    return selectionTouches(from, to) || cursorAt(from) || cursorAt(to)
  }

  function add(from: number, to: number, deco: Decoration) {
    if (from < to || (deco.spec as { widget?: unknown }).widget) {
      pending.push({ from, to, deco })
    }
  }

  function processInlineDecorations(containerNode: SyntaxNode) {
    const cursor = containerNode.cursor()
    if (!cursor.firstChild()) return
    do {
      walkInline(cursor)
    } while (cursor.nextSibling())
  }

  function walkInline(cursor: ReturnType<SyntaxNode['cursor']>) {
    const { from, to } = cursor
    const name = cursor.name

    if (name === 'StrongEmphasis') {
      add(from, to, Decoration.mark({ class: 'cm-md-bold' }))
      tree.iterate({
        from,
        to,
        enter(c) {
          if (c.name === 'EmphasisMark' && !showMarker(c.from, c.to)) {
            add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
          }
        },
      })
      return
    }

    if (name === 'Emphasis') {
      add(from, to, Decoration.mark({ class: 'cm-md-italic' }))
      tree.iterate({
        from,
        to,
        enter(c) {
          if (c.name === 'EmphasisMark' && !showMarker(c.from, c.to)) {
            add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
          }
        },
      })
      return
    }

    if (name === 'InlineCode') {
      const ticks = doc.sliceString(from, to).match(/^`+/)?.[0].length ?? 1
      add(from + ticks, to - ticks, Decoration.mark({ class: 'cm-md-code-inline' }))
      if (!showMarker(from, from + ticks)) add(from, from + ticks, Decoration.mark({ class: 'cm-md-marker-hidden' }))
      if (!showMarker(to - ticks, to)) add(to - ticks, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
      return
    }

    if (name === 'Link' || name === 'Image') {
      const text = doc.sliceString(from, to)
      const m = text.match(/^(!?)\[([^\]]*)\]\(([^)]*)\)$/)
      if (m) {
        const isImage = m[1] === '!'
        const labelStart = from + (isImage ? 2 : 1)
        const labelEnd = labelStart + m[2].length
        const urlStart = labelEnd + 2
        const prefixEnd = labelStart
        add(labelStart, labelEnd, Decoration.mark({ class: 'cm-md-link-text' }))
        if (!showMarker(from, prefixEnd)) add(from, prefixEnd, Decoration.mark({ class: 'cm-md-marker-hidden' }))
        if (!selectionTouches(urlStart, to)) add(labelEnd, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
      }
      return
    }

    if (cursor.firstChild()) {
      do {
        walkInline(cursor)
      } while (cursor.nextSibling())
      cursor.parent()
    }
  }

  tree.iterate({
    from: 0,
    to: doc.length,
    enter(node) {
      const { from, to, name } = node

      const headingMatch = name.match(/^(ATX|Setext)Heading(\d)$/)
      if (headingMatch) {
        const level = parseInt(headingMatch[2], 10)
        const firstLine = doc.lineAt(from)

        if (headingMatch[1] === 'ATX') {
          const markerMatch = firstLine.text.match(/^\s{0,3}(#{1,6})\s+/)
          if (markerMatch) {
            const markerEnd = firstLine.from + markerMatch[0].length
            if (!showMarker(firstLine.from, markerEnd)) {
              add(firstLine.from, markerEnd, Decoration.replace({ widget: new HeadingWidget(level) }))
            } else {
              add(firstLine.from, markerEnd, Decoration.mark({ class: 'cm-md-heading-marker-active' }))
            }
            if (markerEnd < to) {
              add(markerEnd, to, Decoration.mark({ class: `cm-md-h${level}` }))
            }
            processInlineDecorations(node.node)
          }
        } else {
          const lastLine = doc.lineAt(Math.max(from, to - 1))
          add(firstLine.from, firstLine.to, Decoration.mark({ class: `cm-md-h${level}` }))
          add(lastLine.from, lastLine.to, Decoration.mark({ class: showMarker(lastLine.from, lastLine.to) ? 'cm-md-heading-marker-active' : 'cm-md-heading-marker' }))
          processInlineDecorations(node.node)
        }
        return false
      }

      if (name === 'StrongEmphasis') {
        add(from, to, Decoration.mark({ class: 'cm-md-bold' }))
        tree.iterate({
          from,
          to,
          enter(c) {
            if (c.name === 'EmphasisMark' && !showMarker(c.from, c.to)) {
              add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
            }
          },
        })
        return false
      }

      if (name === 'Emphasis') {
        add(from, to, Decoration.mark({ class: 'cm-md-italic' }))
        tree.iterate({
          from,
          to,
          enter(c) {
            if (c.name === 'EmphasisMark' && !showMarker(c.from, c.to)) {
              add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
            }
          },
        })
        return false
      }

      if (name === 'InlineCode') {
        const ticks = doc.sliceString(from, to).match(/^`+/)?.[0].length ?? 1
        add(from + ticks, to - ticks, Decoration.mark({ class: 'cm-md-code-inline' }))
        if (!showMarker(from, from + ticks)) add(from, from + ticks, Decoration.mark({ class: 'cm-md-marker-hidden' }))
        if (!showMarker(to - ticks, to)) add(to - ticks, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
        return false
      }

      if (name === 'Link' || name === 'Image') {
        const text = doc.sliceString(from, to)
        const m = text.match(/^(!?)\[([^\]]*)\]\(([^)]*)\)$/)
        if (m) {
          const isImage = m[1] === '!'
          const labelStart = from + (isImage ? 2 : 1)
          const labelEnd = labelStart + m[2].length
          const urlStart = labelEnd + 2
          const prefixEnd = labelStart
          add(labelStart, labelEnd, Decoration.mark({ class: 'cm-md-link-text' }))
          if (!showMarker(from, prefixEnd)) add(from, prefixEnd, Decoration.mark({ class: 'cm-md-marker-hidden' }))
          if (!selectionTouches(urlStart, to)) add(labelEnd, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
        }
        return false
      }

      if (name === 'Blockquote') {
        add(from, to, Decoration.mark({ class: 'cm-md-blockquote' }))
        tree.iterate({
          from,
          to,
          enter(child) {
            if (child.name === 'QuoteMark') {
              const line = doc.lineAt(child.from)
              const markerEnd = Math.min(child.to + 1, line.to)
              if (!showMarker(child.from, markerEnd)) {
                add(child.from, markerEnd, Decoration.mark({ class: 'cm-md-marker-hidden' }))
              } else {
                add(child.from, markerEnd, Decoration.mark({ class: 'cm-md-marker-active' }))
              }
            }
          },
        })
        return false
      }

      if (name === 'HorizontalRule') {
        const line = doc.lineAt(from)
        if (!selectionTouches(line.from, line.to)) {
          add(from, to, Decoration.replace({ widget: new HrWidget() }))
        }
        return false
      }

      if (name === 'BulletList') {
        let itemIndex = 0
        tree.iterate({
          from, to,
          enter(child) {
            if (child.name === 'ListItem') {
              const itemLine = doc.lineAt(child.from)
              const markerMatch = itemLine.text.match(/^(\s*)([-*+])\s/)
              if (markerMatch) {
                const markerStart = child.from + markerMatch[1].length
                const markerEnd = markerStart + markerMatch[2].length + 1
                if (showMarker(markerStart, markerEnd)) {
                  add(markerStart, markerEnd, Decoration.mark({ class: 'cm-md-marker-active' }))
                } else {
                  add(markerStart, markerEnd, Decoration.replace({ widget: new BulletWidget(false, itemIndex) }))
                }
              }
              processInlineDecorations(child.node)
              itemIndex++
            }
          },
        })
        return false
      }

      if (name === 'OrderedList') {
        let itemIndex = 1
        tree.iterate({
          from, to,
          enter(child) {
            if (child.name === 'ListItem') {
              const itemLine = doc.lineAt(child.from)
              const markerMatch = itemLine.text.match(/^(\s*)(\d+[.)]\s)/)
              if (markerMatch) {
                const markerStart = child.from + markerMatch[1].length
                const markerEnd = markerStart + markerMatch[2].length
                if (showMarker(markerStart, markerEnd)) {
                  add(markerStart, markerEnd, Decoration.mark({ class: 'cm-md-marker-active' }))
                } else {
                  add(markerStart, markerEnd, Decoration.replace({ widget: new BulletWidget(true, itemIndex) }))
                }
              }
              processInlineDecorations(child.node)
              itemIndex++
            }
          },
        })
        return false
      }

      if (name === 'FencedCode') {
        let langName = ''
        let codeText = ''
        let openFenceFrom = from
        let openFenceTo = -1
        let closeFenceFrom = -1
        let closeFenceTo = -1
        let infoFrom = -1
        let infoTo = -1
        const openLine = doc.lineAt(from)
        const closeLine = doc.lineAt(Math.max(from, to - 1))
        const showOpenFence = selectionTouches(openLine.from, openLine.to)
        const showCloseFence = selectionTouches(closeLine.from, closeLine.to)
        tree.iterate({
          from, to,
          enter(child) {
            if (child.name === 'CodeInfo') {
              langName = doc.sliceString(child.from, child.to).trim()
              infoFrom = child.from
              infoTo = child.to
              if (selectionTouches(child.from, child.to)) {
                add(child.from, child.to, Decoration.mark({ class: 'cm-md-codeblock-info-active' }))
              } else {
                add(child.from, child.to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
              }
            }
            if (child.name === 'CodeMark') {
              const isOpen = child.from === from
              if (isOpen) {
                openFenceFrom = child.from
                openFenceTo = child.to
              } else {
                closeFenceFrom = child.from
                closeFenceTo = child.to
              }
              const fenceLine = doc.lineAt(child.from)
              const showFence = isOpen ? showOpenFence : showCloseFence
              if (showFence) {
                add(child.from, child.to, Decoration.mark({
                  class: isOpen ? 'cm-md-codeblock-fence-active' : 'cm-md-codeblock-fence-end-active',
                }))
              } else {
                add(fenceLine.from, fenceLine.to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
              }
            }
            if (child.name === 'CodeText') {
              codeText = doc.sliceString(child.from, child.to)
              for (let pos = child.from; pos < child.to;) {
                const line = doc.lineAt(pos)
                const lineEnd = Math.min(line.to, child.to)
                add(line.from, lineEnd, Decoration.mark({ class: 'cm-md-codeblock-line' }))
                pos = line.to + 1
              }
            }
          },
        })
        if (openFenceTo >= 0) {
          if (!showOpenFence) {
            add(openLine.from, openLine.from, Decoration.widget({
              widget: new CodeBlockHeaderWidget(langName),
              side: -1,
            }))
            add(openLine.from, openLine.from, Decoration.line({ class: 'cm-md-codeblock-header-line' }))
          } else {
            add(openFenceFrom, openFenceTo, Decoration.mark({ class: 'cm-md-codeblock-fence-active' }))
            if (infoFrom >= 0 && infoTo > infoFrom) {
              add(infoFrom, infoTo, Decoration.mark({ class: 'cm-md-codeblock-info-active' }))
            }
          }
          if (!showOpenFence && codeText) {
            add(openLine.to, openLine.to, Decoration.widget({
              widget: new CopyButtonWidget(codeText, langName),
              side: 1,
            }))
          }
        }
        if (closeFenceFrom >= 0 && showCloseFence) {
          add(closeFenceFrom, closeFenceTo, Decoration.mark({ class: 'cm-md-codeblock-fence-end-active' }))
        }
        return false
      }

      if (name === 'Table') {
        let rowIndex = 0
        tree.iterate({
          from, to,
          enter(child) {
            if (child.name === 'TableHeader' || child.name === 'TableRow') {
              const rowClass = child.name === 'TableHeader'
                ? 'cm-md-table-row cm-md-table-row-header'
                : `cm-md-table-row${rowIndex % 2 === 0 ? ' cm-md-table-row-even' : ' cm-md-table-row-odd'}`
              add(child.from, child.to, Decoration.mark({ class: rowClass }))
              if (child.name === 'TableRow') rowIndex++
            }

            if (child.name === 'TableCell') {
              add(child.from, child.to, Decoration.mark({ class: 'cm-md-table-cell' }))
            }

            if (child.name === 'TableDelimiter') {
              if (showMarker(child.from, child.to)) {
                add(child.from, child.to, Decoration.mark({ class: 'cm-md-table-delimiter-active' }))
              } else {
                add(child.from, child.to, Decoration.replace({ widget: new TablePipeWidget() }))
              }
            }
          },
        })
        return false
      }
    },
  })

  pending.sort((a, b) => a.from !== b.from ? a.from - b.from : b.to - a.to)

  const builder = new RangeSetBuilder<Decoration>()
  for (const { from, to, deco } of pending) {
    builder.add(from, to, deco)
  }
  return builder.finish()
}

export const markdownDecorations: Extension = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    constructor(view: EditorView) { this.decorations = buildDecorations(view) }
    update(update: ViewUpdate) {
      if (update.docChanged || update.selectionSet || update.viewportChanged) {
        this.decorations = buildDecorations(update.view)
      }
    }
  },
  { decorations: (v) => v.decorations }
)

export const focusModeCompartment = new Compartment()

export function getFocusLineRange(view: EditorView, pos: number) {
  const doc = view.state.doc
  const currentLine = doc.lineAt(pos)
  let startLine = currentLine.number
  let endLine = currentLine.number

  while (startLine > 1 && doc.line(startLine - 1).text.trim() !== '') {
    startLine--
  }
  while (endLine < doc.lines && doc.line(endLine + 1).text.trim() !== '') {
    endLine++
  }

  return { startLine, endLine }
}

function buildFocusDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>()
  const cursorPos = view.state.selection.main.head
  const { startLine, endLine } = getFocusLineRange(view, cursorPos)
  const activeMark = Decoration.line({ class: 'cm-focus-active' })
  for (let i = startLine; i <= endLine; i++) {
    const line = view.state.doc.line(i)
    builder.add(line.from, line.from, activeMark)
  }
  return builder.finish()
}

export const focusModePlugin: Extension = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet
    constructor(view: EditorView) { this.decorations = buildFocusDecorations(view) }
    update(update: ViewUpdate) {
      if (update.selectionSet || update.docChanged || update.viewportChanged) {
        this.decorations = buildFocusDecorations(update.view)
      }
    }
  },
  { decorations: (v) => v.decorations }
)
