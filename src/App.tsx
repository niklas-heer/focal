import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { Editor, type EditorHandle } from '@/components/Editor'
import { FileTree } from '@/components/FileTree'
import { Outline } from '@/components/Outline'
import { StatusBar } from '@/components/StatusBar'
import { CommandPalette } from '@/components/CommandPalette'
import { extractHeadings } from '@/lib/outline'
import { applyMarkdownFixes } from '@/lib/linter'
import {
  applyFontPreset,
  applyTheme,
  defaultFontPresetId,
  defaultThemeId,
  fontPresetOptions,
  isFontPresetId,
  isThemeId,
  themeOptions,
  type FontPresetId,
  type ThemeId,
} from '@/lib/theme'
import type { Diagnostic } from '@codemirror/lint'
import type { FileNode } from '@/types'

type SaveStatus = 'saved' | 'saving' | 'unsaved'
type SidebarPanel = 'files' | 'outline'
type RecentItem = {
  path: string
  label: string
  parentPath?: string
}

function countWords(text: string): number {
  return text.trim().split(/\s+/).filter(Boolean).length
}

function countFiles(nodes: FileNode[]): number {
  return nodes.reduce((total, node) => (
    total + (node.type === 'file' ? 1 : countFiles(node.children ?? []))
  ), 0)
}

function countFolders(nodes: FileNode[]): number {
  return nodes.reduce((total, node) => (
    total + (node.type === 'directory'
      ? 1 + countFolders(node.children ?? [])
      : 0)
  ), 0)
}

function formatReadingTime(words: number): string {
  const minutes = Math.max(1, Math.round(words / 200))
  return `${minutes} min read`
}

function relativePath(basePath: string | null, filePath: string | null): string | null {
  if (!basePath || !filePath) return null
  if (!filePath.startsWith(basePath)) return filePath
  return filePath.slice(basePath.length + 1)
}

function pushRecent(items: RecentItem[], next: RecentItem, limit = 8): RecentItem[] {
  return [next, ...items.filter((item) => item.path !== next.path)].slice(0, limit)
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
  const [showHiddenFiles, setShowHiddenFiles] = useState<boolean>(() => {
    if (typeof window === 'undefined') return false
    return window.localStorage.getItem('focal.showHiddenFiles') === 'true'
  })
  const [themeId, setThemeId] = useState<ThemeId>(() => {
    const stored = typeof window !== 'undefined' ? window.localStorage.getItem('focal.theme') : null
    return stored && isThemeId(stored) ? stored : defaultThemeId
  })
  const [fontPresetId, setFontPresetId] = useState<FontPresetId>(() => {
    const stored = typeof window !== 'undefined' ? window.localStorage.getItem('focal.fontPreset') : null
    return stored && isFontPresetId(stored) ? stored : defaultFontPresetId
  })
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const [recentFolders, setRecentFolders] = useState<RecentItem[]>(() => {
    if (typeof window === 'undefined') return []
    try {
      const stored = window.localStorage.getItem('focal.recentFolders')
      return stored ? JSON.parse(stored) as RecentItem[] : []
    } catch {
      return []
    }
  })
  const [recentFiles, setRecentFiles] = useState<RecentItem[]>(() => {
    if (typeof window === 'undefined') return []
    try {
      const stored = window.localStorage.getItem('focal.recentFiles')
      return stored ? JSON.parse(stored) as RecentItem[] : []
    } catch {
      return []
    }
  })
  const editorRef = useRef<EditorHandle>(null)
  const [lintDiagnostics, setLintDiagnostics] = useState<Diagnostic[]>([])
  const saveRequestId = useRef(0)

  const headings = useMemo(() => extractHeadings(content), [content])
  const activeHeading = useMemo(
    () => activeHeadingLine(headings, cursorLine),
    [headings, cursorLine]
  )

  const loadTree = useCallback(async (targetPath: string) => {
    const tree = await window.electronAPI.readDir(targetPath, { showHidden: showHiddenFiles })
    setFiles(tree)
  }, [showHiddenFiles])

  const openFolder = useCallback(async (path?: string) => {
    const targetPath = path ?? (await window.electronAPI.openFolder())
    if (!targetPath) return
    setFolderPath(targetPath)
    await loadTree(targetPath)
    setActiveFile(null)
    setContent('')
    const label = targetPath.split('/').pop() ?? targetPath
    setRecentFolders((items) => pushRecent(items, { path: targetPath, label }))
  }, [loadTree])

  const selectFile = useCallback(async (filePath: string) => {
    if (saveTimer.current) {
      clearTimeout(saveTimer.current)
      saveTimer.current = null
    }
    const raw = await window.electronAPI.readFile(filePath)
    if (raw !== null) {
      const parentPath = filePath.split('/').slice(0, -1).join('/')
      if (folderPath !== parentPath) {
        setFolderPath(parentPath)
        await loadTree(parentPath)
      }
      setActiveFile(filePath)
      setContent(raw)
      setSaveStatus('saved')
      setRecentFiles((items) => pushRecent(items, {
        path: filePath,
        label: filePath.split('/').pop() ?? filePath,
        parentPath,
      }))
      setRecentFolders((items) => pushRecent(items, {
        path: parentPath,
        label: parentPath.split('/').pop() ?? parentPath,
      }))
    }
  }, [folderPath, loadTree])

  const handleContentChange = useCallback((newContent: string) => {
    setContent(newContent)
    setSaveStatus('unsaved')
    if (saveTimer.current) clearTimeout(saveTimer.current)
    const requestId = ++saveRequestId.current
    saveTimer.current = setTimeout(async () => {
      if (activeFile) {
        setSaveStatus('saving')
        await window.electronAPI.writeFile(activeFile, newContent)
        if (saveRequestId.current === requestId) {
          setSaveStatus('saved')
        }
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
      await loadTree(folderPath)
      await selectFile(filePath)
    }
  }, [folderPath, loadTree, openFolder, selectFile])

  const refreshFiles = useCallback(async () => {
    if (!folderPath) return
    await loadTree(folderPath)
  }, [folderPath, loadTree])

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

  useEffect(() => {
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current)
    }
  }, [])

  useEffect(() => {
    applyTheme(themeId)
    window.localStorage.setItem('focal.theme', themeId)
  }, [themeId])

  useEffect(() => {
    applyFontPreset(fontPresetId)
    window.localStorage.setItem('focal.fontPreset', fontPresetId)
  }, [fontPresetId])

  useEffect(() => {
    window.localStorage.setItem('focal.showHiddenFiles', String(showHiddenFiles))
  }, [showHiddenFiles])

  useEffect(() => {
    window.localStorage.setItem('focal.recentFolders', JSON.stringify(recentFolders))
  }, [recentFolders])

  useEffect(() => {
    window.localStorage.setItem('focal.recentFiles', JSON.stringify(recentFiles))
  }, [recentFiles])

  useEffect(() => {
    if (!folderPath) return
    loadTree(folderPath)
  }, [folderPath, loadTree])

  const folderName = folderPath ? folderPath.split('/').pop() ?? folderPath : ''
  const activeFileName = activeFile ? activeFile.split('/').pop() ?? activeFile : null
  const activeFileRelativePath = relativePath(folderPath, activeFile)
  const wordCount = countWords(content)
  const fileCount = useMemo(() => countFiles(files), [files])
  const folderCount = useMemo(() => countFolders(files), [files])
  const outlineCount = headings.length
  const workspaceTitle = (activeFileName ?? folderName) || 'focal'
  const activeTheme = themeOptions.find((theme) => theme.id === themeId)
  const activeFontPreset = fontPresetOptions.find((preset) => preset.id === fontPresetId)

  const commands = [
    { id: 'open-folder', label: 'Open Folder', shortcut: '⌘O', icon: 'Open', action: () => openFolder() },
    { id: 'new-file', label: 'New File', shortcut: '⌘N', icon: 'New', action: createNewFile },
    { id: 'toggle-focus', label: focusMode ? 'Exit Focus Mode' : 'Enter Focus Mode', shortcut: '⌘⇧F', icon: 'Focus', action: () => setFocusMode(f => !f) },
    { id: 'toggle-sidebar', label: sidebarVisible ? 'Hide Sidebar' : 'Show Sidebar', shortcut: '⌘\\', icon: 'Pane', action: () => setSidebarVisible(v => !v) },
    { id: 'toggle-hidden-files', label: showHiddenFiles ? 'Hide Hidden Files' : 'Show Hidden Files', shortcut: '', icon: 'Hidden', action: () => setShowHiddenFiles(v => !v) },
    { id: 'show-outline', label: 'Show Outline', shortcut: '', icon: 'Map', action: () => { setSidebarPanel('outline'); setSidebarVisible(true) } },
    { id: 'show-files', label: 'Show Files', shortcut: '', icon: 'Tree', action: () => { setSidebarPanel('files'); setSidebarVisible(true) } },
    ...themeOptions.map((theme) => ({
      id: `theme-${theme.id}`,
      label: `Theme: ${theme.label}`,
      shortcut: '',
      icon: 'Theme',
      action: () => setThemeId(theme.id),
    })),
    ...fontPresetOptions.map((preset) => ({
      id: `font-${preset.id}`,
      label: `Typeface: ${preset.label}`,
      shortcut: '',
      icon: 'Type',
      action: () => setFontPresetId(preset.id),
    })),
    ...recentFolders.map((folder) => ({
      id: `recent-folder-${folder.path}`,
      label: `Recent Folder: ${folder.label}`,
      shortcut: '',
      icon: 'Open',
      action: () => openFolder(folder.path),
    })),
    ...recentFiles.map((file) => ({
      id: `recent-file-${file.path}`,
      label: `Recent File: ${file.label}`,
      shortcut: '',
      icon: 'File',
      action: () => selectFile(file.path),
    })),
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
                showHiddenFiles={showHiddenFiles}
                onFileSelect={selectFile}
                onRefresh={refreshFiles}
                onToggleShowHidden={() => setShowHiddenFiles((value) => !value)}
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
          <div className="workspace-banner">
            <div className="workspace-banner-copy">
              <span className="workspace-banner-label">
                {activeFile ? 'Current document' : folderPath ? 'Workspace ready' : 'Focused markdown editor'}
              </span>
              <div className="workspace-banner-title-row">
                <h1 className="workspace-banner-title">{workspaceTitle}</h1>
                <span className="workspace-banner-theme-pill">
                  {activeTheme?.label}
                  {activeFontPreset ? ` · ${activeFontPreset.label}` : ''}
                </span>
                {activeFileRelativePath && activeFileRelativePath !== activeFileName && (
                  <span className="workspace-banner-path">{activeFileRelativePath}</span>
                )}
              </div>
            </div>
            <div className="workspace-banner-metrics">
              <div className="workspace-banner-metric">
                <span className="workspace-banner-metric-value">{folderPath ? fileCount : '∞'}</span>
                <span className="workspace-banner-metric-label">{folderPath ? 'notes' : 'blank page'}</span>
              </div>
              <div className="workspace-banner-metric">
                <span className="workspace-banner-metric-value">{activeFile ? wordCount : folderPath ? folderCount : '⌘K'}</span>
                <span className="workspace-banner-metric-label">{activeFile ? 'words' : folderPath ? 'folders' : 'command palette'}</span>
              </div>
              <div className="workspace-banner-metric">
                <span className="workspace-banner-metric-value">{activeFile ? formatReadingTime(wordCount) : outlineCount || 'outline'}</span>
                <span className="workspace-banner-metric-label">{activeFile ? 'reading pace' : 'structure'}</span>
              </div>
            </div>
          </div>
          <div className="editor-toolbar">
            <label className="editor-toolbar-select-wrap" title="Theme">
              <span>Theme</span>
              <select
                className="editor-toolbar-select"
                value={themeId}
                onChange={(e) => setThemeId(e.target.value as ThemeId)}
              >
                {Array.from(new Set(themeOptions.map((theme) => theme.family))).map((family) => (
                  <optgroup key={family} label={family}>
                    {themeOptions
                      .filter((theme) => theme.family === family)
                      .map((theme) => (
                        <option key={theme.id} value={theme.id}>
                          {theme.label}
                        </option>
                      ))}
                  </optgroup>
                ))}
              </select>
            </label>
            <label className="editor-toolbar-select-wrap" title="Typeface">
              <span>Type</span>
              <select
                className="editor-toolbar-select"
                value={fontPresetId}
                onChange={(e) => setFontPresetId(e.target.value as FontPresetId)}
              >
                {fontPresetOptions.map((preset) => (
                  <option key={preset.id} value={preset.id}>
                    {preset.label}
                  </option>
                ))}
              </select>
            </label>
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
            <div className="editor-wrapper">
              <Editor
                ref={editorRef}
                content={content}
                focusMode={focusMode}
                onChange={handleContentChange}
                onCursorChange={(l, c) => { setCursorLine(l); setCursorCol(c) }}
                onLintResults={setLintDiagnostics}
              />
            </div>
          ) : (
            <div className="editor-empty">
              <div className="editor-empty-hero">
                <span className="editor-empty-eyebrow">{folderPath ? 'Workspace loaded' : 'Quiet by design'}</span>
                <span className="editor-empty-wordmark">focal</span>
                <p className="editor-empty-copy">
                  {folderPath
                    ? 'Pick a note from the sidebar or start a fresh draft. The editor stays minimal while the structure remains one click away.'
                    : 'Open a notes folder and write in a space that keeps formatting elegant, navigation nearby, and the chrome out of your way.'}
                </p>
                <div className="editor-empty-actions">
                  <button className="editor-empty-primary" onClick={() => (folderPath ? createNewFile() : openFolder())}>
                    {folderPath ? 'Create a new note' : 'Open a folder'}
                  </button>
                  <button className="editor-empty-secondary" onClick={() => setPaletteOpen(true)}>
                    Browse commands
                  </button>
                </div>
                <div className="editor-empty-shortcuts">
                  <span><kbd>⌘K</kbd> command palette</span>
                  <span><kbd>⌘\\</kbd> toggle sidebar</span>
                  <span><kbd>⌘⇧F</kbd> focus mode</span>
                </div>
                {(recentFolders.length > 0 || recentFiles.length > 0) && (
                  <div className="editor-empty-recents">
                    <span className="editor-empty-recents-label">Recent</span>
                    <div className="editor-empty-recents-list">
                      {recentFolders.slice(0, 3).map((folder) => (
                        <button
                          key={folder.path}
                          className="editor-empty-recent"
                          onClick={() => openFolder(folder.path)}
                        >
                          <span className="editor-empty-recent-kind">Folder</span>
                          <span className="editor-empty-recent-name">{folder.label}</span>
                        </button>
                      ))}
                      {recentFiles.slice(0, 3).map((file) => (
                        <button
                          key={file.path}
                          className="editor-empty-recent"
                          onClick={() => selectFile(file.path)}
                        >
                          <span className="editor-empty-recent-kind">File</span>
                          <span className="editor-empty-recent-name">{file.label}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </div>
      </div>

      <StatusBar
        fileName={activeFileName}
        line={cursorLine}
        col={cursorCol}
        wordCount={wordCount}
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
