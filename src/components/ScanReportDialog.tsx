import { useCallback, useEffect, useRef, useState } from 'react'
import { commands, describeError } from '../lib/ipc'
import type { SkippedPath } from '../types/bindings/SkippedPath'

interface Props {
  scanId: number
  open: boolean
  onClose: () => void
}

export default function ScanReportDialog({ scanId, open, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null)
  const [items, setItems] = useState<SkippedPath[]>([])
  const [total, setTotal] = useState(0)
  const [next, setNext] = useState<number | null>(0)
  const [error, setError] = useState<string | null>(null)

  const loadPage = useCallback(
    async (page: number) => {
      try {
        const r = await commands.getScanReport(scanId, page)
        setItems((prev) => (page === 0 ? r.items : [...prev, ...r.items]))
        setTotal(r.total)
        setNext(r.next_page)
        setError(null)
      } catch (e) {
        setError(describeError(e))
      }
    },
    [scanId],
  )

  useEffect(() => {
    const d = ref.current
    if (!d) return
    if (open && !d.open) {
      d.showModal()
      void loadPage(0)
    }
    if (!open && d.open) d.close()
  }, [open, loadPage])

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      className="m-auto w-[640px] max-w-[90vw] rounded-lg border border-line bg-bg p-0 text-fg backdrop:bg-black/30"
    >
      <div className="flex items-center justify-between border-b border-line px-4 py-3">
        <h2 className="font-semibold">Scan report ({total} skipped)</h2>
        <button
          type="button"
          onClick={onClose}
          className="rounded-md border border-line px-2 py-0.5"
        >
          Close
        </button>
      </div>
      <ul className="max-h-[50vh] overflow-auto px-4 py-2 font-mono text-xs">
        {items.map((it, i) => (
          <li key={i} className="flex gap-3 py-0.5">
            <span className="w-32 shrink-0 text-muted">{it.reason}</span>
            <span className="truncate" title={it.path}>
              {it.path}
            </span>
          </li>
        ))}
        {items.length === 0 && !error && <li className="py-2 text-muted">Nothing was skipped.</li>}
      </ul>
      {error && <p className="px-4 pb-2 text-red-600">{error}</p>}
      {next !== null && (
        <div className="border-t border-line px-4 py-2">
          <button
            type="button"
            onClick={() => void loadPage(next)}
            className="rounded-md border border-line px-3 py-1"
          >
            Load more
          </button>
        </div>
      )}
    </dialog>
  )
}
