export interface HeadingNode {
  level: number
  text: string
  line: number
  children: HeadingNode[]
}

export function extractHeadings(content: string): HeadingNode[] {
  const lines = content.split('\n')
  const flat: Omit<HeadingNode, 'children'>[] = []

  let inFence = false
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i]
    if (raw.trimStart().startsWith('```') || raw.trimStart().startsWith('~~~')) {
      inFence = !inFence
      continue
    }
    if (inFence) continue

    const m = raw.match(/^(#{1,6})\s+(.+)$/)
    if (m) {
      flat.push({ level: m[1].length, text: m[2].trimEnd(), line: i + 1 })
    }
  }

  return buildTree(flat, 0, flat.length, 1).nodes
}

function buildTree(
  flat: Omit<HeadingNode, 'children'>[],
  start: number,
  end: number,
  minLevel: number,
): { nodes: HeadingNode[]; consumed: number } {
  const nodes: HeadingNode[] = []
  let i = start

  while (i < end) {
    const h = flat[i]
    if (h.level < minLevel) break

    const node: HeadingNode = { ...h, children: [] }
    i++

    const { nodes: children, consumed } = buildTree(flat, i, end, h.level + 1)
    node.children = children
    i += consumed

    nodes.push(node)
  }

  return { nodes, consumed: i - start }
}
