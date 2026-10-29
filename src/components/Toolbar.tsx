import { useLocation, useNavigate } from 'react-router-dom'
import { ROOMS } from '../rooms'
import { formatCount } from '../lib/format'
import { useFindStore } from '../state/findStore'
import { useScanStore } from '../state/scanStore'
import Icon from './Icon'
import ScanButton from './ScanButton'

export default function Toolbar() {
  const { pathname } = useLocation()
  const room = ROOMS.find((r) => pathname.startsWith(r.path))
  const phase = useScanStore((s) => s.phase)
  const progress = useScanStore((s) => s.progress)
  const navigate = useNavigate()
  const findText = useFindStore((s) => s.text)
  const setFind = useFindStore((s) => s.set)
  const onFind = pathname.startsWith('/find')

  return (
    <header
      data-tauri-drag-region
      className="flex h-[52px] shrink-0 items-center gap-3 border-b border-line bg-panel px-5"
    >
      <div data-tauri-drag-region className="flex min-w-0 items-baseline gap-2.5">
        <h1 data-tauri-drag-region className="text-[17px] font-semibold">
          {room?.label ?? 'Developer'}
        </h1>
        {room && (
          <span data-tauri-drag-region className="hidden truncate text-muted lg:inline">
            {room.blurb}
          </span>
        )}
      </div>
      {phase === 'running' && (
        <span className="num text-muted" aria-live="polite">
          Scanning… {formatCount(progress?.files ?? 0)} files
        </span>
      )}
      <div className="ml-auto flex items-center gap-3">
        {!onFind && (
          <label className="relative">
            <span className="sr-only">Search files</span>
            <Icon
              name="find"
              size={13}
              className="absolute top-1/2 left-2.5 -translate-y-1/2 text-faint"
            />
            <input
              type="search"
              value={findText}
              onChange={(e) => {
                setFind({ text: e.target.value })
                navigate('/find')
              }}
              onFocus={() => findText && navigate('/find')}
              placeholder="Search  ⌘F"
              className="h-[30px] w-60 rounded-lg border border-line bg-bg pr-2 pl-7 placeholder:text-faint"
            />
          </label>
        )}
        <ScanButton />
      </div>
    </header>
  )
}
