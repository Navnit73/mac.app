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
    <li className="group flex items-center gap-3 border-b border-line px-4 py-2 last:border-0">
      {item.report_only ? (
        <span className="flex w-[13px] justify-center text-faint" title="DuoBolt leaves this alone">
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
          {item.name}
          <span className="ml-2 text-xs text-muted">{item.rule_name}</span>
        </p>
        <p className="truncate text-xs text-muted" title={item.path}>
          {middleTruncate(tildify(item.path), 84)}
          {' · '}
          {item.is_dir ? `${formatCount(item.files)} files · ` : ''}
          changed {formatAge(item.mtime).toLowerCase()}
        </p>
        {item.guidance && (
          <p className="mt-0.5 flex items-center gap-1 text-xs text-accent">
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
          <span className="badge bg-track text-muted" title="Apps or tools recreate it">
            Comes back
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
        className="flex w-full items-start gap-3 px-5 py-4 text-left hover:bg-hover disabled:hover:bg-transparent"
      >
        <span
          className="mt-1 h-3 w-3 shrink-0 rounded-[4px]"
          style={{ background: CATEGORY_COLOR[c.id] }}
          aria-hidden="true"
        />
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
            <div className="mt-2 flex items-center gap-2">
              <div className="h-1.5 w-40 overflow-hidden rounded-full bg-track" aria-hidden="true">
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
        <span className="num text-xl font-semibold">{formatBytes(c.bytes)}</span>
        {!empty && (
          <Icon
            name="chevron"
            size={14}
            className={`mt-1.5 text-faint transition-transform ${open ? 'rotate-90' : ''}`}
          />
        )}
      </button>
      {open && (
        <div className="border-t border-line">
          <div className="flex items-center gap-3 bg-hover px-5 py-1.5 text-xs">
            <button
              type="button"
              className="text-accent hover:underline"
              onClick={() => void selectCategory(c.id, 'All')}
            >
              Select all
            </button>
            <button
              type="button"
              className="text-accent hover:underline"
              onClick={() => void selectCategory(c.id, 'None')}
            >
              Select none
            </button>
            <button
              type="button"
              className="text-accent hover:underline"
              onClick={() => void selectCategory(c.id, 'Defaults')}
              title="Safe items of apps that are not open"
            >
              Suggested
            </button>
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
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent">
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
    <div className="h-full overflow-auto p-6">
      <div className="mx-auto flex max-w-4xl flex-col gap-4">
        <section className="card sticky top-0 z-10 flex flex-wrap items-center gap-4 p-5">
          <div>
            <p className="eyebrow">Ready to clear</p>
            <p className="num text-3xl font-semibold text-accent">
              {formatBytes(summary.selected_bytes)}
            </p>
            <p className="num text-xs text-muted">
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

        <p className="flex items-start gap-2 text-muted">
          <Icon name="shield" size={14} className="mt-0.5 shrink-0 text-accent" />
          Only items marked Safe are selected for you. Review first means it does not come back on
          its own, so look before you tick it. Everything goes to the Trash, and you can undo.
        </p>

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
