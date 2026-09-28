import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'

export interface HeadingNode {
  level: number
  text: string
  line: number
  children: HeadingNode[]
}

interface MarkdownNode {
  type: string
  depth?: number
  value?: string
  alt?: string
  children?: MarkdownNode[]
  position?: {
    start: { line: number }
  }
}

const parser = unified().use(remarkParse).use(remarkGfm)

function extractText(nodes: MarkdownNode[] | undefined): string {
  if (!nodes || nodes.length === 0) return ''
  return nodes.map((node) => {
    switch (node.type) {
      case 'text':
      case 'inlineCode':
        return node.value ?? ''
      case 'image':
        return node.alt ?? ''
      case 'break':
        return ' '
      default:
        return extractText(node.children)
    }
  }).join('').replace(/\s+/g, ' ').trim()
}

function collectHeadings(node: MarkdownNode, out: Omit<HeadingNode, 'children'>[]) {
  if (node.type === 'heading') {
    out.push({
      level: Math.min(Math.max(node.depth ?? 1, 1), 6),
      text: extractText(node.children),
      line: node.position?.start.line ?? 1,
    })
  }

  for (const child of node.children ?? []) {
    collectHeadings(child, out)
  }
}

export function extractHeadings(content: string): HeadingNode[] {
  const tree = parser.parse(content) as MarkdownNode
  const flat: Omit<HeadingNode, 'children'>[] = []
  collectHeadings(tree, flat)

  const root: HeadingNode[] = []
  const stack: HeadingNode[] = []

  for (const heading of flat) {
    const node: HeadingNode = { ...heading, children: [] }

    while (stack.length > 0 && stack[stack.length - 1].level >= node.level) {
      stack.pop()
    }

    if (stack.length === 0) root.push(node)
    else stack[stack.length - 1].children.push(node)

    stack.push(node)
  }

  return root
}
