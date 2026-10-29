import { formatBytes, formatCount, formatDuration } from '../lib/format'
import { useScanStore } from '../state/scanStore'

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt className="eyebrow">{label}</dt>
      <dd className="num mt-0.5 text-2xl font-semibold">{value}</dd>
    </div>
  )
}

/** Live numbers while scanning, and the headline numbers of a finished scan. */
export default function ProgressPanel() {
  const phase = useScanStore((s) => s.phase)
  const progress = useScanStore((s) => s.progress)
  const summary = useScanStore((s) => s.summary)
  const error = useScanStore((s) => s.error)
  const cancel = useScanStore((s) => s.cancel)

  if (phase === 'idle') return null

  const files = summary?.files ?? progress?.files ?? 0
  const dirs = summary?.dirs ?? progress?.dirs ?? 0
  const bytes = summary?.total_alloc ?? progress?.bytes ?? 0
  const elapsed = summary?.elapsed_ms ?? progress?.elapsed_ms ?? 0

  return (
    <div className="card p-6" aria-live="polite">
      <div className="flex items-center justify-between">
        <h2 className="text-base font-semibold">
          {phase === 'running' && 'Scanning your Mac…'}
          {phase === 'done' && 'Scan complete'}
          {phase === 'cancelled' && 'Scan cancelled. Partial results kept.'}
          {phase === 'failed' && 'Scan failed'}
        </h2>
        {phase === 'running' && (
          <button type="button" className="btn" onClick={() => void cancel()}>
            Cancel
          </button>
        )}
      </div>
      {phase === 'running' && (
        <div
          className="relative mt-4 h-1.5 overflow-hidden rounded-full bg-track"
          aria-hidden="true"
        >
          <div className="absolute inset-y-0 w-1/3 animate-[slide_1.4s_ease-in-out_infinite] rounded-full bg-accent" />
        </div>
      )}
      {phase === 'failed' && error && <p className="mt-3 text-bad">{error}</p>}
      <dl className="mt-5 grid grid-cols-2 gap-6 sm:grid-cols-4">
        <Stat label="Files" value={formatCount(files)} />
        <Stat label="Folders" value={formatCount(dirs)} />
        <Stat label={summary ? 'Size on disk' : 'Seen so far'} value={formatBytes(bytes)} />
        <Stat label="Elapsed" value={formatDuration(elapsed)} />
      </dl>
      {phase === 'running' && progress && (
        <p className="mt-4 truncate font-mono text-xs text-muted" title={progress.current_path}>
          {progress.current_path}
          <span className="ml-2 text-faint">{formatCount(progress.files_per_sec)} files/s</span>
        </p>
      )}
    </div>
  )
}
