import { useCallback, useEffect, useState } from 'react'
import Icon from '../components/Icon'
import Modal from '../components/Modal'
import WeeklyChart from '../components/WeeklyChart'
import { formatBytes, formatCount } from '../lib/format'
import { commands, describeError } from '../lib/ipc'
import { middleTruncate, tildify } from '../lib/text'
import { useScanStore } from '../state/scanStore'
import { useTrashStore } from '../state/trashStore'
import type { BatchPage } from '../types/bindings/BatchPage'
import type { BatchSummary } from '../types/bindings/BatchSummary'
import type { ItemStatus } from '../types/bindings/ItemStatus'
import type { WeeklyStats } from '../types/bindings/WeeklyStats'

const STATUS: Record<ItemStatus, { label: string; cls: string }> = {
  InTrash: { label: 'In Trash', cls: 'badge-review' },
  Restored: { label: 'Restored', cls: 'badge-safe' },
  Emptied: { label: 'Emptied', cls: 'bg-track text-muted' },
  Failed: { label: 'Failed', cls: 'badge-careful' },
  Skipped: { label: 'Left alone', cls: 'bg-track text-muted' },
  Conflict: { label: 'Name taken', cls: 'badge-careful' },
}

function when(t: number): string {
  return new Date(t * 1000).toLocaleString('en-US', {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  })
}

function batchState(b: BatchSummary): { label: string; cls: string } {
  if (b.in_trash + b.conflicts > 0 && b.restored > 0)
    return { label: 'Partly restored', cls: 'badge-review' }
  if (b.in_trash + b.conflicts > 0) return { label: 'In Trash', cls: 'badge-review' }
  if (b.restored === b.item_count) return { label: 'Restored', cls: 'badge-safe' }
  return { label: 'Emptied', cls: 'bg-track text-muted' }
}

function Stat({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="card p-4">
      <p className="eyebrow">{label}</p>
      <p className="num mt-1 text-2xl font-semibold">{value}</p>
      {sub && <p className="text-muted">{sub}</p>}
    </div>
  )
}

function Detail({ batchId, onChanged }: { batchId: number; onChanged: () => void }) {
  const scanId = useScanStore((s) => s.scanId)
  const bump = useScanStore((s) => s.bumpData)
  const [page, setPage] = useState<BatchPage | null>(null)
  const [checked, setChecked] = useState<number[]>([])
  const [note, setNote] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const load = useCallback(() => {
    commands
      .getBatch(batchId, 0)
      .then((p) => setPage(p))
      .catch((e) => setNote(describeError(e)))
  }, [batchId])
  useEffect(load, [load])

  const run = async (ids: number[] | null, rename: boolean) => {
    setBusy(true)
    try {
      const r = await commands.undoBatch(batchId, ids, rename, scanId)
      const bits = [`Restored ${r.restored}`]
      if (r.conflicts) bits.push(`${r.conflicts} name taken`)
      if (r.missing) bits.push(`${r.missing} already emptied`)
      if (r.failed.length) bits.push(`${r.failed.length} failed: ${r.failed[0]?.reason ?? ''}`)
      setNote(`${bits.join(' · ')}.`)
      setChecked([])
      bump()
      load()
      onChanged()
    } catch (e) {
      setNote(describeError(e))
    } finally {
      setBusy(false)
    }
  }

  if (!page) return <p className="p-6 text-muted">Loading…</p>
  const undoable = page.items.filter((i) => i.status === 'InTrash' || i.status === 'Conflict')

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="border-b border-line px-5 py-4">
        <p className="eyebrow">{page.batch.room}</p>
        <h2 className="mt-0.5 text-lg font-semibold">
          {formatCount(page.batch.item_count)} items · {formatBytes(page.batch.bytes)}
        </h2>
        <p className="text-muted">{when(page.batch.created_at)}</p>
        <div className="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || undoable.length === 0}
            onClick={() => void run(null, false)}
          >
            Undo all
          </button>
          <button
            type="button"
            className="btn"
            disabled={busy || checked.length === 0}
            onClick={() => void run(checked, false)}
          >
            Undo selected ({checked.length})
          </button>
          {page.batch.conflicts > 0 && (
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => void run(null, true)}
              title="Put items back next to the new file, named “name (restored)”"
            >
              Restore with a new name
            </button>
          )}
        </div>
        {note && (
          <p role="status" className="mt-2 text-muted">
            {note}
          </p>
        )}
      </div>
      <ul className="min-h-0 flex-1 overflow-auto px-5 py-2">
        {page.items.map((it) => {
          const can = it.status === 'InTrash' || it.status === 'Conflict'
          return (
            <li
              key={it.id}
              className="flex items-center gap-3 border-b border-line py-2 last:border-0"
            >
              <input
                type="checkbox"
                aria-label={`Select ${it.original_path}`}
                disabled={!can}
                checked={checked.includes(it.id)}
                onChange={(e) =>
                  setChecked((c) =>
                    e.target.checked ? [...c, it.id] : c.filter((x) => x !== it.id),
                  )
                }
              />
              <div className="min-w-0 flex-1">
                <p className="truncate" title={it.original_path}>
                  {middleTruncate(tildify(it.original_path), 70)}
                </p>
                {it.reason && <p className="truncate text-xs text-muted">{it.reason}</p>}
              </div>
              <span className={`badge ${STATUS[it.status].cls}`}>{STATUS[it.status].label}</span>
              <span className="num w-20 text-right">{formatBytes(it.size)}</span>
            </li>
          )
        })}
        {page.next_page !== null && (
          <li className="py-2 text-center text-xs text-muted">
            Showing the largest {page.items.length} items.
          </li>
        )}
      </ul>
    </div>
  )
}

export default function Activity() {
  const dataVersion = useScanStore((s) => s.dataVersion)
  const banner = useTrashStore((s) => s.banner)
  const [stats, setStats] = useState<WeeklyStats | null>(null)
  const [batches, setBatches] = useState<BatchSummary[]>([])
  const [selected, setSelected] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)
  const [confirmClear, setConfirmClear] = useState(false)
  const [tick, setTick] = useState(0)

  useEffect(() => {
    let live = true
    Promise.all([commands.getWeeklyStats(12), commands.listBatches(200, 0)])
      .then(([s, b]) => {
        if (!live) return
        setStats(s)
        setBatches(b)
        setError(null)
      })
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [dataVersion, banner, tick])

  const refresh = () => setTick((t) => t + 1)

  if (error) return <p className="p-8 text-bad">{error}</p>
  if (!stats) return <p className="p-8 text-muted">Loading…</p>

  if (batches.length === 0) {
    return (
      <section className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent">
          <Icon name="activity" size={26} />
        </div>
        <h1 className="text-2xl font-semibold">No cleanups yet</h1>
        <p className="max-w-sm text-muted">
          When you move items to the Trash, each cleanup appears here with its size. You can undo
          any of them while the items are still in the Trash.
        </p>
      </section>
    )
  }

  const thisWeek = stats.this_week_bytes
  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col gap-5 overflow-auto p-6">
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Stat
            label="Recovered so far"
            value={formatBytes(stats.total_bytes)}
            sub="Restored items not counted"
          />
          <Stat label="This week" value={formatBytes(thisWeek)} />
          <Stat label="Biggest cleanup" value={formatBytes(stats.biggest_bytes)} />
          <Stat label="Cleanups" value={formatCount(stats.total_batches)} />
        </div>

        <section className="card p-5">
          <h2 className="text-base font-semibold">Space recovered per week</h2>
          <p className="mb-3 text-muted">Last 12 weeks</p>
          <WeeklyChart weeks={stats.weeks} />
        </section>

        <section className="card overflow-hidden">
          <div className="flex items-center gap-2 border-b border-line px-5 py-3">
            <h2 className="text-base font-semibold">History</h2>
            <div className="ml-auto flex gap-2">
              <button
                type="button"
                className="btn"
                onClick={() =>
                  void commands
                    .exportActivityCsv()
                    .then((p) => setNote(`Saved ${tildify(p)}`))
                    .catch((e) => setNote(describeError(e)))
                }
              >
                Export CSV
              </button>
              <button type="button" className="btn" onClick={() => setConfirmClear(true)}>
                Clear history
              </button>
            </div>
          </div>
          {note && (
            <p role="status" className="border-b border-line px-5 py-2 text-muted">
              {note}
            </p>
          )}
          <ul>
            {batches.map((b) => {
              const st = batchState(b)
              return (
                <li key={b.id}>
                  <button
                    type="button"
                    onClick={() => setSelected(b.id)}
                    className={`flex w-full items-center gap-4 border-b border-line px-5 py-3 text-left last:border-0 ${selected === b.id ? 'bg-selected' : 'hover:bg-hover'}`}
                  >
                    <div className="min-w-0 flex-1">
                      <p className="font-medium">
                        {formatCount(b.item_count)} item{b.item_count === 1 ? '' : 's'} from{' '}
                        {b.room}
                      </p>
                      <p className="text-xs text-muted">{when(b.created_at)}</p>
                    </div>
                    <span className={`badge ${st.cls}`}>{st.label}</span>
                    <span className="num w-20 text-right font-semibold">
                      {formatBytes(b.bytes)}
                    </span>
                    <Icon name="chevron" size={13} className="text-faint" />
                  </button>
                </li>
              )
            })}
          </ul>
        </section>
      </div>

      {selected !== null && (
        <aside
          className="flex w-[420px] shrink-0 flex-col border-l border-line bg-panel"
          aria-label="Cleanup details"
        >
          <div className="flex justify-end px-3 pt-2">
            <button
              type="button"
              className="btn btn-ghost px-1.5"
              aria-label="Close details"
              onClick={() => setSelected(null)}
            >
              <Icon name="close" size={13} />
            </button>
          </div>
          <Detail key={selected} batchId={selected} onChanged={refresh} />
        </aside>
      )}

      <Modal
        open={confirmClear}
        onClose={() => setConfirmClear(false)}
        labelledBy="clear-title"
        width={440}
      >
        <div className="flex flex-col gap-3 p-6">
          <h2 id="clear-title" className="text-lg font-semibold">
            Clear the history?
          </h2>
          <p className="text-muted">
            The list of cleanups is removed from this Mac. Files are not touched, but items still in
            the Trash can no longer be undone from DuoBolt (you can still drag them out of the Trash
            yourself).
          </p>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              className="btn"
              onClick={() => setConfirmClear(false)}
              data-autofocus
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={() =>
                void commands
                  .clearActivity()
                  .then(() => {
                    setConfirmClear(false)
                    setSelected(null)
                    refresh()
                  })
                  .catch((e) => setNote(describeError(e)))
              }
            >
              Clear history
            </button>
          </div>
        </div>
      </Modal>
    </div>
  )
}
