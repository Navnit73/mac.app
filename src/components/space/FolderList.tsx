import { useVirtualizer } from '@tanstack/react-virtual'
import { useCallback, useEffect, useRef, useState } from 'react'
import { KIND_COLOR } from '../../lib/colors'
import { formatAge, formatBytes, formatCount, formatPercent } from '../../lib/format'
import { commands, describeError } from '../../lib/ipc'
import { useScanStore } from '../../state/scanStore'
import { useSpaceStore } from '../../state/spaceStore'
import type { NodeRow } from '../../types/bindings/NodeRow'
import type { SortKey } from '../../types/bindings/SortKey'
import Icon from '../Icon'
import EntryIcon from './EntryIcon'

const ROW = 38
const COLS = 'grid-cols-[minmax(0,1fr)_200px_84px_96px]'

function SortHeader({
  label,
  k,
  sort,
  desc,
  onSort,
  right,
}: {
  label: string
  k: SortKey
  sort: SortKey
  desc: boolean
  onSort: (k: SortKey) => void
  right?: boolean
}) {
  const active = sort === k
  return (
    <button
      type="button"
      onClick={() => onSort(k)}
      aria-sort={active ? (desc ? 'descending' : 'ascending') : 'none'}
      className={`flex items-center gap-1 ${right ? 'justify-end' : ''} ${active ? 'text-fg' : ''}`}
    >
      {label}
      {active && <Icon name="chevron" size={11} className={desc ? 'rotate-90' : '-rotate-90'} />}
    </button>
  )
}

export default function FolderList({ scanId }: { scanId: number }) {
  const currentId = useSpaceStore((s) => s.currentId)
  const selectedId = useSpaceStore((s) => s.selectedId)
  const select = useSpaceStore((s) => s.select)
  const navigate = useSpaceStore((s) => s.navigate)
  const dataVersion = useScanStore((s) => s.dataVersion)

  const [sort, setSort] = useState<SortKey>('Size')
  const [desc, setDesc] = useState(true)
  const [rows, setRows] = useState<NodeRow[]>([])
  const [total, setTotal] = useState(0)
  const [cursor, setCursor] = useState<number | null>(0)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const request = useRef(0)
  const scroller = useRef<HTMLDivElement>(null)

  const load = useCallback(
    async (from: number) => {
      if (currentId === null) return
      const mine = ++request.current
      setLoading(true)
      try {
        const page = await commands.getChildren(scanId, currentId, sort, desc, from)
        if (mine !== request.current) return // a newer request superseded this one
        setRows((prev) => (from === 0 ? page.items : [...prev, ...page.items]))
        setTotal(page.total)
        setCursor(page.next_cursor)
        setError(null)
      } catch (e) {
        if (mine === request.current) setError(describeError(e))
      } finally {
        if (mine === request.current) setLoading(false)
      }
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [scanId, currentId, sort, desc, dataVersion],
  )

  useEffect(() => {
    setRows([])
    setCursor(0)
    void load(0)
    scroller.current?.scrollTo({ top: 0 })
  }, [load])

  // eslint-disable-next-line react-hooks/incompatible-library
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 12,
  })

  // Fetch the next page when the user nears the end.
  const items = virt.getVirtualItems()
  const last = items[items.length - 1]
  useEffect(() => {
    // Not after an error: the same page would fail again, in a tight loop.
    if (last && cursor !== null && !loading && error === null && last.index >= rows.length - 20)
      void load(cursor)
  }, [last, cursor, loading, error, rows.length, load])

  const onSort = (k: SortKey) => {
    if (k === sort) setDesc((d) => !d)
    else {
      setSort(k)
      setDesc(k !== 'Name')
    }
  }

  const open = (r: NodeRow) => {
    if (r.kind === 'Dir' && r.has_children) navigate(r.id)
  }

  const onKey = (e: React.KeyboardEvent) => {
    const i = rows.findIndex((r) => r.id === selectedId)
    const move = (n: number) => {
      const next = rows[Math.max(0, Math.min(rows.length - 1, n))]
      if (next) {
        select(next.id)
        virt.scrollToIndex(Math.max(0, Math.min(rows.length - 1, n)))
      }
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      move(i + 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      move(i < 0 ? 0 : i - 1)
    } else if (e.key === 'Enter') {
      const r = rows[i]
      if (r) open(r)
    } else if (e.key === ' ' && selectedId !== null) {
      e.preventDefault()
      commands.quickLook(scanId, selectedId).catch((err) => setError(describeError(err)))
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div
        className={`grid ${COLS} gap-3 border-b border-line px-4 py-2 text-xs font-semibold text-muted`}
      >
        <SortHeader label="Name" k="Name" sort={sort} desc={desc} onSort={onSort} />
        <SortHeader label="Size" k="Size" sort={sort} desc={desc} onSort={onSort} />
        <SortHeader label="Items" k="Count" sort={sort} desc={desc} onSort={onSort} right />
        <SortHeader label="Modified" k="Modified" sort={sort} desc={desc} onSort={onSort} right />
      </div>

      <div
        ref={scroller}
        tabIndex={0}
        role="listbox"
        aria-label="Folder contents"
        onKeyDown={onKey}
        className="min-h-0 flex-1 overflow-auto outline-offset-[-2px]"
      >
        {error && <p className="p-4 text-bad">{error}</p>}
        {!error && !loading && rows.length === 0 && (
          <div className="flex flex-col items-center gap-2 p-12 text-center text-muted">
            <Icon name="folder" size={28} />
            <p>This folder is empty or could not be read.</p>
          </div>
        )}
        <div style={{ height: virt.getTotalSize(), position: 'relative' }}>
          {items.map((v) => {
            const r = rows[v.index]
            if (!r) return null
            const selected = r.id === selectedId
            return (
              <div
                key={r.id}
                role="option"
                aria-selected={selected}
                onClick={() => select(r.id)}
                onDoubleClick={() => open(r)}
                className={`${`grid ${COLS}`} absolute inset-x-0 items-center gap-3 px-4 ${selected ? 'bg-selected' : 'hover:bg-hover'}`}
                style={{ top: v.start, height: ROW }}
              >
                <div className="flex min-w-0 items-center gap-2.5">
                  <EntryIcon kind={r.file_kind} isDir={r.kind === 'Dir'} isPackage={r.is_package} />
                  <span className={`truncate ${r.is_hidden ? 'text-muted' : ''}`} title={r.name}>
                    {r.name}
                  </span>
                  {r.is_unreadable && (
                    <span className="badge badge-review shrink-0" title="Needs Full Disk Access">
                      <Icon name="lock" size={10} /> Locked
                    </span>
                  )}
                  {r.is_dataless && (
                    <span className="badge shrink-0 bg-track text-muted" title="Stored in iCloud">
                      <Icon name="cloud" size={10} /> iCloud
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <span className="num w-[64px] text-right font-medium">{formatBytes(r.size)}</span>
                  <span
                    className="h-1.5 flex-1 overflow-hidden rounded-full bg-track"
                    title={formatPercent(r.parent_fraction)}
                  >
                    <span
                      className="block h-full rounded-full"
                      style={{
                        width: `${Math.max(r.parent_fraction * 100, r.size > 0 ? 2 : 0)}%`,
                        background: KIND_COLOR[r.file_kind],
                      }}
                    />
                  </span>
                </div>
                <span className="num text-right text-muted">
                  {r.kind === 'Dir' ? formatCount(r.file_count) : '—'}
                </span>
                <span className="num text-right text-muted">{formatAge(r.mtime)}</span>
              </div>
            )
          })}
        </div>
      </div>
      <div className="border-t border-line px-4 py-1.5 text-xs text-muted">
        {formatCount(total)} item{total === 1 ? '' : 's'}
        {cursor !== null && ' · loading more as you scroll'}
      </div>
    </div>
  )
}
