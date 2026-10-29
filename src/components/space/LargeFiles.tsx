import { useVirtualizer } from '@tanstack/react-virtual'
import { useEffect, useRef, useState } from 'react'
import { KIND_LABEL, KIND_ORDER } from '../../lib/colors'
import { formatAge, formatBytes, formatCount } from '../../lib/format'
import { commands, describeError } from '../../lib/ipc'
import { useScanStore } from '../../state/scanStore'
import { useSpaceStore } from '../../state/spaceStore'
import type { FileKind } from '../../types/bindings/FileKind'
import type { NodeRow } from '../../types/bindings/NodeRow'
import type { SmartGroup } from '../../types/bindings/SmartGroup'
import type { SmartGroupSummary } from '../../types/bindings/SmartGroupSummary'
import type { TopFilesResult } from '../../types/bindings/TopFilesResult'
import EntryIcon from './EntryIcon'

const ROW = 46
const MB = 1_000_000
const SIZES = [
  { label: '10 MB+', value: 10 * MB },
  { label: '100 MB+', value: 100 * MB },
  { label: '500 MB+', value: 500 * MB },
  { label: '1 GB+', value: 1000 * MB },
]
const GROUP_LABEL: Record<SmartGroup, string> = {
  OldInstallers: 'Old installers',
  Archives: 'Archives',
  Videos: 'Videos',
  DiskImages: 'Disk images & VMs',
  OldDownloads: 'Old downloads',
}
const GROUP_HINT: Record<SmartGroup, string> = {
  OldInstallers: '.dmg and .pkg files older than 30 days',
  Archives: '.zip, .tar, .7z and similar',
  Videos: 'Movie and video files',
  DiskImages: 'Disk images and virtual machine files',
  OldDownloads: 'Files in Downloads older than 30 days',
}

/** Shorten the user's home folder to `~` and the middle of long paths to an ellipsis. */
function shortPath(p: string | null): string {
  if (!p) return ''
  const home = p.replace(/^\/Users\/[^/]+/, '~')
  if (home.length <= 64) return home
  return `${home.slice(0, 26)}…${home.slice(-34)}`
}

export default function LargeFiles({ scanId }: { scanId: number }) {
  const selectedId = useSpaceStore((s) => s.selectedId)
  const select = useSpaceStore((s) => s.select)
  const dataVersion = useScanStore((s) => s.dataVersion)
  const [minSize, setMinSize] = useState(100 * MB)
  const [kind, setKind] = useState<FileKind | null>(null)
  const [group, setGroup] = useState<SmartGroup | null>(null)
  const [groups, setGroups] = useState<SmartGroupSummary[]>([])
  const [result, setResult] = useState<TopFilesResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const scroller = useRef<HTMLDivElement>(null)

  useEffect(() => {
    commands
      .getSmartGroups(scanId)
      .then(setGroups)
      .catch(() => setGroups([]))
  }, [scanId, dataVersion])

  useEffect(() => {
    let live = true
    commands
      .getTopFiles(scanId, { min_size: group ? 0 : minSize, kind, group }, 500)
      .then((r) => live && (setResult(r), setError(null)))
      .catch((e) => live && setError(describeError(e)))
    return () => {
      live = false
    }
  }, [scanId, minSize, kind, group, dataVersion])

  const rows: NodeRow[] = result?.items ?? []
  // eslint-disable-next-line react-hooks/incompatible-library
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 10,
  })

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex flex-col gap-2.5 border-b border-line px-4 py-3">
        <div className="flex flex-wrap items-center gap-2">
          <span className="eyebrow mr-1">Smart groups</span>
          {groups.map((g) => (
            <button
              key={g.group}
              type="button"
              className="chip"
              aria-pressed={group === g.group}
              title={GROUP_HINT[g.group]}
              disabled={g.count === 0}
              onClick={() => setGroup(group === g.group ? null : g.group)}
            >
              {GROUP_LABEL[g.group]}
              <span className="num text-xs opacity-80">{formatBytes(g.bytes)}</span>
            </button>
          ))}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <span className="eyebrow mr-1">Filter</span>
          {SIZES.map((s) => (
            <button
              key={s.value}
              type="button"
              className="chip"
              aria-pressed={!group && minSize === s.value}
              disabled={group !== null}
              onClick={() => setMinSize(s.value)}
            >
              {s.label}
            </button>
          ))}
          <span className="mx-1 h-4 w-px bg-line" />
          {KIND_ORDER.filter((k) => k !== 'Folder' && k !== 'Other').map((k) => (
            <button
              key={k}
              type="button"
              className="chip"
              aria-pressed={kind === k}
              onClick={() => setKind(kind === k ? null : k)}
            >
              {KIND_LABEL[k]}
            </button>
          ))}
        </div>
      </div>

      <div
        ref={scroller}
        className="min-h-0 flex-1 overflow-auto"
        role="listbox"
        aria-label="Largest files"
      >
        {error && <p className="p-4 text-bad">{error}</p>}
        {result && rows.length === 0 && (
          <p className="p-12 text-center text-muted">No files match. Try a smaller size.</p>
        )}
        <div style={{ height: virt.getTotalSize(), position: 'relative' }}>
          {virt.getVirtualItems().map((v) => {
            const r = rows[v.index]
            if (!r) return null
            const selected = r.id === selectedId
            return (
              <div
                key={r.id}
                role="option"
                aria-selected={selected}
                onClick={() => select(r.id)}
                className={`absolute inset-x-0 flex items-center gap-3 px-4 ${selected ? 'bg-selected' : 'hover:bg-hover'}`}
                style={{ top: v.start, height: ROW }}
              >
                <EntryIcon kind={r.file_kind} isDir={r.kind === 'Dir'} isPackage={r.is_package} />
                <div className="min-w-0 flex-1">
                  <p className="truncate font-medium" title={r.name}>
                    {r.name}
                  </p>
                  <p className="truncate text-xs text-muted" title={r.location ?? ''}>
                    {shortPath(r.location)}
                  </p>
                </div>
                <span className="num w-24 text-right text-muted">{formatAge(r.mtime)}</span>
                <span className="num w-20 text-right font-semibold">{formatBytes(r.size)}</span>
              </div>
            )
          })}
        </div>
      </div>
      <div className="border-t border-line px-4 py-1.5 text-xs text-muted">
        {result
          ? `${formatCount(result.matched_count)} matching · ${formatBytes(result.matched_bytes)} total${result.matched_count > rows.length ? ` · showing the largest ${rows.length}` : ''}`
          : 'Loading…'}
      </div>
    </div>
  )
}
