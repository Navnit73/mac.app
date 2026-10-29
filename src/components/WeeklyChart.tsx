import { useState } from 'react'
import { formatBytes } from '../lib/format'
import type { WeekStat } from '../types/bindings/WeekStat'

const W = 720
const H = 200
const PAD = { top: 16, right: 8, bottom: 26, left: 8 }

function label(weekStart: number): string {
  return new Date(weekStart * 1000).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  })
}

/** Space recovered per week. One series, so no legend; the heading names it. */
export default function WeeklyChart({ weeks }: { weeks: WeekStat[] }) {
  const [hover, setHover] = useState<number | null>(null)
  const max = Math.max(1, ...weeks.map((w) => w.bytes))
  const innerW = W - PAD.left - PAD.right
  const innerH = H - PAD.top - PAD.bottom
  const slot = innerW / Math.max(weeks.length, 1)
  const barW = Math.min(36, slot * 0.6)
  const tip = hover !== null ? weeks[hover] : undefined

  return (
    <div className="relative">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        className="w-full"
        role="img"
        aria-label="Space recovered per week"
      >
        {[0.5, 1].map((f) => (
          <line
            key={f}
            x1={PAD.left}
            x2={W - PAD.right}
            y1={PAD.top + innerH * (1 - f)}
            y2={PAD.top + innerH * (1 - f)}
            stroke="var(--line)"
          />
        ))}
        <line
          x1={PAD.left}
          x2={W - PAD.right}
          y1={PAD.top + innerH}
          y2={PAD.top + innerH}
          stroke="var(--line-strong)"
        />
        {weeks.map((w, i) => {
          const h = w.bytes > 0 ? Math.max(3, (w.bytes / max) * innerH) : 0
          const x = PAD.left + i * slot + (slot - barW) / 2
          const y = PAD.top + innerH - h
          return (
            <g key={w.week_start}>
              {/* Hit target is the full column, larger than the bar. */}
              <rect
                x={PAD.left + i * slot}
                y={PAD.top}
                width={slot}
                height={innerH}
                fill={hover === i ? 'var(--hover)' : 'transparent'}
                onMouseEnter={() => setHover(i)}
                onMouseLeave={() => setHover(null)}
              />
              {h > 0 && (
                <path
                  d={`M${x} ${y + h}V${y + 4}a4 4 0 0 1 4 -4h${barW - 8}a4 4 0 0 1 4 4V${y + h}Z`}
                  fill="var(--s1)"
                  pointerEvents="none"
                />
              )}
              {(i % 2 === weeks.length % 2 || weeks.length <= 8) && (
                <text
                  x={PAD.left + i * slot + slot / 2}
                  y={H - 8}
                  textAnchor="middle"
                  fontSize="11"
                  fill="var(--muted)"
                >
                  {label(w.week_start)}
                </text>
              )}
            </g>
          )
        })}
        <text x={W - PAD.right} y={PAD.top - 4} textAnchor="end" fontSize="11" fill="var(--muted)">
          {formatBytes(max)}
        </text>
      </svg>
      {tip && hover !== null && (
        <div
          role="tooltip"
          className="pointer-events-none absolute top-0 rounded-lg border border-line bg-raised px-3 py-2 shadow-xl"
          style={{
            left: `${((PAD.left + hover * slot + slot / 2) / W) * 100}%`,
            transform: 'translateX(-50%)',
          }}
        >
          <p className="text-xs text-muted">Week of {label(tip.week_start)}</p>
          <p className="num font-semibold">{formatBytes(tip.bytes)}</p>
          <p className="text-xs text-muted">
            {tip.batches} cleanup{tip.batches === 1 ? '' : 's'}
          </p>
        </div>
      )}
    </div>
  )
}
