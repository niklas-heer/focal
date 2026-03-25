import { useEffect, useRef, useImperativeHandle, forwardRef } from 'react'
import { EditorView, keymap, drawSelection } from '@codemirror/view'
import { EditorState, Extension, Compartment } from '@codemirror/state'
import { markdown, markdownLanguage } from '@codemirror/lang-markdown'
import { languages } from '@codemirror/language-data'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { searchKeymap } from '@codemirror/search'
import { linter, lintGutter, type Diagnostic } from '@codemirror/lint'
import { markdownDecorations, focusModePlugin } from '@/lib/markdownDecorations'
import { tokyoNightTheme, tokyoNightHighlight } from '@/lib/theme'
import { buildMarkdownLinter } from '@/lib/linter'

export interface EditorHandle {
  scrollToLine: (line: number) => void
  getView: () => EditorView | null
}

interface EditorProps {
  content: string
  focusMode: boolean
  onChange: (value: string) => void
  onCursorChange?: (line: number, col: number) => void
  onLintResults?: (diagnostics: Diagnostic[]) => void
  wrapperRef?: React.RefObject<HTMLDivElement | null>
}

const focusCompartment = new Compartment()
const lintCompartment = new Compartment()

export const Editor = forwardRef<EditorHandle, EditorProps>(function Editor(
  { content, focusMode, onChange, onCursorChange, onLintResults, wrapperRef },
  ref
) {
  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const onChangeRef = useRef(onChange)
  const onCursorRef = useRef(onCursorChange)
  const onLintRef = useRef(onLintResults)

  onChangeRef.current = onChange
  onCursorRef.current = onCursorChange
  onLintRef.current = onLintResults

  useImperativeHandle(ref, () => ({
    scrollToLine(lineNumber: number) {
      const view = viewRef.current
      if (!view) return

      const clampedLine = Math.min(Math.max(1, lineNumber), view.state.doc.lines)
      const line = view.state.doc.line(clampedLine)

      view.dispatch({ selection: { anchor: line.from } })

      requestAnimationFrame(() => {
        const wrapper = wrapperRef?.current
        if (!wrapper) return
        const coords = view.coordsAtPos(line.from)
        if (!coords) return
        const wrapperRect = wrapper.getBoundingClientRect()
        const targetScrollTop = wrapper.scrollTop + coords.top - wrapperRect.top - wrapper.clientHeight / 3
        wrapper.scrollTo({ top: targetScrollTop, behavior: 'smooth' })
        view.focus()
      })
    },
    getView() {
      return viewRef.current
    },
  }))

  useEffect(() => {
    if (!containerRef.current) return

    const mdLinter = buildMarkdownLinter((diagnostics) => {
      onLintRef.current?.(diagnostics)
    })

    const extensions: Extension[] = [
      history(),
      drawSelection(),
      EditorState.allowMultipleSelections.of(true),
      markdown({ base: markdownLanguage, codeLanguages: languages }),
      tokyoNightTheme,
      tokyoNightHighlight,
      markdownDecorations,
      focusCompartment.of([]),
      lintCompartment.of([]),
      lintGutter(),
      linter(mdLinter, { delay: 600 }),
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        ...searchKeymap,
        indentWithTab,
      ]),
      EditorView.lineWrapping,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString())
        }
        if (update.selectionSet && onCursorRef.current) {
          const pos = update.state.selection.main.head
          const line = update.state.doc.lineAt(pos)
          onCursorRef.current(line.number, pos - line.from + 1)
        }
      }),
    ]

    const state = EditorState.create({ doc: content, extensions })
    const view = new EditorView({ state, parent: containerRef.current })
    viewRef.current = view

    return () => {
      view.destroy()
      viewRef.current = null
    }
  }, [])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    const currentContent = view.state.doc.toString()
    if (currentContent !== content) {
      view.dispatch({
        changes: { from: 0, to: currentContent.length, insert: content },
      })
    }
  }, [content])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: focusCompartment.reconfigure(focusMode ? focusModePlugin : []),
    })
  }, [focusMode])

  return (
    <div
      ref={containerRef}
      style={{ width: '100%', maxWidth: 'var(--editor-max-width)' }}
    />
  )
})
