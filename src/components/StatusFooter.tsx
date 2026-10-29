import { formatBytes, formatCount, formatDuration } from '../lib/format'
import { useFdaStore } from '../state/fdaStore'
import { useScanStore } from '../state/scanStore'
import Icon from './Icon'

export default function StatusFooter() {
  const phase = useScanStore((s) => s.phase)
  const summary = useScanStore((s) => s.summary)
  const progress = useScanStore((s) => s.progress)
  const fda = useFdaStore((s) => s.status)
  const openSettings = useFdaStore((s) => s.openSettings)

  const scan =
    phase === 'idle'
      ? 'Not scanned yet'
      : phase === 'running'
        ? `Scanning… ${formatCount(progress?.files ?? 0)} files`
        : phase === 'failed'
          ? 'Scan failed'
          : `${phase === 'cancelled' ? 'Partial scan' : 'Scanned'}: ${formatCount(summary?.files ?? 0)} files, ${formatBytes(summary?.total_alloc ?? 0)} in ${formatDuration(summary?.elapsed_ms ?? 0)}`

  return (
    <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-line bg-sidebar px-4 text-xs text-muted">
      <span className="num">{scan}</span>
      <button
        type="button"
        className="ml-auto flex items-center gap-1.5 rounded px-1.5 hover:bg-hover"
        onClick={() => fda === 'Denied' && void openSettings()}
        title={
          fda === 'Denied'
            ? 'Click to open System Settings'
            : 'DuoBolt can read protected folders'
        }
      >
        <Icon name={fda === 'Granted' ? 'check' : 'shield'} size={12} />
        {fda === 'Granted' && 'Full Disk Access on'}
        {fda === 'Denied' && 'Full Disk Access off'}
        {(fda === 'Unknown' || fda === 'checking') && 'Full Disk Access unknown'}
      </button>
    </footer>
  )
}
