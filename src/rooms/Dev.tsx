import { useEffect, useState } from 'react'
import ProgressPanel from '../components/ProgressPanel'
import ScanControl from '../components/ScanControl'
import ScanReportDialog from '../components/ScanReportDialog'
import { commands, describeError, events } from '../lib/ipc'
import { useScanStore } from '../state/scanStore'

export default function Dev() {
  const [result, setResult] = useState('')
  const [ticks, setTicks] = useState<number[]>([])
  const [reportOpen, setReportOpen] = useState(false)
  const scanId = useScanStore((s) => s.scanId)
  const phase = useScanStore((s) => s.phase)
  const reportReady = scanId !== null && (phase === 'done' || phase === 'cancelled')

  useEffect(() => {
    const un = events.onTick((e) => setTicks((t) => [...t, e.seq]))
    return () => {
      void un.then((fn) => fn())
    }
  }, [])

  const ping = async () => {
    try {
      const r = await commands.ping('ui')
      setResult(`${r.message} (core ${r.core_version})`)
    } catch (e) {
      setResult(describeError(e))
    }
  }

  const ticker = async () => {
    setTicks([])
    try {
      await commands.emitTicks(5)
    } catch (e) {
      setResult(describeError(e))
    }
  }

  return (
    <section className="p-8">
      <h1 className="text-2xl font-semibold">Developer</h1>
      <p className="mt-1 text-muted">IPC round-trip check.</p>
      <div className="mt-6 flex gap-2">
        <button
          type="button"
          onClick={ping}
          className="rounded-md bg-accent px-3 py-1 text-accent-fg"
        >
          Ping
        </button>
        <button type="button" onClick={ticker} className="rounded-md border border-line px-3 py-1">
          Emit 5 events
        </button>
      </div>
      <p className="mt-4 font-mono" data-testid="ping-result">
        {result}
      </p>
      <p className="mt-2 font-mono text-muted">events: {ticks.join(', ')}</p>

      <h2 className="mt-10 text-lg font-semibold">Scan engine</h2>
      <div className="mt-3">
        <ScanControl />
      </div>
      <div className="mt-4">
        <ProgressPanel />
      </div>
      <button
        type="button"
        disabled={!reportReady}
        onClick={() => setReportOpen(true)}
        className="mt-4 rounded-md border border-line px-3 py-1 disabled:opacity-50"
      >
        Scan report
      </button>
      {scanId !== null && (
        <ScanReportDialog scanId={scanId} open={reportOpen} onClose={() => setReportOpen(false)} />
      )}
    </section>
  )
}
