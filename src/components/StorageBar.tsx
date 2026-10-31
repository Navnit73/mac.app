import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { CATEGORY_COLOR, CATEGORY_ROUTE } from '../lib/colors'
import { formatBytes, formatPercent } from '../lib/format'
import type { OverviewSummary } from '../types/bindings/OverviewSummary'

interface Segment {
  key: string
  label: string
  bytes: number
  color: string
  route?: string
  hatched?: boolean
  note?: string
}

function buildSegments(o: OverviewSummary): { segments: Segment[]; free: Segment } {
  const segments: Segment[] = [
    {
      key: 'kept',
      label: 'Kept',
      bytes: o.kept_bytes,
      color: 'var(--kept)',
      route: '/space',
      note: 'Files MyMacStorage does not suggest clearing.',
    },
    ...o.categories
      .filter((c) => c.analyzed && c.bytes > 0)
      .map((c) => ({
        key: c.id,
        label: c.name,
        bytes: c.bytes,
        color: CATEGORY_COLOR[c.id],
        route: CATEGORY_ROUTE[c.id],
        note: c.blurb,
      })),
    {
      key: 'other',
      label: o.covers_volume ? 'System & other' : 'Outside the scan',
      bytes: o.system_other_bytes,
      color: 'var(--other)',
      hatched: true,
      note: o.covers_volume
        ? 'macOS, purgeable space, local snapshots and anything the scan could not read.'
        : 'Used space in folders you did not scan, plus system files.',
    },
  ]
  const free: Segment = {
    key: 'free',
    label: 'Free',
    bytes: o.volume.available_important,
    color: 'var(--track)',
    note: 'Includes space macOS can purge when needed.',
  }
  return { segments: segments.filter((s) => s.bytes > 0), free }
}

export default function StorageBar({ overview }: { overview: OverviewSummary }) {
  const navigate = useNavigate()
  const [hover, setHover] = useState<string | null>(null)
  const { segments, free } = buildSegments(overview)
  const all = [...segments, free]
  const sum = all.reduce((a, s) => a + s.bytes, 0)
  const denom = Math.max(overview.volume.total, sum, 1)

  return (
    <div>
      <div
        className="flex h-8 gap-0.5 overflow-hidden rounded-lg bg-panel"
        role="img"
        aria-label={`Storage: ${all.map((s) => `${s.label} ${formatBytes(s.bytes)}`).join(', ')}`}
      >
        {all.map((s) => (
          <button
            key={s.key}
            type="button"
            tabIndex={s.route ? 0 : -1}
            onMouseEnter={() => setHover(s.key)}
            onMouseLeave={() => setHover(null)}
            onFocus={() => setHover(s.key)}
            onBlur={() => setHover(null)}
            onClick={() => s.route && navigate(s.route)}
            title={`${s.label}: ${formatBytes(s.bytes)} (${formatPercent(s.bytes / denom)})`}
            className="h-full transition-opacity"
            style={{
              width: `${(s.bytes / denom) * 100}%`,
              minWidth: 4,
              outline: s.key === 'free' ? '1px solid var(--line-strong)' : undefined,
              outlineOffset: -1,
              borderRadius: s.key === 'free' ? '0 8px 8px 0' : undefined,
              background:
                s.key === 'free'
                  ? 'transparent'
                  : s.hatched
                    ? `repeating-linear-gradient(135deg, ${s.color} 0 5px, color-mix(in srgb, ${s.color} 55%, transparent) 5px 10px)`
                    : s.color,
              opacity: hover && hover !== s.key ? 0.45 : 1,
              cursor: s.route ? 'pointer' : 'default',
            }}
          />
        ))}
      </div>

      <ul className="mt-4 flex flex-wrap gap-x-6 gap-y-2">
        {all.map((s) => (
          <li key={s.key}>
            <button
              type="button"
              className="flex items-center gap-2 rounded px-1 text-left"
              onMouseEnter={() => setHover(s.key)}
              onMouseLeave={() => setHover(null)}
              onClick={() => s.route && navigate(s.route)}
              title={s.note}
            >
              <span
                className="h-2.5 w-2.5 shrink-0 rounded-[3px]"
                style={{
                  background: s.color,
                  border: s.key === 'free' ? '1px solid var(--line-strong)' : undefined,
                }}
              />
              <span className="text-muted">{s.label}</span>
              <span className="num font-semibold">{formatBytes(s.bytes)}</span>
            </button>
          </li>
        ))}
      </ul>
    </div>
  )
}
