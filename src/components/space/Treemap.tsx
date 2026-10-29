import { useLayoutEffect, useMemo, useRef, useState } from 'react'
import { formatBytes } from '../../lib/format'
import { squarify, type Rect } from '../../lib/squarify'
import type { ColorBy } from '../../state/spaceStore'
import type { ChartNode } from '../../types/bindings/ChartNode'
import { branchHues, nodeColor } from './chartColor'
import ChartTooltip, { type TooltipState } from './ChartTooltip'

const GAP = 2
const HEADER = 18

interface Tile {
  node: ChartNode
  rect: Rect
  depth: number
  container: boolean
}

function layoutTiles(root: ChartNode, rect: Rect): Tile[] {
  const tiles: Tile[] = []
  const place = (n: ChartNode, r: Rect, depth: number) => {
    const placed = squarify(n.children, r)
    for (const p of placed) {
      const inner: Rect = {
        x: p.rect.x + GAP / 2,
        y: p.rect.y + GAP / 2,
        w: Math.max(0, p.rect.w - GAP),
        h: Math.max(0, p.rect.h - GAP),
      }
      if (inner.w < 3 || inner.h < 3) continue
      const container = p.item.children.length > 0 && inner.w > 80 && inner.h > 56
      tiles.push({ node: p.item, rect: inner, depth, container })
      if (container) {
        place(
          p.item,
          { x: inner.x + 3, y: inner.y + HEADER, w: inner.w - 6, h: inner.h - HEADER - 3 },
          depth + 1,
        )
      }
    }
  }
  place(root, rect, 0)
  return tiles
}

function useSize(ref: React.RefObject<HTMLElement | null>) {
  const [size, setSize] = useState({ w: 0, h: 0 })
  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    // Measure right away, then keep following the element's size.
    const measure = () => {
      const r = el.getBoundingClientRect()
      setSize((prev) =>
        prev.w === r.width && prev.h === r.height ? prev : { w: r.width, h: r.height },
      )
    }
    measure()
    const ro = new ResizeObserver(measure)
    ro.observe(el)
    return () => ro.disconnect()
  }, [ref])
  return size
}

interface Props {
  root: ChartNode
  colorBy: ColorBy
  selectedId: number | null
  onZoom: (id: number) => void
  onSelect: (id: number) => void
}

export default function Treemap({ root, colorBy, selectedId, onZoom, onSelect }: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const { w, h } = useSize(ref)
  const tiles = useMemo(
    () => (w > 0 && h > 0 ? layoutTiles(root, { x: 0, y: 0, w, h }) : []),
    [root, w, h],
  )
  const [tip, setTip] = useState<TooltipState | null>(null)
  const hues = useMemo(() => branchHues(root), [root])

  const activate = (n: ChartNode) => {
    if (n.id === null) return
    if (n.is_dir && n.children.length > 0) onZoom(n.id)
    else onSelect(n.id)
  }

  return (
    <div
      ref={ref}
      className="relative h-[calc(100vh-290px)] min-h-[360px] w-full"
      role="group"
      aria-label={`Treemap of ${root.name}`}
    >
      {tiles.map((t, i) => {
        const color = nodeColor(t.node, colorBy, hues)
        // Folder mode tints containers with their branch hue too; kind and age keep them gray.
        const box = colorBy === 'folder' && t.node.id !== null ? color : 'var(--folder)'
        const selected = t.node.id !== null && t.node.id === selectedId
        const showLabel = t.rect.w > 54 && t.rect.h > 26
        return (
          <button
            key={`${t.node.id ?? 'other'}-${i}`}
            type="button"
            aria-label={`${t.node.name}, ${formatBytes(t.node.value)}`}
            onClick={() => activate(t.node)}
            onMouseMove={(e) => {
              const box = ref.current?.getBoundingClientRect()
              if (box) setTip({ x: e.clientX - box.left, y: e.clientY - box.top, node: t.node })
            }}
            onMouseLeave={() => setTip(null)}
            className="absolute flex flex-col items-start justify-start overflow-hidden rounded-md text-left transition-[filter] hover:brightness-110"
            style={{
              left: t.rect.x,
              top: t.rect.y,
              width: t.rect.w,
              height: t.rect.h,
              // Light tints keep black labels readable on every hue.
              background: t.container
                ? `color-mix(in srgb, ${box} 10%, var(--panel))`
                : `color-mix(in srgb, ${color} 30%, var(--panel))`,
              border: `1px solid ${selected ? 'var(--fg)' : t.container ? `color-mix(in srgb, ${box} 38%, var(--panel))` : `color-mix(in srgb, ${color} 62%, var(--panel))`}`,
              outline: selected ? '1px solid var(--fg)' : undefined,
              zIndex: t.depth,
            }}
          >
            {showLabel && (
              <span className="block truncate px-1.5 pt-0.5 text-xs font-semibold leading-tight">
                {t.node.name}
              </span>
            )}
            {showLabel && !t.container && t.rect.h > 44 && (
              <span className="num block truncate px-1.5 text-xs text-muted">
                {formatBytes(t.node.value)}
              </span>
            )}
          </button>
        )
      })}
      <ChartTooltip tip={tip} total={root.value} width={w} />
    </div>
  )
}
