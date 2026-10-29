import { useScanStore, type ScanScope } from '../state/scanStore'
import Icon from './Icon'

/** Scope picker plus the primary Scan / Cancel button. Used in the toolbar and on Overview. */
export default function ScanButton({ large = false }: { large?: boolean }) {
  const phase = useScanStore((s) => s.phase)
  const scope = useScanStore((s) => s.scope)
  const setScope = useScanStore((s) => s.setScope)
  const startSelected = useScanStore((s) => s.startSelected)
  const cancel = useScanStore((s) => s.cancel)
  const running = phase === 'running'

  return (
    <div className="flex items-center gap-2">
      <select
        aria-label="What to scan"
        value={scope}
        disabled={running}
        onChange={(e) => setScope(e.target.value as ScanScope)}
        className="btn pr-6"
        title="Home folder is quick and works without extra permission. Entire disk needs Full Disk Access to see everything."
      >
        <option value="home">Home folder</option>
        <option value="disk">Entire disk</option>
      </select>
      {running ? (
        <button
          type="button"
          className={`btn ${large ? 'btn-lg' : ''}`}
          onClick={() => void cancel()}
        >
          <Icon name="close" size={14} /> Cancel
        </button>
      ) : (
        <button
          type="button"
          className={`btn btn-primary ${large ? 'btn-lg' : ''}`}
          onClick={() => void startSelected()}
        >
          <Icon name="space" size={14} /> {phase === 'idle' ? 'Scan' : 'Scan again'}
        </button>
      )}
    </div>
  )
}
