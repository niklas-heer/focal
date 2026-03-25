import type { HeadingNode } from '@/lib/outline'

interface OutlineProps {
  headings: HeadingNode[]
  activeLine: number
  onJump: (line: number) => void
}

function OutlineNode({
  node,
  depth,
  activeLine,
  onJump,
}: {
  node: HeadingNode
  depth: number
  activeLine: number
  onJump: (line: number) => void
}) {
  const isActive = activeLine === node.line
  const indent = depth * 12

  return (
    <div className="outline-node">
      <div
        className={`outline-item${isActive ? ' active' : ''} outline-h${node.level}`}
        style={{ paddingLeft: `${12 + indent}px` }}
        onClick={() => onJump(node.line)}
        title={node.text}
      >
        {node.level > 1 && (
          <span className="outline-item-indent-bar" style={{ left: `${8 + (depth - 1) * 12}px` }} />
        )}
        <span className="outline-item-text">{node.text}</span>
      </div>
      {node.children.length > 0 && (
        <div className="outline-children">
          {node.children.map((child, i) => (
            <OutlineNode
              key={`${child.line}-${i}`}
              node={child}
              depth={depth + 1}
              activeLine={activeLine}
              onJump={onJump}
            />
          ))}
        </div>
      )}
    </div>
  )
}

export function Outline({ headings, activeLine, onJump }: OutlineProps) {
  if (headings.length === 0) {
    return (
      <div className="outline-empty">
        <span className="outline-empty-text">No headings</span>
      </div>
    )
  }

  return (
    <div className="outline-list">
      {headings.map((node, i) => (
        <OutlineNode
          key={`${node.line}-${i}`}
          node={node}
          depth={0}
          activeLine={activeLine}
          onJump={onJump}
        />
      ))}
    </div>
  )
}
