import type { EditorView } from '@codemirror/view'
import type { Diagnostic } from '@codemirror/lint'

interface MarkdownlintResult {
  lineNumber: number
  ruleNames: string[]
  ruleDescription: string
  errorDetail: string | null
  fixInfo: {
    lineNumber?: number
    editColumn?: number
    deleteCount?: number
    insertText?: string
  } | null
}

type LintResults = Record<string, MarkdownlintResult[]>

let lintSync: ((opts: { strings: Record<string, string>; config: Record<string, unknown> }) => LintResults) | null = null

async function getLintSync() {
  if (!lintSync) {
    const mod = await import('markdownlint/sync')
    lintSync = mod.lint
  }
  return lintSync
}

const IGNORED_RULES = new Set([
  'MD013',
  'MD033',
  'MD041',
])

const LINT_CONFIG: Record<string, unknown> = {
  default: true,
  MD013: false,
  MD033: false,
  MD041: false,
}

export function buildMarkdownLinter(
  onResults: (diagnostics: Diagnostic[]) => void
) {
  return async function markdownLinter(view: EditorView): Promise<Diagnostic[]> {
    const fn = await getLintSync()
    const content = view.state.doc.toString()

    let results: LintResults
    try {
      results = fn({ strings: { doc: content }, config: LINT_CONFIG })
    } catch {
      return []
    }

    const issues = results['doc'] ?? []
    const diagnostics: Diagnostic[] = []

    for (const issue of issues) {
      if (IGNORED_RULES.has(issue.ruleNames[0])) continue

      const lineNum = Math.min(issue.lineNumber, view.state.doc.lines)
      const line = view.state.doc.line(lineNum)

      let from = line.from
      let to = line.to

      if (issue.fixInfo?.editColumn != null) {
        from = line.from + Math.max(0, issue.fixInfo.editColumn - 1)
        to = issue.fixInfo.deleteCount != null
          ? Math.min(from + issue.fixInfo.deleteCount, line.to)
          : Math.min(from + 1, line.to)
      }

      diagnostics.push({
        from,
        to,
        severity: 'warning',
        message: `${issue.ruleNames[0]}: ${issue.ruleDescription}${issue.errorDetail ? ` — ${issue.errorDetail}` : ''}`,
        source: 'markdownlint',
      })
    }

    onResults(diagnostics)
    return diagnostics
  }
}

export function applyMarkdownFixes(content: string): string {
  if (!lintSync) return content

  let fixed = content
  let changed = true
  let iterations = 0

  while (changed && iterations < 10) {
    changed = false
    iterations++
    let results: LintResults
    try {
      results = lintSync({ strings: { doc: fixed }, config: LINT_CONFIG })
    } catch {
      break
    }

    const issues = results['doc'] ?? []
    const fixable = issues
      .filter(i => i.fixInfo != null && !IGNORED_RULES.has(i.ruleNames[0]))
      .sort((a, b) => (b.fixInfo?.lineNumber ?? b.lineNumber) - (a.fixInfo?.lineNumber ?? a.lineNumber))

    if (fixable.length === 0) break

    const lines = fixed.split('\n')
    for (const issue of fixable) {
      const lineIdx = (issue.fixInfo?.lineNumber ?? issue.lineNumber) - 1
      if (lineIdx < 0 || lineIdx >= lines.length) continue

      const fix = issue.fixInfo!
      if (fix.deleteCount != null && fix.editColumn != null) {
        const col = fix.editColumn - 1
        const line = lines[lineIdx]
        lines[lineIdx] = line.slice(0, col) + (fix.insertText ?? '') + line.slice(col + fix.deleteCount)
        changed = true
      } else if (fix.insertText != null && fix.editColumn == null) {
        lines[lineIdx] = fix.insertText
        changed = true
      } else if (fix.deleteCount === -1) {
        lines.splice(lineIdx, 1)
        changed = true
      }
    }
    fixed = lines.join('\n')
  }

  return fixed
}
