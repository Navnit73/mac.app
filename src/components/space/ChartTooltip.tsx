import { KIND_LABEL } from '../../lib/colors'
import { formatBytes, formatCount, formatPercent } from '../../lib/format'
import type { ChartNode } from '../../types/bindings/ChartNode'

export interface TooltipState {
  x: number
  y: number
  node: ChartNode
}

/** Follows the pointer inside a `position: relative` chart container. */
export default function ChartTooltip({
  tip,
  total,
  width,
}: {
  tip: TooltipState | null
  total: number
  width: number
}) {
  if (!tip) return null
  const { node } = tip
  const flip = tip.x > width - 240
  return (
    <div
      role="tooltip"
      className="pointer-events-none absolute z-20 w-56 rounded-lg border border-line-strong bg-raised p-3"
      style={{ left: flip ? tip.x - 236 : tip.x + 14, top: tip.y + 14 }}
    >
      <p className="truncate font-semibold">{node.name}</p>
      <p className="num mt-1 text-lg font-semibold">{formatBytes(node.value)}</p>
      <p className="num text-muted">
        {formatPercent(total > 0 ? node.value / total : 0)} of this view
      </p>
      <p className="mt-1 text-xs text-muted">
        {node.id === null
          ? `${formatCount(node.other_count)} smaller items`
          : node.is_dir
            ? 'Folder · click to open'
            : KIND_LABEL[node.file_kind]}
      </p>
    </div>
  )
}
