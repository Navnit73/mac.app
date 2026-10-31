import { useNavigate } from 'react-router-dom'
import { CATEGORY_COLOR, CATEGORY_ROUTE } from '../lib/colors'
import { formatBytes, formatCount } from '../lib/format'
import type { CategorySummary } from '../types/bindings/CategorySummary'
import Icon from './Icon'

const RISK = {
  Safe: { cls: 'badge-safe', label: 'Safe to clear', icon: 'check' },
  Review: { cls: 'badge-review', label: 'Review first', icon: 'eye' },
  Careful: { cls: 'badge-careful', label: 'Be careful', icon: 'warning' },
} as const

export default function CategoryCard({ c }: { c: CategorySummary }) {
  const navigate = useNavigate()
  const risk = RISK[c.risk]
  const dim = !c.analyzed

  return (
    <button
      type="button"
      onClick={() => navigate(CATEGORY_ROUTE[c.id])}
      className={`group card card-link flex flex-col gap-2 p-5 text-left ${dim ? 'opacity-60' : ''}`}
    >
      <div className="flex items-center gap-2">
        <span
          className="h-2.5 w-2.5 rounded-full"
          style={{ background: CATEGORY_COLOR[c.id] }}
          aria-hidden="true"
        />
        <span className="font-semibold">{c.name}</span>
        <Icon
          name="chevron"
          size={14}
          className="ml-auto text-faint transition-colors group-hover:text-fg"
        />
      </div>
      <div className="num mt-1 font-display text-[26px] font-semibold tracking-tight">
        {c.analyzed ? formatBytes(c.bytes) : '—'}
      </div>
      <p className="text-muted">{c.blurb}</p>
      <div className="mt-auto flex items-center gap-2 pt-1">
        {c.analyzed ? (
          <>
            <span className={`badge ${risk.cls}`}>
              <Icon name={risk.icon} size={11} /> {risk.label}
            </span>
            <span className="num text-xs text-muted">{formatCount(c.items)} items</span>
          </>
        ) : (
          <span className="badge bg-track text-muted">Not checked yet</span>
        )}
      </div>
    </button>
  )
}
