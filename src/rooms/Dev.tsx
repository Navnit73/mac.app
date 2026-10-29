import { useEffect, useState } from 'react'
import { commands, describeError, events } from '../lib/ipc'

export default function Dev() {
  const [result, setResult] = useState('')
  const [ticks, setTicks] = useState<number[]>([])

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
      <h2 className="text-2xl font-semibold">Developer</h2>
      <p className="mt-1 text-muted">
        Checks that the app and the Rust core can talk to each other.
      </p>
      <div className="mt-6 flex gap-2">
        <button type="button" onClick={ping} className="btn btn-primary">
          Ping
        </button>
        <button type="button" onClick={ticker} className="btn">
          Emit 5 events
        </button>
      </div>
      <p className="mt-4 font-mono" data-testid="ping-result">
        {result}
      </p>
      <p className="mt-2 font-mono text-muted">events: {ticks.join(', ')}</p>
    </section>
  )
}
