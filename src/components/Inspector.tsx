import { useEffect, useState } from 'react'
import { KIND_LABEL } from '../lib/colors'
import { formatAge, formatBytes, formatCount, formatDate } from '../lib/format'
import { commands, describeError } from '../lib/ipc'
import { useScanStore } from '../state/scanStore'
import { useSpaceStore } from '../state/spaceStore'
import type { NodeDetails } from '../types/bindings/NodeDetails'
import Icon from './Icon'
import EntryIcon from './space/EntryIcon'

function Row({ label, value, title }: { label: string; value: React.ReactNode; title?: string }) {
  return (
    <div className="flex justify-between gap-4 py-1.5">
      <dt className="shrink-0 text-muted">{label}</dt>
      <dd className="num min-w-0 select-text break-words text-right" title={title}>
        {value}
      </dd>
    </div>
  )
}

export default function Inspector() {
  const scanId = useScanStore((s) => s.scanId)
  const selectedId = useSpaceStore((s) => s.selectedId)
  if (scanId === null || selectedId === null) return null
  // Remount per selection so details and notes never leak between items.
  return <InspectorBody key={`${scanId}-${selectedId}`} scanId={scanId} nodeId={selectedId} />
}

function InspectorBody({ scanId, nodeId }: { scanId: number; nodeId: number }) {
  const select = useSpaceStore((s) => s.select)
  const [d, setD] = useState<NodeDetails | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [note, setNote] = useState<string | null>(null)

  useEffect(() => {
    let live = true
    commands
      .getNodeDetails(scanId, nodeId)
      .then((r) => live && setD(r))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [scanId, nodeId])

  const act = async (fn: () => Promise<unknown>, done?: string) => {
    try {
      await fn()
      if (done) setNote(done)
    } catch (e) {
      setNote(describeError(e))
    }
  }

  return (
    <aside
      aria-label="Inspector"
      className="flex w-80 shrink-0 flex-col overflow-y-auto border-l border-line bg-panel"
    >
      <div className="flex items-center justify-between px-4 pt-3">
        <span className="eyebrow">Inspector</span>
        <button
          type="button"
          className="btn btn-ghost px-1.5"
          aria-label="Close inspector"
          onClick={() => select(null)}
        >
          <Icon name="close" size={14} />
        </button>
      </div>

      {error && <p className="p-4 text-bad">{error}</p>}
      {!d && !error && <p className="p-4 text-muted">Loading…</p>}
      {d && (
        <div className="flex flex-col gap-4 p-4 pt-2">
          <header className="flex items-start gap-3">
            <EntryIcon kind={d.file_kind} isDir={d.kind === 'Dir'} isPackage={d.is_package} />
            <div className="min-w-0">
              <h2 className="select-text break-words text-[15px] font-semibold leading-tight">
                {d.name}
              </h2>
              <p className="text-muted">
                {d.is_package ? 'Package' : KIND_LABEL[d.file_kind]}
                {d.is_hidden && ' · Hidden'}
              </p>
            </div>
          </header>

          <p className="num text-3xl font-semibold tracking-tight">{formatBytes(d.total_alloc)}</p>

          {!d.exists && (
            <p className="rounded-lg bg-warn-soft px-3 py-2 text-warn">
              This item no longer exists. It changed after the scan.
            </p>
          )}
          {d.protected_reason && (
            <p className="flex items-start gap-2 rounded-lg bg-track px-3 py-2 text-muted">
              <Icon name="lock" size={14} className="mt-0.5 shrink-0" />
              {d.protected_reason}
            </p>
          )}
          {d.why && (
            <p className="flex items-start gap-2 rounded-lg bg-accent-soft px-3 py-2">
              <Icon name="info" size={14} className="mt-0.5 shrink-0 text-accent" />
              {d.why}
            </p>
          )}

          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              className="btn"
              disabled={!d.exists}
              onClick={() => void act(() => commands.revealInFinder(scanId, d.id))}
            >
              <Icon name="reveal" size={13} /> Reveal
            </button>
            <button
              type="button"
              className="btn"
              disabled={!d.exists}
              onClick={() => void act(() => commands.openNode(scanId, d.id))}
            >
              Open
            </button>
            <button
              type="button"
              className="btn"
              disabled={!d.exists}
              onClick={() => void act(() => commands.quickLook(scanId, d.id))}
              title="Quick Look (Space)"
            >
              <Icon name="eye" size={13} /> Quick Look
            </button>
            <button
              type="button"
              className="btn"
              onClick={() => void act(() => navigator.clipboard.writeText(d.path), 'Path copied.')}
            >
              <Icon name="copy" size={13} /> Copy path
            </button>
            <button
              type="button"
              className="btn"
              onClick={() =>
                void act(
                  () => commands.addExclusion(scanId, d.id),
                  'Excluded. It will be skipped in the next scan.',
                )
              }
              title="Skip this in future scans"
            >
              <Icon name="ban" size={13} /> Exclude
            </button>
          </div>
          <button
            type="button"
            className="btn justify-center"
            disabled
            title="Moving to the Trash arrives with Safe Trash (M4)"
          >
            <Icon name="trash" size={13} /> Move to Trash (M4)
          </button>
          {note && (
            <p role="status" className="text-muted">
              {note}
            </p>
          )}

          <section>
            <h3 className="eyebrow mb-1">Location</h3>
            <p className="select-text break-all font-mono text-xs text-muted">{d.path}</p>
          </section>

          <dl className="divide-y divide-line border-y border-line">
            <Row label="Size on disk" value={formatBytes(d.total_alloc)} />
            <Row
              label="Logical size"
              value={formatBytes(d.size_logical)}
              title="Bytes as apps see them. Can differ from size on disk for compressed or sparse files."
            />
            {d.kind === 'Dir' && <Row label="Items" value={formatCount(d.file_count)} />}
            <Row label="Modified" value={formatDate(d.modified)} title={formatAge(d.modified)} />
            <Row label="Created" value={formatDate(d.created)} />
            <Row
              label="Last opened"
              value={formatDate(d.last_used)}
              title={formatAge(d.last_used)}
            />
            <Row label="Owner" value={d.owner ?? '—'} />
            <Row label="Permissions" value={d.permissions ?? '—'} />
            {d.hard_links > 1 && d.kind === 'File' && (
              <Row label="Hard links" value={d.hard_links} />
            )}
            <Row
              label="iCloud"
              value={d.icloud === 'CloudOnly' ? 'Not downloaded' : 'On this Mac'}
            />
          </dl>

          {d.tags.length > 0 && (
            <section>
              <h3 className="eyebrow mb-1.5">Tags</h3>
              <div className="flex flex-wrap gap-1.5">
                {d.tags.map((t) => (
                  <span key={t} className="chip">
                    {t}
                  </span>
                ))}
              </div>
            </section>
          )}
          {d.where_froms.length > 0 && (
            <section>
              <h3 className="eyebrow mb-1">Downloaded from</h3>
              {d.where_froms.map((u) => (
                <p key={u} className="select-text break-all text-xs text-muted">
                  {u}
                </p>
              ))}
            </section>
          )}
        </div>
      )}
    </aside>
  )
}
