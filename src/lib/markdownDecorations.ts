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

interface PendingDeco {
  from: number
  to: number
  deco: Decoration
}

function buildDecorations(view: EditorView): DecorationSet {
  const pending: PendingDeco[] = []
  const { selection } = view.state
  const cursorLine = view.state.doc.lineAt(selection.main.head).number

  const tree = syntaxTree(view.state)
  const doc = view.state.doc

  const lineFrom = view.visibleRanges[0]?.from ?? 0
  const lineTo = view.visibleRanges[view.visibleRanges.length - 1]?.to ?? doc.length

  function cursorIn(from: number, to: number) {
    const { from: selFrom, to: selTo } = selection.main
    return (selFrom >= from && selFrom <= to) || (selTo >= from && selTo <= to) || (selFrom <= from && selTo >= to)
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
      if (!cursorIn(from, to)) {
        add(from, to, Decoration.mark({ class: 'cm-md-bold' }))
        tree.iterate({ from, to, enter(c) { if (c.name === 'EmphasisMark') add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' })) } })
      }
      return
    }

    if (name === 'Emphasis') {
      if (!cursorIn(from, to)) {
        add(from, to, Decoration.mark({ class: 'cm-md-italic' }))
        tree.iterate({ from, to, enter(c) { if (c.name === 'EmphasisMark') add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' })) } })
      }
      return
    }

    if (name === 'InlineCode') {
      if (!cursorIn(from, to)) {
        const ticks = doc.sliceString(from, to).match(/^`+/)?.[0].length ?? 1
        add(from, from + ticks, Decoration.mark({ class: 'cm-md-marker-hidden' }))
        add(from + ticks, to - ticks, Decoration.mark({ class: 'cm-md-code-inline' }))
        add(to - ticks, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
      }
      return
    }

    if (name === 'Link' || name === 'Image') {
      if (!cursorIn(from, to)) {
        const text = doc.sliceString(from, to)
        const m = text.match(/^(!?)\[([^\]]*)\]\(([^)]*)\)$/)
        if (m) {
          const isImage = m[1] === '!'
          const labelStart = from + (isImage ? 2 : 1)
          const labelEnd = labelStart + m[2].length
          add(from, labelStart, Decoration.mark({ class: 'cm-md-marker-hidden' }))
          add(labelStart, labelEnd, Decoration.mark({ class: 'cm-md-link-text' }))
          add(labelEnd, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
        }
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
    from: lineFrom,
    to: lineTo,
    enter(node) {
      const { from, to, name } = node

      const headingMatch = name.match(/^ATXHeading(\d)$/)
      if (headingMatch) {
        const level = parseInt(headingMatch[1])
        const line = doc.lineAt(from)
        const markerMatch = line.text.match(/^(#{1,6}) /)
        if (markerMatch) {
          const markerEnd = line.from + markerMatch[1].length + 1
          add(line.from, markerEnd, Decoration.mark({
            class: line.number === cursorLine ? 'cm-md-heading-marker-active' : 'cm-md-heading-marker'
          }))
          if (markerEnd < to) add(markerEnd, to, Decoration.mark({ class: `cm-md-h${level}` }))
          processInlineDecorations(node.node)
        }
        return false
      }

      if (name === 'StrongEmphasis') {
        if (!cursorIn(from, to)) {
          add(from, to, Decoration.mark({ class: 'cm-md-bold' }))
          tree.iterate({ from, to, enter(c) { if (c.name === 'EmphasisMark') add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' })) } })
        }
        return false
      }

      if (name === 'Emphasis') {
        if (!cursorIn(from, to)) {
          add(from, to, Decoration.mark({ class: 'cm-md-italic' }))
          tree.iterate({ from, to, enter(c) { if (c.name === 'EmphasisMark') add(c.from, c.to, Decoration.mark({ class: 'cm-md-marker-hidden' })) } })
        }
        return false
      }

      if (name === 'InlineCode') {
        if (!cursorIn(from, to)) {
          const ticks = doc.sliceString(from, to).match(/^`+/)?.[0].length ?? 1
          add(from, from + ticks, Decoration.mark({ class: 'cm-md-marker-hidden' }))
          add(from + ticks, to - ticks, Decoration.mark({ class: 'cm-md-code-inline' }))
          add(to - ticks, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
        }
        return false
      }

      if (name === 'Link' || name === 'Image') {
        if (!cursorIn(from, to)) {
          const text = doc.sliceString(from, to)
          const m = text.match(/^(!?)\[([^\]]*)\]\(([^)]*)\)$/)
          if (m) {
            const isImage = m[1] === '!'
            const labelStart = from + (isImage ? 2 : 1)
            const labelEnd = labelStart + m[2].length
            add(from, labelStart, Decoration.mark({ class: 'cm-md-marker-hidden' }))
            add(labelStart, labelEnd, Decoration.mark({ class: 'cm-md-link-text' }))
            add(labelEnd, to, Decoration.mark({ class: 'cm-md-marker-hidden' }))
          }
        }
        return false
      }

      if (name === 'Blockquote') {
        add(from, to, Decoration.mark({ class: 'cm-md-blockquote' }))
        return false
      }

      if (name === 'HorizontalRule') {
        const line = doc.lineAt(from)
        if (!cursorIn(line.from, line.to)) {
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
              const isCursorItem = itemLine.number === cursorLine
              const markerMatch = itemLine.text.match(/^(\s*)([-*+])\s/)
              if (markerMatch) {
                const markerStart = child.from + markerMatch[1].length
                const markerEnd = markerStart + markerMatch[2].length + 1
                if (isCursorItem) {
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
              const isCursorItem = itemLine.number === cursorLine
              const markerMatch = itemLine.text.match(/^(\s*)(\d+[.)]\s)/)
              if (markerMatch) {
                const markerStart = child.from + markerMatch[1].length
                const markerEnd = markerStart + markerMatch[2].length
                if (isCursorItem) {
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
        let openFenceTo = -1
        tree.iterate({
          from, to,
          enter(child) {
            if (child.name === 'CodeInfo') {
              langName = doc.sliceString(child.from, child.to).trim()
            }
            if (child.name === 'CodeMark') {
              const isOpen = child.from === from
              if (isOpen) openFenceTo = child.to
              add(child.from, child.to, Decoration.mark({
                class: isOpen ? 'cm-md-codeblock-fence' : 'cm-md-codeblock-fence-end',
                attributes: isOpen && langName ? { 'data-lang': langName } : {},
              }))
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
          const fenceLine = doc.lineAt(from)
          add(fenceLine.to, fenceLine.to, Decoration.widget({
            widget: new CopyButtonWidget(codeText, langName),
            side: 1,
          }))
        }
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

function buildFocusDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>()
  const cursorPos = view.state.selection.main.head
  const cursorLine = view.state.doc.lineAt(cursorPos).number
  const activeMark = Decoration.line({ class: 'cm-focus-active' })
  for (let i = 1; i <= view.state.doc.lines; i++) {
    const line = view.state.doc.line(i)
    if (i === cursorLine) builder.add(line.from, line.from, activeMark)
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
