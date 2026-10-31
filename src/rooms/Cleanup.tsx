import { useEffect, useState } from 'react'
import Icon from '../components/Icon'
import Modal from '../components/Modal'
import ScanButton from '../components/ScanButton'
import { CATEGORY_COLOR } from '../lib/colors'
import { formatAge, formatBytes, formatCount } from '../lib/format'
import { commands } from '../lib/ipc'
import { middleTruncate, tildify } from '../lib/text'
import { useCleanupStore } from '../state/cleanupStore'
import { useScanStore } from '../state/scanStore'
import { useTrashStore } from '../state/trashStore'
import type { CleanupCategory } from '../types/bindings/CleanupCategory'
import type { CleanupItem } from '../types/bindings/CleanupItem'
import type { Risk } from '../types/bindings/Risk'

const RISK: Record<Risk, { cls: string; label: string; icon: 'check' | 'eye' | 'warning' }> = {
  Safe: { cls: 'badge-safe', label: 'Safe', icon: 'check' },
  Review: { cls: 'badge-review', label: 'Review first', icon: 'eye' },
  Careful: { cls: 'badge-careful', label: 'Careful', icon: 'warning' },
}

function RiskBadge({ risk }: { risk: Risk }) {
  const r = RISK[risk]
  return (
    <span className={`badge ${r.cls}`}>
      <Icon name={r.icon} size={11} /> {r.label}
    </span>
  )
}

function ItemRow({
  item,
  scanId,
  onCareful,
}: {
  item: CleanupItem
  scanId: number
  onCareful: (item: CleanupItem) => void
}) {
  const setSelected = useCleanupStore((s) => s.setSelected)
  const toggle = (on: boolean) => {
    if (on && item.risk === 'Careful') onCareful(item)
    else void setSelected(item, on)
  }
  return (
    <li
      className={`group flex items-center gap-3 border-b border-line px-5 py-2.5 last:border-0 ${item.selected ? '' : 'hover:bg-hover'}`}
    >
      {item.report_only ? (
        <span
          className="flex w-[13px] justify-center text-faint"
          title="MyMacStorage leaves this alone"
        >
          <Icon name="lock" size={12} />
        </span>
      ) : (
        <input
          type="checkbox"
          aria-label={`Clear ${item.path}`}
          checked={item.selected}
          onChange={(e) => toggle(e.target.checked)}
        />
      )}
      <div className="min-w-0 flex-1">
        <p className="truncate" title={item.path}>
          <span className="font-medium">{item.name}</span>
          <span className="ml-2 text-xs text-faint">{item.rule_name}</span>
        </p>
        <p className="truncate text-xs text-faint" title={item.path}>
          {middleTruncate(tildify(item.path), 84)}
          {' · '}
          {item.is_dir ? `${formatCount(item.files)} files · ` : ''}
          changed {formatAge(item.mtime).toLowerCase()}
        </p>
        {item.guidance && (
          <p className="mt-0.5 flex items-center gap-1 text-xs text-accent-ink">
            <Icon name="info" size={11} /> {item.guidance}
          </p>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        {item.running_app && (
          <span className="badge badge-review" title="Not selected by default while it is open">
            <Icon name="warning" size={11} /> {item.running_app} is open
          </span>
        )}
        {item.regenerates && !item.report_only && (
          <span
            className="flex items-center gap-1 pr-1 text-xs text-faint"
            title="Apps or tools recreate it"
          >
            <Icon name="regen" size={12} /> Comes back
          </span>
        )}
        <RiskBadge risk={item.risk} />
      </div>
      <button
        type="button"
        className="btn btn-ghost hidden h-6 px-1.5 group-hover:inline-flex"
        aria-label="Reveal in Finder"
        title="Reveal in Finder"
        onClick={() => void commands.revealInFinder(scanId, item.node_id).catch(() => undefined)}
      >
        <Icon name="reveal" size={12} />
      </button>
      <span className="num w-20 text-right font-semibold">{formatBytes(item.bytes)}</span>
    </li>
  )
}

function CategoryCard({
  c,
  scanId,
  onCareful,
}: {
  c: CleanupCategory
  scanId: number
  onCareful: (item: CleanupItem) => void
}) {
  const open = useCleanupStore((s) => s.expanded.has(c.id))
  const list = useCleanupStore((s) => s.lists[c.id])
  const toggleExpanded = useCleanupStore((s) => s.toggleExpanded)
  const selectCategory = useCleanupStore((s) => s.selectCategory)
  const loadItems = useCleanupStore((s) => s.loadItems)
  const empty = c.items === 0
  const share = c.bytes > 0 ? c.selected_bytes / c.bytes : 0

  return (
    <li className={`card overflow-hidden ${empty ? 'opacity-60' : ''}`}>
      <button
        type="button"
        onClick={() => !empty && toggleExpanded(c.id)}
        aria-expanded={open}
        disabled={empty}
        className="flex w-full items-start gap-4 px-5 py-4 text-left hover:bg-hover disabled:hover:bg-transparent"
      >
        <span
          className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg"
          style={{ background: `color-mix(in srgb, ${CATEGORY_COLOR[c.id]} 16%, transparent)` }}
          aria-hidden="true"
        >
          <span className="h-2.5 w-2.5 rounded-full" style={{ background: CATEGORY_COLOR[c.id] }} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[15px] font-semibold">{c.name}</span>
            <RiskBadge risk={c.risk} />
            <span className="num text-xs text-muted">
              {empty ? 'Nothing found' : `${formatCount(c.items)} items`}
            </span>
          </div>
          <p className="mt-0.5 text-muted">{c.blurb}</p>
          {!empty && (
            <div className="mt-2.5 flex items-center gap-2.5">
              <div className="h-1.5 w-48 overflow-hidden rounded-full bg-track" aria-hidden="true">
                <div
                  className="h-full rounded-full"
                  style={{ width: `${share * 100}%`, background: CATEGORY_COLOR[c.id] }}
                />
              </div>
              <span className="num text-xs text-muted">
                {formatBytes(c.selected_bytes)} selected
              </span>
            </div>
          )}
        </div>
        <div className="text-right">
          <p className="num font-display text-xl font-semibold tracking-tight">
            {formatBytes(c.bytes)}
          </p>
          {!empty && <p className="text-xs text-faint">found</p>}
        </div>
        {!empty && (
          <Icon
            name="chevron"
            size={14}
            className={`mt-2 text-faint transition-transform ${open ? 'rotate-90' : ''}`}
          />
        )}
      </button>
      {open && (
        <div className="border-t border-line">
          <div className="flex items-center gap-3 border-b border-line bg-bg px-5 py-2 text-xs">
            <span className="text-muted">Select</span>
            <div className="seg text-xs" role="group" aria-label={`Select ${c.name}`}>
              <button type="button" onClick={() => void selectCategory(c.id, 'All')}>
                All
              </button>
              <button
                type="button"
                onClick={() => void selectCategory(c.id, 'Defaults')}
                title="Safe items of apps that are not open"
              >
                Suggested
              </button>
              <button type="button" onClick={() => void selectCategory(c.id, 'None')}>
                None
              </button>
            </div>
            <span className="num ml-auto text-muted">
              {formatCount(c.selected_items)} of {formatCount(c.items)} selected
            </span>
          </div>
          <ul>
            {list?.items.map((it) => (
              <ItemRow key={it.node_id} item={it} scanId={scanId} onCareful={onCareful} />
            ))}
          </ul>
          {list?.loading && <p className="px-5 py-3 text-xs text-muted">Loading…</p>}
          {list && list.nextPage !== null && !list.loading && (
            <button type="button" className="btn m-3" onClick={() => void loadItems(c.id, true)}>
              Show more ({formatCount(list.total - list.items.length)} left)
            </button>
          )}
        </div>
      )}
    </li>
  )
}

/**
 * Where the selected space comes from: one bar, a segment per category in its data color, with
 * a legend that names each one (color is never the only label).
 */
function SelectionBar({ categories, found }: { categories: CleanupCategory[]; found: number }) {
  const parts = categories.filter((c) => c.selected_bytes > 0)
  if (found === 0) return null
  return (
    <div className="flex w-full flex-col gap-2.5 border-t border-line pt-4">
      <div
        className="flex h-2.5 w-full gap-[2px] overflow-hidden rounded-full bg-track"
        role="img"
        aria-label={`Selected: ${parts.map((c) => `${c.name} ${formatBytes(c.selected_bytes)}`).join(', ') || 'nothing'}`}
      >
        {parts.map((c) => (
          <span
            key={c.id}
            title={`${c.name}: ${formatBytes(c.selected_bytes)}`}
            className="h-full"
            style={{
              width: `${(c.selected_bytes / found) * 100}%`,
              minWidth: 3,
              background: CATEGORY_COLOR[c.id],
            }}
          />
        ))}
      </div>
      <ul className="flex flex-wrap gap-x-5 gap-y-1 text-xs">
        {parts.map((c) => (
          <li key={c.id} className="flex items-center gap-1.5">
            <span
              className="h-2 w-2 rounded-full"
              style={{ background: CATEGORY_COLOR[c.id] }}
              aria-hidden="true"
            />
            <span className="text-muted">{c.name}</span>
            <span className="num font-semibold">{formatBytes(c.selected_bytes)}</span>
          </li>
        ))}
        {parts.length === 0 && <li className="text-muted">Nothing selected yet.</li>}
        <li className="ml-auto flex items-center gap-1.5 text-muted">
          <span
            className="h-2 w-2 rounded-full border border-line-strong bg-track"
            aria-hidden="true"
          />
          Not selected
        </li>
      </ul>
    </div>
  )
}

/** What the three risk levels mean, said once at the top. */
function RiskLegend() {
  const rows: { risk: Risk; text: string }[] = [
    { risk: 'Safe', text: 'Selected for you. Apps and tools rebuild it.' },
    { risk: 'Review', text: 'Does not come back on its own. Look before you tick it.' },
    { risk: 'Careful', text: 'May hold data you need. Confirmed one at a time.' },
  ]
  return (
    <section className="grid gap-3 sm:grid-cols-3" aria-label="What the labels mean">
      {rows.map((r) => (
        <div
          key={r.risk}
          className="flex flex-col items-start gap-1.5 rounded-xl border border-line bg-panel p-3.5"
        >
          <RiskBadge risk={r.risk} />
          <p className="text-xs text-muted">{r.text}</p>
        </div>
      ))}
      <p className="flex items-center gap-1.5 text-xs text-muted sm:col-span-3">
        <Icon name="shield" size={12} className="text-accent-ink" />
        Everything goes to the Trash first, and you can undo it from Activity.
      </p>
    </section>
  )
}

export default function Cleanup() {
  const scanId = useScanStore((s) => s.scanId)
  const phase = useScanStore((s) => s.phase)
  const dataVersion = useScanStore((s) => s.dataVersion)
  const { summary, loading, error, load, selectCategory, review, setSelected } = useCleanupStore()
  const openEmpty = useTrashStore((s) => s.openEmpty)
  const [careful, setCareful] = useState<CleanupItem | null>(null)
  const ready = scanId !== null && (phase === 'done' || phase === 'cancelled')

  // Entering the room re-reads open apps; later data changes only refresh the numbers.
  useEffect(() => {
    if (ready) void load(true)
  }, [ready, scanId, load])
  useEffect(() => {
    if (ready && dataVersion > 0) void load(false)
  }, [ready, dataVersion, load])

  if (!ready || scanId === null) {
    return (
      <section className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent-ink">
          <Icon name="cleanup" size={26} />
        </div>
        <h1 className="text-2xl font-semibold">
          {phase === 'running' ? 'Scanning…' : 'Scan first, then clean up'}
        </h1>
        <p className="max-w-sm text-muted">
          Cleanup finds caches, build files, logs and old installers in your last scan, grouped and
          totalled. Nothing is removed until you review it.
        </p>
        {phase !== 'running' && <ScanButton large />}
      </section>
    )
  }

  if (!summary) {
    return (
      <p className="p-8 text-muted">{error ?? (loading ? 'Looking through your files…' : '')}</p>
    )
  }

  const found = summary.categories.reduce((a, c) => a + c.bytes, 0)
  const foundItems = summary.categories.reduce((a, c) => a + c.items, 0)

  return (
    <div className="h-full overflow-auto px-6 pb-8">
      <div className="flex w-full flex-col gap-4 pt-6">
        <section className="card sticky top-0 z-10 flex flex-wrap items-center gap-4 p-6">
          <div>
            <p className="eyebrow">Ready to clear</p>
            <p className="num mt-1 font-display text-4xl font-semibold tracking-tight text-accent-ink">
              {formatBytes(summary.selected_bytes)}
            </p>
            <p className="num mt-1 text-xs text-muted">
              {formatCount(summary.selected_items)} of {formatCount(foundItems)} items selected ·{' '}
              {formatBytes(found)} found
            </p>
          </div>
          <div className="ml-auto flex flex-wrap items-center gap-2">
            <button
              type="button"
              className="btn"
              onClick={() => void selectCategory(null, 'Defaults')}
              title="Only safe items of apps that are not open"
            >
              Suggested
            </button>
            <button type="button" className="btn" onClick={() => void selectCategory(null, 'None')}>
              Select none
            </button>
            <button
              type="button"
              className="btn btn-primary"
              disabled={summary.selected_items === 0}
              onClick={() => void review()}
            >
              <Icon name="trash" size={13} /> Review &amp; move{' '}
              {formatCount(summary.selected_items)} items…
            </button>
          </div>
          <SelectionBar categories={summary.categories} found={found} />
          {summary.running_selected.length > 0 && (
            <p className="flex w-full items-center gap-1.5 text-xs text-warn">
              <Icon name="warning" size={12} /> Some selected items belong to apps that are open (
              {summary.running_selected.join(', ')}). You can quit them from the review.
            </p>
          )}
        </section>

        {error && (
          <p role="alert" className="text-bad">
            {error}
          </p>
        )}

        <RiskLegend />

        <ul className="flex flex-col gap-3" aria-label="Cleanup categories">
          {summary.categories.map((c) => (
            <CategoryCard key={c.id} c={c} scanId={scanId} onCareful={setCareful} />
          ))}
        </ul>

        <section className="card flex flex-wrap items-center gap-4 p-5">
          <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-bad-soft text-bad">
            <Icon name="trash" size={16} />
          </div>
          <div className="min-w-0 flex-1">
            <p className="font-semibold">
              Trash{' '}
              <span className="num font-normal text-muted">
                {summary.trash_bytes === null
                  ? '· size unknown without Full Disk Access'
                  : `· ${formatBytes(summary.trash_bytes)}`}
              </span>
            </p>
            <p className="text-muted">
              Moving files to the Trash does not free space yet. Emptying it does, for good.
            </p>
          </div>
          <span className="badge badge-careful">
            <Icon name="warning" size={11} /> Careful
          </span>
          <button type="button" className="btn" onClick={() => void openEmpty()}>
            Empty Trash…
          </button>
        </section>
      </div>

      <Modal
        open={careful !== null}
        onClose={() => setCareful(null)}
        labelledBy="careful-title"
        width={480}
      >
        <div className="flex flex-col gap-3 p-6">
          <h2 id="careful-title" className="text-lg font-semibold">
            Select this careful item?
          </h2>
          <p className="text-muted">
            <span className="font-medium text-fg">{careful?.name}</span> is marked careful: it may
            hold data you cannot get back, so it is never selected in bulk.
          </p>
          <p className="truncate font-mono text-xs text-muted" title={careful?.path}>
            {careful && tildify(careful.path)}
          </p>
          <div className="flex justify-end gap-2">
            <button type="button" className="btn" data-autofocus onClick={() => setCareful(null)}>
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() => {
                if (careful) void setSelected(careful, true, true)
                setCareful(null)
              }}
            >
              Select it
            </button>
          </div>
        </div>
      </Modal>
    </div>
  )
}
