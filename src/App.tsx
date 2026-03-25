import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { Editor, type EditorHandle } from '@/components/Editor'
import { FileTree } from '@/components/FileTree'
import { Outline } from '@/components/Outline'
import { StatusBar } from '@/components/StatusBar'
import { CommandPalette } from '@/components/CommandPalette'
import { extractHeadings } from '@/lib/outline'
import { applyMarkdownFixes } from '@/lib/linter'
import type { Diagnostic } from '@codemirror/lint'
import type { FileNode } from '@/types'

type SaveStatus = 'saved' | 'saving' | 'unsaved'
type SidebarPanel = 'files' | 'outline'

function countWords(text: string): number {
  return text.trim().split(/\s+/).filter(Boolean).length
}

function activeHeadingLine(headings: ReturnType<typeof extractHeadings>, cursorLine: number): number {
  let best = 0
  function walk(nodes: ReturnType<typeof extractHeadings>) {
    for (const n of nodes) {
      if (n.line <= cursorLine) best = n.line
      walk(n.children)
    }
  }
  walk(headings)
  return best
}

export default function App() {
  const [folderPath, setFolderPath] = useState<string | null>(null)
  const [files, setFiles] = useState<FileNode[]>([])
  const [activeFile, setActiveFile] = useState<string | null>(null)
  const [content, setContent] = useState('')
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('saved')
  const [focusMode, setFocusMode] = useState(false)
  const [sidebarVisible, setSidebarVisible] = useState(true)
  const [sidebarPanel, setSidebarPanel] = useState<SidebarPanel>('files')
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [cursorLine, setCursorLine] = useState(1)
  const [cursorCol, setCursorCol] = useState(1)
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const editorRef = useRef<EditorHandle>(null)
  const editorWrapperRef = useRef<HTMLDivElement>(null)
  const [lintDiagnostics, setLintDiagnostics] = useState<Diagnostic[]>([])

  const headings = useMemo(() => extractHeadings(content), [content])
  const activeHeading = useMemo(
    () => activeHeadingLine(headings, cursorLine),
    [headings, cursorLine]
  )

  const openFolder = useCallback(async (path?: string) => {
    const targetPath = path ?? (await window.electronAPI.openFolder())
    if (!targetPath) return
    setFolderPath(targetPath)
    const tree = await window.electronAPI.readDir(targetPath)
    setFiles(tree)
    setActiveFile(null)
    setContent('')
  }, [])

  const selectFile = useCallback(async (filePath: string) => {
    const raw = await window.electronAPI.readFile(filePath)
    if (raw !== null) {
      setActiveFile(filePath)
      setContent(raw)
      setSaveStatus('saved')
    }
  }, [])

  const handleContentChange = useCallback((newContent: string) => {
    setContent(newContent)
    setSaveStatus('saving')
    if (saveTimer.current) clearTimeout(saveTimer.current)
    saveTimer.current = setTimeout(async () => {
      if (activeFile) {
        await window.electronAPI.writeFile(activeFile, newContent)
        setSaveStatus('saved')
      }
    }, 500)
  }, [activeFile])

  const createNewFile = useCallback(async () => {
    if (!folderPath) {
      await openFolder()
      return
    }
    const name = `untitled-${Date.now()}.md`
    const filePath = await window.electronAPI.createFile(folderPath, name)
    if (filePath) {
      const tree = await window.electronAPI.readDir(folderPath)
      setFiles(tree)
      await selectFile(filePath)
    }
  }, [folderPath, openFolder, selectFile])

  const refreshFiles = useCallback(async () => {
    if (!folderPath) return
    const tree = await window.electronAPI.readDir(folderPath)
    setFiles(tree)
  }, [folderPath])

  const handleFixLint = useCallback(() => {
    if (!activeFile) return
    const fixed = applyMarkdownFixes(content)
    if (fixed !== content) {
      setContent(fixed)
      window.electronAPI.writeFile(activeFile, fixed)
    }
  }, [content, activeFile])

  const handleJumpToLine = useCallback((line: number) => {
    editorRef.current?.scrollToLine(line)
  }, [])

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey
      if (meta && e.key === 'k') { e.preventDefault(); setPaletteOpen(p => !p) }
      if (meta && e.key === '\\') { e.preventDefault(); setSidebarVisible(v => !v) }
      if (meta && e.shiftKey && e.key === 'F') { e.preventDefault(); setFocusMode(f => !f) }
      if (e.key === 'Escape') setPaletteOpen(false)
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [])

  useEffect(() => {
    if (!window.electronAPI?.onOpenFolderFromCli) return
    const unsubscribe = window.electronAPI.onOpenFolderFromCli((folderArg: string) => {
      openFolder(folderArg)
    })
    return unsubscribe
  }, [openFolder])

  const folderName = folderPath ? folderPath.split('/').pop() ?? folderPath : ''
  const activeFileName = activeFile ? activeFile.split('/').pop() ?? activeFile : null

  const commands = [
    { id: 'open-folder', label: 'Open Folder', shortcut: '⌘O', icon: '📁', action: () => openFolder() },
    { id: 'new-file', label: 'New File', shortcut: '⌘N', icon: '📄', action: createNewFile },
    { id: 'toggle-focus', label: focusMode ? 'Exit Focus Mode' : 'Enter Focus Mode', shortcut: '⌘⇧F', icon: '🎯', action: () => setFocusMode(f => !f) },
    { id: 'toggle-sidebar', label: sidebarVisible ? 'Hide Sidebar' : 'Show Sidebar', shortcut: '⌘\\', icon: '⬜', action: () => setSidebarVisible(v => !v) },
    { id: 'show-outline', label: 'Show Outline', shortcut: '', icon: '§', action: () => { setSidebarPanel('outline'); setSidebarVisible(true) } },
    { id: 'show-files', label: 'Show Files', shortcut: '', icon: '📂', action: () => { setSidebarPanel('files'); setSidebarVisible(true) } },
  ]

  return (
    <div className="app">
      <div className="app-drag-region" />
      <div className="app-body">
        <div className={`sidebar${sidebarVisible ? '' : ' collapsed'}`}>
          <div className="sidebar-tabs">
            <button
              className={`sidebar-tab${sidebarPanel === 'files' ? ' active' : ''}`}
              onClick={() => setSidebarPanel('files')}
              title="Files"
            >
              <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
                <path d="M1 4.5C1 3.67 1.67 3 2.5 3H6l1.5 1.5H13.5C14.33 4.5 15 5.17 15 6v6.5C15 13.33 14.33 14 13.5 14h-11C1.67 14 1 13.33 1 12.5v-8z" fill="currentColor" opacity="0.8"/>
              </svg>
              Files
            </button>
            <button
              className={`sidebar-tab${sidebarPanel === 'outline' ? ' active' : ''}`}
              onClick={() => setSidebarPanel('outline')}
              title="Outline"
            >
              <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
                <rect x="1" y="2" width="6" height="1.5" rx="0.75" fill="currentColor"/>
                <rect x="3" y="5.5" width="5" height="1.5" rx="0.75" fill="currentColor" opacity="0.7"/>
                <rect x="1" y="9" width="8" height="1.5" rx="0.75" fill="currentColor"/>
                <rect x="3" y="12.5" width="5" height="1.5" rx="0.75" fill="currentColor" opacity="0.7"/>
              </svg>
              Outline
            </button>
          </div>

          {sidebarPanel === 'files' ? (
            folderPath ? (
              <FileTree
                files={files}
                activeFile={activeFile}
                folderName={folderName}
                folderPath={folderPath ?? ''}
                onFileSelect={selectFile}
                onRefresh={refreshFiles}
              />
            ) : (
              <>
                <div className="sidebar-header">
                  <span className="sidebar-folder-name">Focal</span>
                </div>
                <div className="sidebar-empty">
                  <svg className="sidebar-empty-icon" width="32" height="32" viewBox="0 0 32 32" fill="none">
                    <path d="M4 9C4 7.34 5.34 6 7 6H13L16 9H25C26.66 9 28 10.34 28 12V25C28 26.66 26.66 28 25 28H7C5.34 28 4 26.66 4 25V9Z" stroke="currentColor" strokeWidth="1.5" fill="none"/>
                  </svg>
                  <span className="sidebar-empty-text">No folder open</span>
                  <span className="sidebar-empty-hint">Open a folder to get started</span>
                  <button className="sidebar-open-btn" onClick={() => openFolder()}>Open Folder</button>
                </div>
              </>
            )
          ) : (
            <Outline
              headings={headings}
              activeLine={activeHeading}
              onJump={handleJumpToLine}
            />
          )}
        </div>

        <div className={`editor-area${focusMode ? ' focus-mode' : ''}`}>
          <div className="editor-toolbar">
            <button className="editor-toolbar-btn" title="Command Palette (⌘K)" onClick={() => setPaletteOpen(true)}>
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                <rect x="1" y="4" width="14" height="2" rx="1" fill="currentColor"/>
                <rect x="1" y="7.5" width="10" height="2" rx="1" fill="currentColor"/>
                <rect x="1" y="11" width="12" height="2" rx="1" fill="currentColor"/>
              </svg>
            </button>
            <button className={`editor-toolbar-btn${focusMode ? ' active' : ''}`} title="Focus Mode (⌘⇧F)" onClick={() => setFocusMode(f => !f)}>
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                <circle cx="8" cy="8" r="2.5" fill="currentColor"/>
                <path d="M1 5V2h3M12 2h3v3M1 11v3h3M12 14h3v-3" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/>
              </svg>
            </button>
            <div className="editor-toolbar-sep"/>
            <button className="editor-toolbar-btn" title="New File (⌘N)" onClick={createNewFile}>
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
                <path d="M9 1H3C2.45 1 2 1.45 2 2v12c0 .55.45 1 1 1h10c.55 0 1-.45 1-1V6L9 1Z" stroke="currentColor" strokeWidth="1.3" fill="none"/>
                <path d="M9 1v5h5" stroke="currentColor" strokeWidth="1.3" fill="none"/>
                <path d="M8 9.5v3M6.5 11h3" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/>
              </svg>
            </button>
          </div>

          {activeFile ? (
            <div className="editor-wrapper" ref={editorWrapperRef}>
              <Editor
                ref={editorRef}
                content={content}
                focusMode={focusMode}
                onChange={handleContentChange}                onCursorChange={(l, c) => { setCursorLine(l); setCursorCol(c) }}
                onLintResults={setLintDiagnostics}
                wrapperRef={editorWrapperRef}
              />
            </div>
          ) : (
            <div className="editor-empty">
              <span className="editor-empty-wordmark">focal</span>
              <span className="editor-empty-hint">
                {folderPath ? 'Select a file to edit' : 'Open a folder to begin — ⌘K'}
              </span>
            </div>
          )}
        </div>
      </div>

      <StatusBar
        fileName={activeFileName}
        line={cursorLine}
        col={cursorCol}
        wordCount={countWords(content)}
        saveStatus={saveStatus}
        focusMode={focusMode}
        sidebarVisible={sidebarVisible}
        lintIssues={lintDiagnostics.length}
        onToggleFocus={() => setFocusMode(f => !f)}
        onToggleSidebar={() => setSidebarVisible(v => !v)}
        onFixLint={handleFixLint}
      />

      <CommandPalette
        isOpen={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        commands={commands}
      />
    </div>
  )
}
