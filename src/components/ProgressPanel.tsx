import { formatBytes, formatCount, formatDuration } from '../lib/format'
import { useScanStore } from '../state/scanStore'

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="text-xs text-muted">{label}</dt>
      <dd className="text-lg font-medium tabular-nums">{value}</dd>
    </div>
  )
}

export default function ProgressPanel() {
  const phase = useScanStore((s) => s.phase)
  const progress = useScanStore((s) => s.progress)
  const summary = useScanStore((s) => s.summary)
  const error = useScanStore((s) => s.error)

  if (phase === 'idle') return <p className="text-muted">No scan yet.</p>

  const files = summary?.files ?? progress?.files ?? 0
  const dirs = summary?.dirs ?? progress?.dirs ?? 0
  const bytes = summary?.total_alloc ?? progress?.bytes ?? 0
  const elapsed = summary?.elapsed_ms ?? progress?.elapsed_ms ?? 0

  return (
    <div aria-live="polite">
      <p className="mb-3 text-muted">
        {phase === 'running' && 'Scanning…'}
        {phase === 'done' && 'Scan complete.'}
        {phase === 'cancelled' && 'Scan cancelled. Partial results kept.'}
        {phase === 'failed' && `Scan failed${error ? `: ${error}` : '.'}`}
      </p>
      <dl className="grid grid-cols-2 gap-4 sm:grid-cols-4">
        <Stat label="Files" value={formatCount(files)} />
        <Stat label="Folders" value={formatCount(dirs)} />
        <Stat label={summary ? 'Size on disk' : 'Seen so far'} value={formatBytes(bytes)} />
        <Stat label="Elapsed" value={formatDuration(elapsed)} />
      </dl>
      {phase === 'running' && progress && (
        <p className="mt-3 truncate font-mono text-xs text-muted" title={progress.current_path}>
          {progress.current_path}
          <span className="ml-2">({formatCount(progress.files_per_sec)} files/s)</span>
        </p>
      )}
      {summary && summary.skipped > 0 && (
        <p className="mt-3 text-muted">{formatCount(summary.skipped)} paths skipped.</p>
      )}
    </div>
  )
}
