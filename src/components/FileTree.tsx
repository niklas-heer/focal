import { useState, useRef, useCallback, useEffect } from 'react'
import type { FileNode } from '@/types'

interface FileTreeProps {
  files: FileNode[]
  activeFile: string | null
  folderName: string
  folderPath: string
  onFileSelect: (path: string) => void
  onRefresh: () => void
}

interface ContextMenu {
  x: number
  y: number
  node: FileNode
}

function FileIcon({ type }: { type: 'file' | 'directory' }) {
  if (type === 'directory') {
    return (
      <svg className="file-tree-item-icon" width="14" height="14" viewBox="0 0 16 16" fill="none">
        <path d="M1 4.5C1 3.67 1.67 3 2.5 3H6l1.5 1.5H13.5C14.33 4.5 15 5.17 15 6v6.5C15 13.33 14.33 14 13.5 14h-11C1.67 14 1 13.33 1 12.5v-8z" fill="var(--tn-comment)" opacity="0.7"/>
      </svg>
    )
  }
  return (
    <svg className="file-tree-item-icon" width="14" height="14" viewBox="0 0 16 16" fill="none">
      <path d="M3 1.5C3 1.22 3.22 1 3.5 1H9.5L13 4.5v10c0 .28-.22.5-.5.5h-9C3.22 15 3 14.78 3 14.5v-13z" fill="var(--tn-bg-highlight)" stroke="var(--tn-fg-gutter)" strokeWidth="1"/>
      <path d="M9.5 1L13 4.5H9.5V1z" fill="var(--tn-fg-gutter)"/>
    </svg>
  )
}

function FileTreeNode({
  node,
  activeFile,
  depth,
  dragOverPath,
  onFileSelect,
  onContextMenu,
  onDragStart,
  onDragOver,
  onDrop,
}: {
  node: FileNode
  activeFile: string | null
  depth: number
  dragOverPath: string | null
  onFileSelect: (path: string) => void
  onContextMenu: (e: React.MouseEvent, node: FileNode) => void
  onDragStart: (e: React.DragEvent, node: FileNode) => void
  onDragOver: (e: React.DragEvent, path: string) => void
  onDrop: (e: React.DragEvent, targetNode: FileNode) => void
}) {
  const [expanded, setExpanded] = useState(true)
  const isDragOver = dragOverPath === node.path

  const itemStyle: React.CSSProperties = {
    paddingLeft: `${12 + depth * 12}px`,
    ...(isDragOver ? { background: 'rgba(122, 162, 247, 0.12)', outline: '1px solid rgba(122, 162, 247, 0.3)' } : {}),
  }

  if (node.type === 'directory') {
    return (
      <div className="file-tree-node">
        <div
          className="file-tree-item"
          style={itemStyle}
          onClick={() => setExpanded(e => !e)}
          onContextMenu={e => onContextMenu(e, node)}
          draggable
          onDragStart={e => onDragStart(e, node)}
          onDragOver={e => onDragOver(e, node.path)}
          onDrop={e => onDrop(e, node)}
        >
          <svg className="file-tree-item-icon" width="10" height="10" viewBox="0 0 10 10" fill="none"
            style={{ transform: expanded ? 'rotate(90deg)' : 'rotate(0deg)', transition: 'transform 0.15s', flexShrink: 0 }}>
            <path d="M3 2L7 5L3 8" stroke="var(--tn-comment)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/>
          </svg>
          <span className="file-tree-item-name">{node.name}</span>
        </div>
        {expanded && node.children && (
          <div className="file-tree-dir-children">
            {node.children.map(child => (
              <FileTreeNode
                key={child.path}
                node={child}
                activeFile={activeFile}
                depth={depth + 1}
                dragOverPath={dragOverPath}
                onFileSelect={onFileSelect}
                onContextMenu={onContextMenu}
                onDragStart={onDragStart}
                onDragOver={onDragOver}
                onDrop={onDrop}
              />
            ))}
          </div>
        )}
      </div>
    )
  }

  const isActive = node.path === activeFile
  return (
    <div
      className={`file-tree-item${isActive ? ' active' : ''}`}
      style={itemStyle}
      onClick={() => onFileSelect(node.path)}
      onContextMenu={e => onContextMenu(e, node)}
      draggable
      onDragStart={e => onDragStart(e, node)}
    >
      <FileIcon type="file" />
      <span className="file-tree-item-name">{node.name.replace(/\.(md|markdown)$/, '')}</span>
    </div>
  )
}

export function FileTree({ files, activeFile, folderName, onFileSelect, onRefresh }: FileTreeProps) {
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null)
  const [renaming, setRenaming] = useState<string | null>(null)
  const [renameValue, setRenameValue] = useState('')
  const [dragOverPath, setDragOverPath] = useState<string | null>(null)
  const renameInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (renaming && renameInputRef.current) {
      renameInputRef.current.focus()
      renameInputRef.current.select()
    }
  }, [renaming])

  useEffect(() => {
    const close = () => setContextMenu(null)
    window.addEventListener('click', close)
    window.addEventListener('contextmenu', close)
    return () => { window.removeEventListener('click', close); window.removeEventListener('contextmenu', close) }
  }, [])

  const handleContextMenu = useCallback((e: React.MouseEvent, node: FileNode) => {
    e.preventDefault()
    e.stopPropagation()
    setContextMenu({ x: e.clientX, y: e.clientY, node })
  }, [])

  const startRename = useCallback((node: FileNode) => {
    setContextMenu(null)
    setRenaming(node.path)
    setRenameValue(node.name.replace(/\.(md|markdown)$/, ''))
  }, [])

  const commitRename = useCallback(async () => {
    if (!renaming || !renameValue.trim()) { setRenaming(null); return }
    await window.electronAPI.renameFile(renaming, renameValue.trim())
    setRenaming(null)
    onRefresh()
  }, [renaming, renameValue, onRefresh])

  const handleDelete = useCallback(async (node: FileNode) => {
    setContextMenu(null)
    const confirmed = window.confirm(`Delete "${node.name}"? This cannot be undone.`)
    if (!confirmed) return
    await window.electronAPI.deleteFile(node.path)
    onRefresh()
  }, [onRefresh])

  const handleNewFile = useCallback(async (dirPath: string) => {
    setContextMenu(null)
    const name = `untitled-${Date.now()}.md`
    await window.electronAPI.createFile(dirPath, name)
    onRefresh()
  }, [onRefresh])

  const handleDragStart = useCallback((e: React.DragEvent, node: FileNode) => {
    e.dataTransfer.setData('text/plain', node.path)
    e.dataTransfer.effectAllowed = 'move'
  }, [])

  const handleDragOver = useCallback((e: React.DragEvent, path: string) => {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    setDragOverPath(path)
  }, [])

  const handleDrop = useCallback(async (e: React.DragEvent, targetNode: FileNode) => {
    e.preventDefault()
    setDragOverPath(null)
    const srcPath = e.dataTransfer.getData('text/plain')
    if (!srcPath || srcPath === targetNode.path) return
    const destDir = targetNode.type === 'directory' ? targetNode.path : targetNode.path.split('/').slice(0, -1).join('/')
    await window.electronAPI.moveFile(srcPath, destDir)
    onRefresh()
  }, [onRefresh])

  const handleDragLeave = useCallback(() => {
    setDragOverPath(null)
  }, [])

  return (
    <>
      <div className="sidebar-header">
        <svg className="sidebar-folder-icon" width="14" height="14" viewBox="0 0 16 16" fill="none">
          <path d="M1 4.5C1 3.67 1.67 3 2.5 3H6l1.5 1.5H13.5C14.33 4.5 15 5.17 15 6v6.5C15 13.33 14.33 14 13.5 14h-11C1.67 14 1 13.33 1 12.5v-8z" fill="var(--tn-comment)"/>
        </svg>
        <span className="sidebar-folder-name">{folderName}</span>
      </div>

      <div className="file-tree" onDragLeave={handleDragLeave}>
        {files.map(node => (
          renaming === node.path ? (
            <div key={node.path} className="file-tree-item" style={{ paddingLeft: '12px' }}>
              <input
                ref={renameInputRef}
                className="file-tree-rename-input"
                value={renameValue}
                onChange={e => setRenameValue(e.target.value)}
                onBlur={commitRename}
                onKeyDown={e => { if (e.key === 'Enter') commitRename(); if (e.key === 'Escape') setRenaming(null) }}
              />
            </div>
          ) : (
            <FileTreeNode
              key={node.path}
              node={node}
              activeFile={activeFile}
              depth={0}
              dragOverPath={dragOverPath}
              onFileSelect={onFileSelect}
              onContextMenu={handleContextMenu}
              onDragStart={handleDragStart}
              onDragOver={handleDragOver}
              onDrop={handleDrop}
            />
          )
        ))}
      </div>

      {contextMenu && (
        <div
          className="context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={e => e.stopPropagation()}
        >
          {contextMenu.node.type === 'directory' && (
            <button className="context-menu-item" onClick={() => handleNewFile(contextMenu.node.path)}>
              New File
            </button>
          )}
          <button className="context-menu-item" onClick={() => startRename(contextMenu.node)}>
            Rename
          </button>
          <div className="context-menu-sep" />
          <button className="context-menu-item danger" onClick={() => handleDelete(contextMenu.node)}>
            Delete
          </button>
        </div>
      )}
    </>
  )
}
