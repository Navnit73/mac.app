import { useMemo, useState } from 'react'
import { formatBytes } from '../../lib/format'
import type { ColorBy } from '../../state/spaceStore'
import type { ChartNode } from '../../types/bindings/ChartNode'
import { branchHues, nodeColor, ringColor, sumDepth } from './chartColor'
import ChartTooltip, { type TooltipState } from './ChartTooltip'

const SIZE = 640
const C = SIZE / 2
const HOLE = 78
const OUTER = C - 6
const TAU = Math.PI * 2

interface Arc {
  node: ChartNode
  depth: number
  a0: number
  a1: number
  path: string
}

function polar(r: number, a: number): [number, number] {
  // Angle 0 points up, increasing clockwise.
  return [C + r * Math.sin(a), C - r * Math.cos(a)]
}

function arcPath(r1: number, r2: number, a0: number, a1: number): string {
  const full = a1 - a0 >= TAU - 1e-4
  if (full) {
    const [ox, oy] = polar(r2, 0)
    const [ox2, oy2] = polar(r2, Math.PI)
    const [ix, iy] = polar(r1, 0)
    const [ix2, iy2] = polar(r1, Math.PI)
    return `M${ox} ${oy}A${r2} ${r2} 0 1 1 ${ox2} ${oy2}A${r2} ${r2} 0 1 1 ${ox} ${oy}M${ix} ${iy}A${r1} ${r1} 0 1 0 ${ix2} ${iy2}A${r1} ${r1} 0 1 0 ${ix} ${iy}Z`
  }
  const large = a1 - a0 > Math.PI ? 1 : 0
  const [x1, y1] = polar(r2, a0)
  const [x2, y2] = polar(r2, a1)
  const [x3, y3] = polar(r1, a1)
  const [x4, y4] = polar(r1, a0)
  return `M${x1} ${y1}A${r2} ${r2} 0 ${large} 1 ${x2} ${y2}L${x3} ${y3}A${r1} ${r1} 0 ${large} 0 ${x4} ${y4}Z`
}

function layout(root: ChartNode, rings: number): Arc[] {
  const ringW = Math.min(64, (OUTER - HOLE) / rings)
  const arcs: Arc[] = []
  const walk = (n: ChartNode, depth: number, a0: number, a1: number) => {
    if (depth > 0) {
      const r1 = HOLE + (depth - 1) * ringW
      arcs.push({ node: n, depth, a0, a1, path: arcPath(r1, r1 + ringW - 1.5, a0, a1) })
    }
    let a = a0
    for (const c of n.children) {
      const span = ((a1 - a0) * c.value) / Math.max(n.value, 1)
      if (span > 0.002) walk(c, depth + 1, a, a + span)
      a += span
    }
  }
  walk(root, 0, 0, TAU)
  return arcs
}

interface Props {
  root: ChartNode
  colorBy: ColorBy
  selectedId: number | null
  onZoom: (id: number) => void
  onSelect: (id: number) => void
  onUp?: () => void
}

export default function Sunburst({ root, colorBy, selectedId, onZoom, onSelect, onUp }: Props) {
  const rings = Math.max(1, sumDepth(root) - 1)
  const arcs = useMemo(() => layout(root, rings), [root, rings])
  const hues = useMemo(() => branchHues(root), [root])
  const [tip, setTip] = useState<TooltipState | null>(null)
  const [hover, setHover] = useState<ChartNode | null>(null)

  // The arcs only change with the data, colors or selection. Keeping them out of the tooltip's
  // renders stops every pointer move from re-rendering hundreds of paths. Dimming is in CSS.
  const shapes = useMemo(() => {
    const activate = (n: ChartNode) => {
      if (n.id === null) return
      if (n.is_dir && n.children.length > 0) onZoom(n.id)
      else onSelect(n.id)
    }
    return (
      <g className="sunburst-arcs">
        {arcs.map((a, i) => {
          const selected = a.node.id !== null && a.node.id === selectedId
          return (
            <path
              key={`${a.node.id ?? 'other'}-${i}`}
              d={a.path}
              fillRule="evenodd"
              fill={
                colorBy === 'folder' && a.node.id !== null
                  ? ringColor(hues.get(a.node) ?? 'var(--folder)', a.depth)
                  : nodeColor(a.node, colorBy)
              }
              stroke={selected ? 'var(--fg)' : 'var(--panel)'}
              strokeWidth={selected ? 2.5 : 1.5}
              tabIndex={a.depth === 1 ? 0 : -1}
              role="button"
              aria-label={`${a.node.name}, ${formatBytes(a.node.value)}`}
              style={{ cursor: a.node.id === null ? 'default' : 'pointer' }}
              onMouseMove={(e) => {
                const box = e.currentTarget.ownerSVGElement?.getBoundingClientRect()
                if (!box) return
                setHover(a.node)
                setTip({ x: e.clientX - box.left, y: e.clientY - box.top, node: a.node })
              }}
              onMouseLeave={() => {
                setHover(null)
                setTip(null)
              }}
              onFocus={() => setHover(a.node)}
              onBlur={() => setHover(null)}
              onClick={() => activate(a.node)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  activate(a.node)
                }
              }}
            />
          )
        })}
      </g>
    )
  }, [arcs, hues, colorBy, selectedId, onZoom, onSelect])

  const shown = hover ?? root
  return (
    <div className="relative mx-auto aspect-square w-full max-w-[680px]">
      <svg
        viewBox={`0 0 ${SIZE} ${SIZE}`}
        className="h-full w-full"
        role="group"
        aria-label={`Sunburst chart of ${root.name}`}
      >
        {shapes}
        <g
          onClick={onUp}
          style={{ cursor: onUp ? 'pointer' : 'default' }}
          role={onUp ? 'button' : undefined}
          aria-label={onUp ? 'Go up one folder' : undefined}
        >
          <circle cx={C} cy={C} r={HOLE - 4} fill="var(--panel)" />
          <text x={C} y={C - 6} textAnchor="middle" fontSize="14" fontWeight="600" fill="var(--fg)">
            {shown.name.length > 16 ? `${shown.name.slice(0, 15)}…` : shown.name}
          </text>
          <text x={C} y={C + 14} textAnchor="middle" fontSize="13" fill="var(--muted)">
            {formatBytes(shown.value)}
          </text>
        </g>
      </svg>
      <ChartTooltip tip={tip} total={root.value} width={680} />
    </div>
  )
}
