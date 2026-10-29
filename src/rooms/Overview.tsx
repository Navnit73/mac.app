import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import CategoryCard from '../components/CategoryCard'
import Icon from '../components/Icon'
import ProgressPanel from '../components/ProgressPanel'
import ScanButton from '../components/ScanButton'
import ScanReportDialog from '../components/ScanReportDialog'
import StorageBar from '../components/StorageBar'
import { commands, describeError } from '../lib/ipc'
import { formatBytes, formatCount, formatDuration } from '../lib/format'
import { useFdaStore } from '../state/fdaStore'
import { useScanStore } from '../state/scanStore'
import { useSpaceStore } from '../state/spaceStore'
import { useTrashStore } from '../state/trashStore'
import type { OverviewSummary } from '../types/bindings/OverviewSummary'

function Welcome() {
  return (
    <section className="mx-auto flex max-w-xl flex-col items-center gap-5 px-6 py-20 text-center">
      <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-accent-soft text-accent">
        <Icon name="overview" size={30} />
      </div>
      <h1 className="text-3xl font-semibold tracking-tight">See what is filling your Mac</h1>
      <p className="text-base text-muted">
        One scan explains your storage and shows what you can safely clear. Everything stays on this
        Mac.
      </p>
      <ScanButton large />
      <ul className="mt-4 grid gap-3 text-left sm:grid-cols-3">
        {[
          ['Nothing is deleted', 'Cleanups go to the Trash first, and you can undo them.'],
          ['You review everything', 'See each item and its size before anything moves.'],
          ['Private by design', 'No uploads, no accounts, no tracking.'],
        ].map(([t, d]) => (
          <li key={t} className="card p-4">
            <p className="font-semibold">{t}</p>
            <p className="mt-1 text-muted">{d}</p>
          </li>
        ))}
      </ul>
    </section>
  )
}

function InfoCard({
  label,
  value,
  sub,
  action,
}: {
  label: string
  value: string
  sub?: string
  action?: { text: string; onClick: () => void; disabled?: boolean; title?: string }
}) {
  return (
    <div className="card flex flex-col gap-1 p-4">
      <span className="eyebrow">{label}</span>
      <span className="num text-xl font-semibold">{value}</span>
      {sub && <span className="text-muted">{sub}</span>}
      {action && (
        <button
          type="button"
          className="btn mt-2 self-start"
          onClick={action.onClick}
          disabled={action.disabled}
          title={action.title}
        >
          {action.text}
        </button>
      )}
    </div>
  )
}

function Dashboard({ scanId, partial }: { scanId: number; partial: boolean }) {
  const navigate = useNavigate()
  const navigateSpace = useSpaceStore((s) => s.navigate)
  const fda = useFdaStore((s) => s.status)
  const dataVersion = useScanStore((s) => s.dataVersion)
  const [o, setO] = useState<OverviewSummary | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [reportOpen, setReportOpen] = useState(false)

  useEffect(() => {
    let live = true
    commands
      .getOverview(scanId)
      .then((r) => live && setO(r))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [scanId, dataVersion])

  if (error) return <p className="p-8 text-bad">{error}</p>
  if (!o) return <p className="p-8 text-muted">Loading…</p>

  const clearable = o.categories.filter((c) => c.analyzed).reduce((a, c) => a + c.bytes, 0)
  const used = o.volume.total - o.volume.available_important

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-6 p-8">
      {partial && (
        <p className="rounded-lg bg-warn-soft px-4 py-2 text-warn">
          This scan was cancelled, so the numbers below are partial.
        </p>
      )}

      <section className="card p-6">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="eyebrow">{o.volume.name}</p>
            <p className="mt-1 flex items-baseline gap-2">
              <span className="num text-4xl font-semibold tracking-tight">
                {formatBytes(o.volume.available_important)}
              </span>
              <span className="text-base text-muted">free of {formatBytes(o.volume.total)}</span>
            </p>
            <p className="mt-1 text-muted">
              {formatBytes(used)} used
              {o.volume.purgeable > 0 && (
                <span title="macOS frees this automatically when apps need space.">
                  {' '}
                  · {formatBytes(o.volume.purgeable)} purgeable
                </span>
              )}
            </p>
          </div>
          <div className="text-right">
            <p className="eyebrow">You could clear</p>
            <p className="num mt-1 text-3xl font-semibold text-accent">{formatBytes(clearable)}</p>
            <button
              type="button"
              className="btn btn-primary mt-2"
              disabled
              title="Review & Clear arrives with Cleanup (M6)"
            >
              Review &amp; Clear
            </button>
          </div>
        </div>
        <div className="mt-6">
          <StorageBar overview={o} />
        </div>
        {!o.covers_volume && (
          <p className="mt-4 flex items-start gap-2 text-muted">
            <Icon name="info" size={14} className="mt-0.5 shrink-0" />
            <span>
              You scanned {o.roots.join(', ')}. Everything else on the disk shows as &ldquo;Outside
              the scan&rdquo;. Choose Entire disk to account for all of it.
            </span>
          </p>
        )}
      </section>

      <section>
        <h2 className="mb-3 text-base font-semibold">What you can clear</h2>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {o.categories.map((c) => (
            <CategoryCard key={c.id} c={c} />
          ))}
        </div>
      </section>

      <section className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        <InfoCard
          label="Scanned"
          value={`${formatCount(o.files)} files`}
          sub={`${formatCount(o.dirs)} folders in ${formatDuration(o.elapsed_ms)}`}
        />
        <InfoCard
          label="Largest folder"
          value={o.largest_folder ? formatBytes(o.largest_folder_bytes) : '—'}
          sub={o.largest_folder?.name}
          action={
            o.largest_folder
              ? {
                  text: 'Open in Space',
                  onClick: () => {
                    if (o.largest_folder) navigateSpace(o.largest_folder.id)
                    navigate('/space')
                  },
                }
              : undefined
          }
        />
        <InfoCard
          label="Trash"
          value={o.trash_bytes === null ? 'Locked' : formatBytes(o.trash_bytes)}
          sub={
            o.trash_bytes === null
              ? fda === 'Granted'
                ? 'Could not be read'
                : 'Needs Full Disk Access to read'
              : 'Empty it to reclaim the space'
          }
          action={{
            text: 'Empty Trash…',
            onClick: () => void useTrashStore.getState().openEmpty(),
            title: 'Permanently delete everything in the Trash',
          }}
        />
        <InfoCard
          label="Skipped"
          value={formatCount(o.skipped)}
          sub={o.skipped > 0 ? 'Paths DuoBolt could not count' : 'Everything was readable'}
          action={
            o.skipped > 0 ? { text: 'View report', onClick: () => setReportOpen(true) } : undefined
          }
        />
      </section>

      <ScanReportDialog scanId={scanId} open={reportOpen} onClose={() => setReportOpen(false)} />
    </div>
  )
}

export default function Overview() {
  const phase = useScanStore((s) => s.phase)
  const scanId = useScanStore((s) => s.scanId)

  if (phase === 'idle') return <Welcome />
  if (phase === 'running' || phase === 'failed') {
    return (
      <div className="mx-auto max-w-3xl p-8">
        <ProgressPanel />
        {phase === 'failed' && (
          <div className="mt-4">
            <ScanButton />
          </div>
        )}
      </div>
    )
  }
  if (scanId === null) return null
  return <Dashboard key={scanId} scanId={scanId} partial={phase === 'cancelled'} />
}
