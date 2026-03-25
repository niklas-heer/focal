interface StatusBarProps {
  fileName: string | null
  line: number
  col: number
  wordCount: number
  saveStatus: 'saved' | 'saving' | 'unsaved'
  focusMode: boolean
  sidebarVisible: boolean
  lintIssues: number
  onToggleFocus: () => void
  onToggleSidebar: () => void
  onFixLint: () => void
}

export function StatusBar({
  fileName,
  line,
  col,
  wordCount,
  saveStatus,
  focusMode,
  sidebarVisible,
  lintIssues,
  onToggleFocus,
  onToggleSidebar,
  onFixLint,
}: StatusBarProps) {
  const saveLabel = saveStatus === 'saving' ? 'Saving…' : saveStatus === 'saved' ? 'Saved' : ''

  return (
    <div className="status-bar">
      <button
        className={`status-bar-mode-btn${!sidebarVisible ? ' active' : ''}`}
        onClick={onToggleSidebar}
        title="Toggle sidebar (⌘\\)"
      >
        {sidebarVisible ? (
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
            <rect x="1" y="1" width="6" height="14" rx="1" fill="currentColor" opacity="0.5"/>
            <rect x="9" y="1" width="6" height="14" rx="1" fill="currentColor"/>
          </svg>
        ) : (
          <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
            <rect x="1" y="1" width="6" height="14" rx="1" fill="currentColor"/>
            <rect x="9" y="1" width="6" height="14" rx="1" fill="currentColor" opacity="0.5"/>
          </svg>
        )}
      </button>

      {fileName && (
        <>
          <span className="status-bar-sep">·</span>
          <span className="status-bar-item">{fileName}</span>
          <span className="status-bar-sep">·</span>
          <span className="status-bar-item">{line}:{col}</span>
          <span className="status-bar-sep">·</span>
          <span className="status-bar-item">{wordCount} words</span>
        </>
      )}

      {lintIssues > 0 && (
        <>
          <span className="status-bar-sep">·</span>
          <span className="status-bar-item lint-issues">
            <svg width="10" height="10" viewBox="0 0 16 16" fill="none" style={{ marginRight: 4, verticalAlign: 'middle' }}>
              <path d="M8 1L15 14H1L8 1Z" stroke="var(--tn-yellow)" strokeWidth="1.5" fill="none"/>
              <rect x="7.25" y="6" width="1.5" height="4" fill="var(--tn-yellow)"/>
              <circle cx="8" cy="12" r="0.75" fill="var(--tn-yellow)"/>
            </svg>
            {lintIssues} {lintIssues === 1 ? 'issue' : 'issues'}
          </span>
          <button className="status-bar-fix-btn" onClick={onFixLint} title="Auto-fix lint issues">
            fix
          </button>
        </>
      )}

      <button
        className={`status-bar-mode-btn${focusMode ? ' active' : ''}`}
        onClick={onToggleFocus}
        title="Toggle focus mode (⌘⇧F)"
        style={{ marginLeft: 'auto' }}
      >
        focus
      </button>

      {saveLabel && (
        <>
          <span className="status-bar-sep">·</span>
          <span className={`status-bar-item save-status ${saveStatus}`}>{saveLabel}</span>
        </>
      )}
    </div>
  )
}
