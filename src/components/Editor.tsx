import { useEffect, useImperativeHandle, useRef, forwardRef } from 'react'
import { EditorView, keymap, drawSelection } from '@codemirror/view'
import { Compartment, EditorState, type Extension } from '@codemirror/state'
import { markdown, markdownLanguage } from '@codemirror/lang-markdown'
import { languages } from '@codemirror/language-data'
import { GFM } from '@lezer/markdown'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { searchKeymap } from '@codemirror/search'
import type { Diagnostic } from '@codemirror/lint'
import { tokyoNightTheme, tokyoNightHighlight } from '@/lib/theme'
import { lintMarkdownContent } from '@/lib/linter'
import { focusModeCompartment, focusModePlugin, markdownDecorations } from '@/lib/markdownDecorations'

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
}

const editableCompartment = new Compartment()

function reportCursor(view: EditorView, onCursorChange?: (line: number, col: number) => void) {
  if (!onCursorChange) return
  const pos = view.state.selection.main.head
  const line = view.state.doc.lineAt(pos)
  onCursorChange(line.number, pos - line.from + 1)
}

export const Editor = forwardRef<EditorHandle, EditorProps>(function Editor(
  { content, focusMode, onChange, onCursorChange, onLintResults },
  ref
) {
  const containerRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const onChangeRef = useRef(onChange)
  const onCursorChangeRef = useRef(onCursorChange)

  onChangeRef.current = onChange
  onCursorChangeRef.current = onCursorChange

  useEffect(() => {
    if (!containerRef.current) return

    const extensions: Extension[] = [
      history(),
      drawSelection(),
      markdown({ base: markdownLanguage, codeLanguages: languages, extensions: [GFM] }),
      tokyoNightTheme,
      tokyoNightHighlight,
      markdownDecorations,
      EditorView.lineWrapping,
      editableCompartment.of(EditorView.editable.of(true)),
      focusModeCompartment.of(focusMode ? focusModePlugin : []),
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        ...searchKeymap,
        indentWithTab,
      ]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString())
        }
        if (update.docChanged || update.selectionSet) {
          reportCursor(update.view, onCursorChangeRef.current)
        }
      }),
    ]

    const view = new EditorView({
      state: EditorState.create({ doc: content, extensions }),
      parent: containerRef.current,
    })

    viewRef.current = view
    view.focus()
    reportCursor(view, onCursorChangeRef.current)

    return () => {
      view.destroy()
      viewRef.current = null
    }
  }, [])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    view.dispatch({
      effects: focusModeCompartment.reconfigure(focusMode ? focusModePlugin : []),
    })
  }, [focusMode])

  useEffect(() => {
    const view = viewRef.current
    if (!view) return
    const current = view.state.doc.toString()
    if (current === content) return
    const anchor = Math.min(view.state.selection.main.anchor, content.length)
    view.dispatch({
      changes: { from: 0, to: current.length, insert: content },
      selection: { anchor },
    })
  }, [content])

  useEffect(() => {
    let cancelled = false
    if (!onLintResults) return

    const timer = window.setTimeout(() => {
      lintMarkdownContent(content).then((diagnostics) => {
        if (!cancelled) onLintResults(diagnostics)
      })
    }, 120)

    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [content, onLintResults])

  useImperativeHandle(ref, () => ({
    scrollToLine(lineNumber: number) {
      const view = viewRef.current
      if (!view) return
      const targetLine = view.state.doc.line(Math.min(Math.max(lineNumber, 1), view.state.doc.lines))
      view.dispatch({
        selection: { anchor: targetLine.from },
        effects: EditorView.scrollIntoView(targetLine.from, { y: 'center' }),
      })
      view.focus()
    },
    getView() {
      return viewRef.current
    },
  }), [])

  return (
    <div className={`hybrid-editor${focusMode ? ' focus-mode' : ''}`}>
      <div className="hybrid-document">
        <div ref={containerRef} className="hybrid-live-editor" />
      </div>
    </div>
  )
})
