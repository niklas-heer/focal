import { useState, useEffect, useRef, useCallback, KeyboardEvent } from 'react'

interface Command {
  id: string
  label: string
  shortcut?: string
  icon: string
  action: () => void
}

interface CommandPaletteProps {
  isOpen: boolean
  onClose: () => void
  commands: Command[]
}

function fuzzyMatch(query: string, str: string): boolean {
  if (!query) return true
  const q = query.toLowerCase()
  const s = str.toLowerCase()
  let qi = 0
  for (let i = 0; i < s.length && qi < q.length; i++) {
    if (s[i] === q[qi]) qi++
  }
  return qi === q.length
}

export function CommandPalette({ isOpen, onClose, commands }: CommandPaletteProps) {
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)

  const filtered = commands.filter(cmd => fuzzyMatch(query, cmd.label))

  useEffect(() => {
    if (isOpen) {
      setQuery('')
      setSelected(0)
      setTimeout(() => inputRef.current?.focus(), 10)
    }
  }, [isOpen])

  useEffect(() => {
    setSelected(0)
  }, [query])

  const handleKeyDown = useCallback((e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Escape') {
      onClose()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelected(s => Math.min(s + 1, filtered.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelected(s => Math.max(s - 1, 0))
    } else if (e.key === 'Enter') {
      e.preventDefault()
      if (filtered[selected]) {
        filtered[selected].action()
        onClose()
      }
    }
  }, [filtered, selected, onClose])

  if (!isOpen) return null

  return (
    <div className="palette-overlay" onClick={onClose}>
      <div className="palette" onClick={e => e.stopPropagation()}>
        <div className="palette-input-wrapper">
          <svg className="palette-input-icon" width="14" height="14" viewBox="0 0 16 16" fill="none">
            <circle cx="6.5" cy="6.5" r="4.5" stroke="currentColor" strokeWidth="1.5"/>
            <path d="M10.5 10.5L14 14" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
          </svg>
          <input
            ref={inputRef}
            className="palette-input"
            value={query}
            onChange={e => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Type a command…"
            spellCheck={false}
          />
        </div>
        <div className="palette-results">
          {filtered.length === 0 ? (
            <div className="palette-empty">No commands found</div>
          ) : (
            filtered.map((cmd, i) => (
              <div
                key={cmd.id}
                className={`palette-item${i === selected ? ' selected' : ''}`}
                onClick={() => { cmd.action(); onClose() }}
                onMouseEnter={() => setSelected(i)}
              >
                <span className="palette-item-icon">{cmd.icon}</span>
                <span className="palette-item-label">{cmd.label}</span>
                {cmd.shortcut && (
                  <span className="palette-item-shortcut">{cmd.shortcut}</span>
                )}
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  )
}
