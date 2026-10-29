import { useVirtualizer } from '@tanstack/react-virtual'
import { useEffect, useMemo, useRef, useState } from 'react'
import Icon from '../components/Icon'
import ScanButton from '../components/ScanButton'
import EntryIcon from '../components/space/EntryIcon'
import { KIND_LABEL, KIND_ORDER } from '../lib/colors'
import { formatAge, formatBytes, formatCount } from '../lib/format'
import { commands, describeError } from '../lib/ipc'
import { highlightParts, middleTruncate, tildify } from '../lib/text'
import { toQuery, useFindStore, type DatePreset, type SizePreset } from '../state/findStore'
import { useScanStore } from '../state/scanStore'
import { useSpaceStore } from '../state/spaceStore'
import type { HiddenMode } from '../types/bindings/HiddenMode'
import type { NodeRow } from '../types/bindings/NodeRow'
import type { SearchMode } from '../types/bindings/SearchMode'
import type { SearchSort } from '../types/bindings/SearchSort'

const ROW = 46
const DEBOUNCE_MS = 40

const MODES: { value: SearchMode; label: string; hint: string }[] = [
  { value: 'Substring', label: 'Contains', hint: 'Name contains the text anywhere' },
  { value: 'Prefix', label: 'Starts with', hint: 'Name starts with the text' },
  { value: 'Exact', label: 'Exact', hint: 'Whole name equals the text' },
  { value: 'Glob', label: 'Glob', hint: 'Wildcards: *.pdf, IMG_??.jpg, report-[0-9]*' },
  { value: 'Regex', label: 'Regex', hint: 'Regular expression, for example ^img_\\d+\\.jpe?g$' },
]
const SIZES: { value: SizePreset; label: string }[] = [
  { value: 'any', label: 'Any size' },
  { value: '1mb', label: '1 MB+' },
  { value: '100mb', label: '100 MB+' },
  { value: '1gb', label: '1 GB+' },
]
const DATES: { value: DatePreset; label: string }[] = [
  { value: 'any', label: 'Any time' },
  { value: 'day', label: 'Today' },
  { value: 'week', label: 'Past week' },
  { value: 'month', label: 'Past month' },
  { value: 'year', label: 'Past year' },
  { value: 'older', label: 'Over a year ago' },
]
const HIDDEN: { value: HiddenMode; label: string }[] = [
  { value: 'Include', label: 'Show hidden' },
  { value: 'Exclude', label: 'Skip hidden' },
  { value: 'Only', label: 'Only hidden' },
]
const SORTS: { value: SearchSort; label: string }[] = [
  { value: 'Relevance', label: 'Best match' },
  { value: 'Name', label: 'Name' },
  { value: 'Size', label: 'Size' },
  { value: 'Modified', label: 'Date modified' },
]

function Name({ row, text, mode }: { row: NodeRow; text: string; mode: SearchMode }) {
  const parts =
    mode === 'Substring' || mode === 'Prefix' || mode === 'Exact'
      ? highlightParts(row.name, text, mode !== 'Substring')
      : null
  if (!parts) return <>{row.name}</>
  return (
    <>
      {parts[0]}
      <mark className="rounded-[3px] bg-accent-soft font-semibold text-fg">{parts[1]}</mark>
      {parts[2]}
    </>
  )
}

function Select<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: T
  options: { value: T; label: string }[]
  onChange: (v: T) => void
}) {
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(e) => onChange(e.target.value as T)}
      className={`btn h-6 rounded-full pr-6 text-xs ${value !== options[0]?.value ? 'border-accent bg-accent-soft' : ''}`}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  )
}

export default function Find() {
  const scanId = useScanStore((s) => s.scanId)
  const phase = useScanStore((s) => s.phase)
  const store = useFindStore()
  const {
    text,
    mode,
    kind,
    size,
    date,
    hidden,
    scope,
    sort,
    descending,
    selected,
    focusTick,
    set,
  } = store
  const selectInspector = useSpaceStore((s) => s.select)

  const [rows, setRows] = useState<NodeRow[]>([])
  const [total, setTotal] = useState(0)
  const [cursor, setCursor] = useState<number | null>(null)
  const [truncated, setTruncated] = useState(false)
  const [patternError, setPatternError] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [missing, setMissing] = useState<Set<number>>(new Set())
  const [note, setNote] = useState<string | null>(null)
  const anchor = useRef<number | null>(null)
  const request = useRef(0)
  const input = useRef<HTMLInputElement>(null)
  const scroller = useRef<HTMLDivElement>(null)

  const query = useMemo(
    () => toQuery({ ...store }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [text, mode, kind, size, date, hidden, scope, sort, descending],
  )
  const ready = scanId !== null && (phase === 'done' || phase === 'cancelled')

  useEffect(() => {
    input.current?.focus()
  }, [focusTick])

  // Debounced search; responses to older requests are ignored.
  useEffect(() => {
    if (!ready || scanId === null) return
    const mine = ++request.current
    const t = setTimeout(() => {
      setBusy(true)
      commands
        .search(scanId, query, 0)
        .then((r) => {
          if (mine !== request.current) return
          setRows(r.items)
          setTotal(r.total)
          setCursor(r.next_cursor)
          setTruncated(r.truncated)
          setPatternError(r.pattern_error)
          setError(null)
          scroller.current?.scrollTo({ top: 0 })
        })
        .catch((e) => mine === request.current && setError(describeError(e)))
        .finally(() => mine === request.current && setBusy(false))
    }, DEBOUNCE_MS)
    return () => clearTimeout(t)
  }, [ready, scanId, query])

  const loadMore = () => {
    if (scanId === null || cursor === null || busy) return
    const mine = request.current
    setBusy(true)
    commands
      .search(scanId, query, cursor)
      .then((r) => {
        if (mine !== request.current) return
        setRows((prev) => [...prev, ...r.items])
        setCursor(r.next_cursor)
      })
      .catch((e) => setError(describeError(e)))
      .finally(() => setBusy(false))
  }

  // eslint-disable-next-line react-hooks/incompatible-library
  const virt = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => ROW,
    overscan: 10,
  })
  const items = virt.getVirtualItems()
  const last = items[items.length - 1]
  useEffect(() => {
    if (last && cursor !== null && !busy && last.index >= rows.length - 20) loadMore()
  })

  const click = (e: React.MouseEvent, index: number, id: number) => {
    let next: number[]
    if (e.shiftKey && anchor.current !== null) {
      const a = Math.min(anchor.current, index)
      const b = Math.max(anchor.current, index)
      next = rows.slice(a, b + 1).map((r) => r.id)
    } else if (e.metaKey) {
      next = selected.includes(id) ? selected.filter((x) => x !== id) : [...selected, id]
      anchor.current = index
    } else {
      next = [id]
      anchor.current = index
    }
    set({ selected: next })
    selectInspector(next.includes(id) ? id : (next[next.length - 1] ?? null))
  }

  const action = async (fn: () => Promise<unknown>, id: number, done?: string) => {
    try {
      await fn()
      setNote(done ?? null)
    } catch (e) {
      const msg = describeError(e)
      if (msg.startsWith('NotFound')) setMissing((m) => new Set(m).add(id))
      setNote(msg.replace(/^NotFound: /, ''))
    }
  }

  const onKey = (e: React.KeyboardEvent) => {
    const current = selected[selected.length - 1]
    const i = rows.findIndex((r) => r.id === current)
    const go = (n: number) => {
      const idx = Math.max(0, Math.min(rows.length - 1, n))
      const r = rows[idx]
      if (!r) return
      set({ selected: [r.id] })
      selectInspector(r.id)
      anchor.current = idx
      virt.scrollToIndex(idx)
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      go(i + 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      go(i < 0 ? 0 : i - 1)
    } else if (e.key === ' ' && current !== undefined && scanId !== null) {
      e.preventDefault()
      void action(() => commands.quickLook(scanId, current), current)
    } else if (e.key === 'Enter' && current !== undefined && scanId !== null) {
      void action(() => commands.revealInFinder(scanId, current), current)
    }
  }

  if (!ready) {
    return (
      <section className="flex h-full flex-col items-center justify-center gap-4 p-8 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent">
          <Icon name="find" size={26} />
        </div>
        <h1 className="text-2xl font-semibold">
          {phase === 'running' ? 'Scanning…' : 'Scan once, then find anything'}
        </h1>
        <p className="max-w-sm text-muted">
          Search every file and folder by name, hidden ones included, with results as you type.
        </p>
        {phase !== 'running' && <ScanButton large />}
      </section>
    )
  }

  const filtersOn =
    kind !== null || size !== 'any' || date !== 'any' || hidden !== 'Include' || scope !== null

  return (
    <div className="flex h-full min-h-0 flex-col bg-panel">
      <div className="flex flex-col gap-3 border-b border-line px-5 py-4">
        <div className="flex items-center gap-3">
          <label className="relative flex-1">
            <span className="sr-only">Search by name</span>
            <Icon
              name="find"
              size={16}
              className="absolute top-1/2 left-3 -translate-y-1/2 text-faint"
            />
            <input
              id="find-input"
              ref={input}
              autoFocus
              value={text}
              onChange={(e) => set({ text: e.target.value })}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown') {
                  e.preventDefault()
                  scroller.current?.focus()
                  onKey(e)
                }
                if (e.key === 'Escape') set({ text: '' })
              }}
              placeholder={
                mode === 'Glob'
                  ? 'e.g. *.dmg'
                  : mode === 'Regex'
                    ? 'e.g. ^IMG_\\d+'
                    : 'Search by name'
              }
              spellCheck={false}
              autoCorrect="off"
              className={`h-10 w-full rounded-xl border bg-raised pr-3 pl-9 text-[15px] outline-none focus:border-accent ${patternError ? 'border-bad' : 'border-line-strong'}`}
            />
          </label>
          <div className="seg" role="group" aria-label="Match mode">
            {MODES.map((m) => (
              <button
                key={m.value}
                type="button"
                title={m.hint}
                aria-pressed={mode === m.value}
                onClick={() => set({ mode: m.value })}
              >
                {m.label}
              </button>
            ))}
          </div>
        </div>
        {patternError && (
          <p role="alert" className="text-bad">
            That pattern is not valid: {patternError.split('\n').pop()}
          </p>
        )}
        <div className="flex flex-wrap items-center gap-2">
          <Select<string>
            label="Kind"
            value={kind ?? ''}
            onChange={(v) => set({ kind: v === '' ? null : (v as never) })}
            options={[
              { value: '', label: 'Any kind' },
              ...KIND_ORDER.filter((k) => k !== 'Other').map((k) => ({
                value: k,
                label: KIND_LABEL[k],
              })),
            ]}
          />
          <Select<SizePreset>
            label="Size"
            value={size}
            options={SIZES}
            onChange={(v) => set({ size: v })}
          />
          <Select<DatePreset>
            label="Modified"
            value={date}
            options={DATES}
            onChange={(v) => set({ date: v })}
          />
          <Select<HiddenMode>
            label="Hidden files"
            value={hidden}
            options={HIDDEN}
            onChange={(v) => set({ hidden: v })}
          />
          {scope && (
            <span className="chip" aria-pressed="true">
              <Icon name="folder" size={12} /> In {scope.name}
              <button
                type="button"
                aria-label="Search everywhere"
                onClick={() => set({ scope: null })}
              >
                <Icon name="close" size={11} />
              </button>
            </span>
          )}
          {filtersOn && (
            <button
              type="button"
              className="text-xs text-accent hover:underline"
              onClick={() =>
                set({ kind: null, size: 'any', date: 'any', hidden: 'Include', scope: null })
              }
            >
              Clear filters
            </button>
          )}
          <div className="ml-auto flex items-center gap-1.5">
            <span className="text-xs text-muted">Sort</span>
            <Select<SearchSort>
              label="Sort by"
              value={sort}
              options={SORTS}
              onChange={(v) => set({ sort: v, descending: false })}
            />
            <button
              type="button"
              className="btn btn-ghost h-6 px-1.5"
              aria-label="Reverse order"
              title="Reverse order"
              onClick={() => set({ descending: !descending })}
            >
              <Icon name="sort" size={13} />
            </button>
          </div>
        </div>
      </div>

      {selected.length > 1 && (
        <div className="flex items-center gap-3 border-b border-line bg-accent-soft px-5 py-2">
          <span className="font-semibold">{selected.length} selected</span>
          <button
            type="button"
            className="btn h-6"
            onClick={() => {
              const paths = rows
                .filter((r) => selected.includes(r.id))
                .map((r) => `${r.location ?? ''}/${r.name}`)
              void navigator.clipboard
                .writeText(paths.join('\n'))
                .then(() => setNote('Paths copied.'))
            }}
          >
            <Icon name="copy" size={12} /> Copy paths
          </button>
          <button
            type="button"
            className="btn h-6"
            disabled
            title="Moving to the Trash arrives with Safe Trash (M4)"
          >
            <Icon name="trash" size={12} /> Move to Trash (M4)
          </button>
          <button
            type="button"
            className="ml-auto text-xs text-accent hover:underline"
            onClick={() => set({ selected: [] })}
          >
            Deselect
          </button>
        </div>
      )}

      <div
        ref={scroller}
        tabIndex={0}
        role="listbox"
        aria-multiselectable="true"
        aria-label="Search results"
        onKeyDown={onKey}
        className="min-h-0 flex-1 overflow-auto outline-offset-[-2px]"
      >
        {error && <p className="p-5 text-bad">{error}</p>}
        {!error && !patternError && rows.length === 0 && !busy && (
          <div className="flex flex-col items-center gap-2 p-16 text-center text-muted">
            <Icon name="find" size={28} />
            <p className="text-base">
              {text || filtersOn
                ? 'Nothing matches.'
                : 'Type to search every scanned file and folder.'}
            </p>
            <p className="text-xs">
              {text || filtersOn
                ? 'Try fewer filters, or a different match mode.'
                : 'Tip: press ⌘F from anywhere. Use Glob or Regex for patterns.'}
            </p>
          </div>
        )}
        <div style={{ height: virt.getTotalSize(), position: 'relative' }}>
          {items.map((v) => {
            const r = rows[v.index]
            if (!r || scanId === null) return null
            const isSel = selected.includes(r.id)
            const gone = missing.has(r.id)
            return (
              <div
                key={r.id}
                role="option"
                aria-selected={isSel}
                onClick={(e) => click(e, v.index, r.id)}
                onDoubleClick={() => void action(() => commands.openNode(scanId, r.id), r.id)}
                className={`group absolute inset-x-0 flex items-center gap-3 px-5 ${isSel ? 'bg-selected' : 'hover:bg-hover'} ${gone ? 'opacity-50' : ''}`}
                style={{ top: v.start, height: ROW }}
              >
                <EntryIcon kind={r.file_kind} isDir={r.kind === 'Dir'} isPackage={r.is_package} />
                <div className="min-w-0 flex-1">
                  <p className={`truncate ${gone ? 'line-through' : ''}`} title={r.name}>
                    <Name row={r} text={text} mode={mode} />
                    {gone && <span className="badge badge-review ml-2 no-underline">Missing</span>}
                  </p>
                  <p className="truncate text-xs text-muted" title={r.location ?? ''}>
                    {middleTruncate(tildify(r.location), 80)}
                  </p>
                </div>
                <div className="hidden items-center gap-1 group-hover:flex">
                  <button
                    type="button"
                    className="btn btn-ghost h-6 px-1.5"
                    title="Reveal in Finder"
                    aria-label="Reveal in Finder"
                    onClick={(e) => {
                      e.stopPropagation()
                      void action(() => commands.revealInFinder(scanId, r.id), r.id)
                    }}
                  >
                    <Icon name="reveal" size={13} />
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost h-6 px-1.5"
                    title="Quick Look"
                    aria-label="Quick Look"
                    onClick={(e) => {
                      e.stopPropagation()
                      void action(() => commands.quickLook(scanId, r.id), r.id)
                    }}
                  >
                    <Icon name="eye" size={13} />
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost h-6 px-1.5"
                    title="Copy path"
                    aria-label="Copy path"
                    onClick={(e) => {
                      e.stopPropagation()
                      void navigator.clipboard
                        .writeText(`${r.location ?? ''}/${r.name}`)
                        .then(() => setNote('Path copied.'))
                    }}
                  >
                    <Icon name="copy" size={13} />
                  </button>
                </div>
                <span className="num w-24 text-right text-muted">{formatAge(r.mtime)}</span>
                <span className="num w-20 text-right font-semibold">{formatBytes(r.size)}</span>
              </div>
            )
          })}
        </div>
      </div>

      <div
        className="flex items-center gap-3 border-t border-line px-5 py-1.5 text-xs text-muted"
        aria-live="polite"
      >
        <span className="num">
          {busy && rows.length === 0
            ? 'Searching…'
            : `${formatCount(total)} result${total === 1 ? '' : 's'}`}
          {truncated && ` · showing the first ${formatCount(10_000)}, refine to see more`}
        </span>
        {note && <span className="ml-auto">{note}</span>}
      </div>
    </div>
  )
}
