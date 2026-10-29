import { useCallback, useEffect, useRef, useState } from 'react'
import { commands, describeError } from '../lib/ipc'
import { useFdaStore } from '../state/fdaStore'
import type { SkippedPath } from '../types/bindings/SkippedPath'
import type { SkipReason } from '../types/bindings/SkipReason'
import Icon from './Icon'

interface Props {
  scanId: number
  open: boolean
  onClose: () => void
}

const REASON: Record<SkipReason, { label: string; hint: string }> = {
  PermissionDenied: {
    label: 'Protected by macOS',
    hint: 'Needs Full Disk Access to be read.',
  },
  Vanished: { label: 'Disappeared', hint: 'Removed while the scan was running.' },
  IoError: { label: 'Unreadable', hint: 'The disk reported an error for this path.' },
  Excluded: { label: 'Excluded', hint: 'Skipped by an exclusion or a volume rule.' },
  Dataless: { label: 'In iCloud', hint: 'Stored in iCloud, not counted as local space.' },
}

export default function ScanReportDialog({ scanId, open, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null)
  const [items, setItems] = useState<SkippedPath[]>([])
  const [total, setTotal] = useState(0)
  const [next, setNext] = useState<number | null>(0)
  const [error, setError] = useState<string | null>(null)
  const fda = useFdaStore((s) => s.status)
  const openSettings = useFdaStore((s) => s.openSettings)

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

  const protectedCount = items.filter((i) => i.reason === 'PermissionDenied').length

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      className="m-auto w-[720px] max-w-[92vw] rounded-2xl border border-line bg-panel p-0 text-fg shadow-2xl backdrop:bg-black/40"
    >
      <div className="flex items-center justify-between border-b border-line px-5 py-4">
        <div>
          <h2 className="text-base font-semibold">Scan report</h2>
          <p className="text-muted">
            {total} path{total === 1 ? '' : 's'} not counted in the totals.
          </p>
        </div>
        <button type="button" onClick={onClose} className="btn btn-ghost px-2" aria-label="Close">
          <Icon name="close" />
        </button>
      </div>

      {protectedCount > 0 && fda !== 'Granted' && (
        <div className="flex items-start gap-3 border-b border-line bg-warn-soft px-5 py-3">
          <span className="mt-0.5 text-warn">
            <Icon name="shield" size={18} />
          </span>
          <div className="flex-1">
            <p className="font-semibold">These folders are protected by macOS</p>
            <p className="text-muted">
              Grant Full Disk Access to include them: Mail, Messages, Safari, Trash and more.
              Nothing is changed or sent anywhere.
            </p>
          </div>
          <button type="button" className="btn btn-primary" onClick={() => void openSettings()}>
            Open System Settings
          </button>
        </div>
      )}

      <ul className="max-h-[50vh] overflow-auto px-5 py-2">
        {items.map((it, i) => (
          <li key={i} className="flex items-center gap-3 border-b border-line py-1.5 last:border-0">
            <span
              className={`badge w-40 shrink-0 justify-center ${it.reason === 'PermissionDenied' ? 'badge-review' : 'bg-track text-muted'}`}
              title={REASON[it.reason].hint}
            >
              {REASON[it.reason].label}
            </span>
            <span className="truncate font-mono text-xs" title={it.path}>
              {it.path}
            </span>
          </li>
        ))}
        {items.length === 0 && !error && (
          <li className="py-6 text-center text-muted">Nothing was skipped.</li>
        )}
      </ul>
      {error && <p className="px-5 pb-3 text-bad">{error}</p>}
      {next !== null && items.length > 0 && (
        <div className="border-t border-line px-5 py-3">
          <button type="button" onClick={() => void loadPage(next)} className="btn">
            Load more
          </button>
        </div>
      )}
    </dialog>
  )
}
