import { useLocation } from 'react-router-dom'
import { ROOMS } from '../rooms'
import { formatCount } from '../lib/format'
import { useScanStore } from '../state/scanStore'
import ScanButton from './ScanButton'

export default function Toolbar() {
  const { pathname } = useLocation()
  const room = ROOMS.find((r) => pathname.startsWith(r.path))
  const phase = useScanStore((s) => s.phase)
  const progress = useScanStore((s) => s.progress)

  return (
    <header
      data-tauri-drag-region
      className="flex h-[52px] shrink-0 items-center gap-3 border-b border-line bg-panel px-5"
    >
      <h1 data-tauri-drag-region className="text-[15px] font-semibold">
        {room?.label ?? 'Developer'}
      </h1>
      {phase === 'running' && (
        <span className="num text-muted" aria-live="polite">
          Scanning… {formatCount(progress?.files ?? 0)} files
        </span>
      )}
      <div className="ml-auto flex items-center gap-3">
        <input
          type="search"
          aria-label="Search files"
          placeholder="Search (arrives in M3)"
          disabled
          className="h-7 w-56 rounded-lg border border-line bg-bg px-3 placeholder:text-faint"
        />
        <ScanButton />
      </div>
    </header>
  )
}
