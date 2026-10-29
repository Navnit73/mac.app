import { homeDir } from '@tauri-apps/api/path'
import { useEffect, useState } from 'react'
import { useScanStore } from '../state/scanStore'

export default function ScanControl() {
  const phase = useScanStore((s) => s.phase)
  const start = useScanStore((s) => s.start)
  const cancel = useScanStore((s) => s.cancel)
  const [root, setRoot] = useState('')

  useEffect(() => {
    homeDir()
      .then((h) => setRoot((cur) => cur || h))
      .catch(() => undefined)
  }, [])

  const running = phase === 'running'

  return (
    <div className="flex items-center gap-2">
      <input
        aria-label="Folder to scan"
        value={root}
        onChange={(e) => setRoot(e.target.value)}
        disabled={running}
        placeholder="/absolute/path"
        className="w-96 rounded-md border border-line bg-elevated px-2 py-1 font-mono"
      />
      {running ? (
        <button
          type="button"
          onClick={() => void cancel()}
          className="rounded-md border border-line px-3 py-1"
        >
          Cancel
        </button>
      ) : (
        <button
          type="button"
          onClick={() => void start([root])}
          disabled={!root.startsWith('/')}
          className="rounded-md bg-accent px-3 py-1 text-accent-fg disabled:opacity-50"
        >
          Start scan
        </button>
      )}
    </div>
  )
}
