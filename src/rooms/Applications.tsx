import { convertFileSrc } from '@tauri-apps/api/core'
import { useEffect, useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import Icon from '../components/Icon'
import Modal from '../components/Modal'
import { formatAge, formatBytes, formatCount, formatDate } from '../lib/format'
import { tildify } from '../lib/text'
import { useAppsStore } from '../state/appsStore'
import { useScanStore } from '../state/scanStore'
import type { AppArch } from '../types/bindings/AppArch'
import type { AppSource } from '../types/bindings/AppSource'
import type { AppSummary } from '../types/bindings/AppSummary'
import type { Confidence } from '../types/bindings/Confidence'
import type { RelatedKind } from '../types/bindings/RelatedKind'

type SortBy = 'size' | 'used' | 'name'
const YEAR = 365 * 86_400

const ARCH: Record<AppArch, { label: string; hint: string }> = {
  AppleSilicon: { label: 'Apple silicon', hint: 'Runs natively on this Mac' },
  Universal: { label: 'Universal', hint: 'Runs natively on Apple silicon and Intel' },
  Intel: { label: 'Intel', hint: 'Runs through Rosetta on Apple silicon' },
  Unknown: { label: 'Unknown', hint: 'Could not read the program file' },
}
const SOURCE: Record<AppSource, string> = {
  AppStore: 'App Store',
  System: 'macOS',
  Other: 'Downloaded',
}
const KIND: Record<RelatedKind, string> = {
  Support: 'App data',
  Caches: 'Cache',
  Preferences: 'Settings',
  Containers: 'Container',
  GroupContainers: 'Shared container',
  SavedState: 'Window state',
  Logs: 'Logs',
  LaunchAgents: 'Starts at login',
  HttpStorage: 'Web data',
  WebKit: 'Web data',
  SystemSupport: 'All users',
}
const CONFIDENCE: Record<Confidence, { label: string; hint: string; cls: string }> = {
  Exact: { label: 'Exact match', hint: 'Named exactly after the app', cls: 'badge-safe' },
  Prefix: {
    label: 'Helper',
    hint: 'Named after the app with a suffix, like its helper or extension',
    cls: 'badge-safe',
  },
  Name: {
    label: 'Check',
    hint: 'Matched by the app’s name only. Make sure it is this app’s.',
    cls: 'badge-review',
  },
  Shared: {
    label: 'Shared',
    hint: 'Used by several apps from the same developer, so MyMacStorage keeps it.',
    cls: 'badge-careful',
  },
}

const unused = (a: AppSummary) => a.last_used !== null && Date.now() / 1000 - a.last_used > YEAR

function AppIcon({ app, size }: { app: AppSummary; size: number }) {
  const icon = useAppsStore((s) => s.icons[app.id])
  const loadIcon = useAppsStore((s) => s.loadIcon)
  const [broken, setBroken] = useState(false)
  useEffect(() => {
    if (icon === undefined) loadIcon(app.id)
  }, [icon, app.id, loadIcon])
  if (icon && !broken) {
    return (
      <img
        src={convertFileSrc(icon)}
        alt=""
        width={size}
        height={size}
        onError={() => setBroken(true)}
        className="shrink-0"
      />
    )
  }
  return (
    <span
      className="flex shrink-0 items-center justify-center rounded-lg bg-track text-muted"
      style={{ width: size, height: size }}
    >
      <Icon name="applications" size={size * 0.55} />
    </span>
  )
}

function Details() {
  const details = useAppsStore((s) => s.details)
  const selectedId = useAppsStore((s) => s.selectedId)
  const chosen = useAppsStore((s) => s.chosen)
  const toggleChosen = useAppsStore((s) => s.toggleChosen)
  const select = useAppsStore((s) => s.select)
  const [ask, setAsk] = useState(false)

  if (selectedId === null) return null
  if (!details) {
    return (
      <aside className="w-[400px] shrink-0 border-l border-line bg-panel p-5 text-muted">
        Reading app details…
      </aside>
    )
  }
  const a = details.app
  const chosenBytes = details.related
    .filter((r) => chosen.has(r.node_id))
    .reduce((x, r) => x + r.bytes, 0)
  const total = a.app_bytes + a.data_bytes
  const parts = [
    { label: 'App', bytes: a.app_bytes, color: 'var(--s1)' },
    { label: 'Data', bytes: Math.max(0, a.data_bytes - a.cache_bytes), color: 'var(--s5)' },
    { label: 'Caches', bytes: a.cache_bytes, color: 'var(--s3)' },
  ]

  return (
    <aside
      className="flex w-[400px] shrink-0 flex-col overflow-auto border-l border-line bg-panel"
      aria-label={`${a.name} details`}
    >
      <div className="flex items-start gap-3 border-b border-line p-5">
        <AppIcon app={a} size={48} />
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-base font-semibold">{a.name}</h2>
          <p className="text-xs text-muted">
            {a.version ? `Version ${a.version} · ` : ''}
            {SOURCE[a.source]}
          </p>
          <p className="truncate font-mono text-[11px] text-faint" title={a.bundle_id}>
            {a.bundle_id}
          </p>
        </div>
        <button
          type="button"
          className="btn btn-ghost px-1.5"
          aria-label="Close details"
          onClick={() => void select(null)}
        >
          <Icon name="close" size={13} />
        </button>
      </div>

      <div className="flex flex-col gap-4 p-5">
        <div>
          <p className="eyebrow">Total size</p>
          <p className="num text-2xl font-semibold">{formatBytes(total)}</p>
          <div className="mt-2 flex h-2 overflow-hidden rounded-full bg-track" aria-hidden="true">
            {parts.map((p) => (
              <div
                key={p.label}
                style={{ width: `${total ? (p.bytes / total) * 100 : 0}%`, background: p.color }}
              />
            ))}
          </div>
          <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs">
            {parts.map((p) => (
              <span key={p.label} className="flex items-center gap-1.5">
                <span className="h-2 w-2 rounded-sm" style={{ background: p.color }} />
                {p.label} <span className="num font-semibold">{formatBytes(p.bytes)}</span>
              </span>
            ))}
          </div>
        </div>

        <dl className="grid grid-cols-[110px_1fr] gap-x-3 gap-y-1.5 text-xs">
          <dt className="text-muted">Last opened</dt>
          <dd>
            {a.last_used ? `${formatDate(a.last_used)} (${formatAge(a.last_used)})` : 'Unknown'}
          </dd>
          <dt className="text-muted">Runs on</dt>
          <dd title={ARCH[a.arch].hint}>
            {ARCH[a.arch].label}
            <span className="text-muted"> · {ARCH[a.arch].hint}</span>
          </dd>
          <dt className="text-muted">Signed by</dt>
          <dd className="truncate" title={details.signed_by ?? undefined}>
            {details.signed_by ?? 'Not signed'}
          </dd>
          <dt className="text-muted">Location</dt>
          <dd className="truncate" title={a.path}>
            {tildify(a.path)}
          </dd>
          {details.receipts.length > 0 && (
            <>
              <dt className="text-muted">Installer receipts</dt>
              <dd>{formatCount(details.receipts.length)} (kept, for information)</dd>
            </>
          )}
        </dl>

        <div>
          <div className="flex items-baseline justify-between">
            <h3 className="font-semibold">Related files</h3>
            <span className="num text-xs text-muted">
              {formatCount(details.related.length)} found
            </span>
          </div>
          {details.related.length === 0 ? (
            <p className="mt-1 text-muted">No settings or data found for this app.</p>
          ) : (
            <ul className="mt-2 flex flex-col">
              {details.related.map((r) => {
                const c = CONFIDENCE[r.confidence]
                return (
                  <li
                    key={r.node_id}
                    className="flex items-center gap-2 border-b border-line py-1.5 last:border-0"
                  >
                    <input
                      type="checkbox"
                      aria-label={`Remove ${r.path}`}
                      checked={chosen.has(r.node_id)}
                      disabled={!a.removable || r.confidence === 'Shared'}
                      onChange={() => toggleChosen(r.node_id)}
                    />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-xs" title={r.path}>
                        {r.name}
                      </p>
                      <p className="text-[11px] text-muted">{KIND[r.kind]}</p>
                    </div>
                    <span className={`badge ${c.cls}`} title={c.hint}>
                      {c.label}
                    </span>
                    <span className="num w-16 text-right text-xs font-semibold">
                      {formatBytes(r.bytes)}
                    </span>
                  </li>
                )
              })}
            </ul>
          )}
        </div>

        {a.removable ? (
          <button type="button" className="btn btn-danger self-start" onClick={() => setAsk(true)}>
            <Icon name="trash" size={13} /> Uninstall…
          </button>
        ) : (
          <p className="flex items-center gap-1.5 text-muted">
            <Icon name="lock" size={12} /> {a.locked_reason}
          </p>
        )}
      </div>

      <UninstallDialog
        open={ask}
        onClose={() => setAsk(false)}
        app={a}
        appBytes={a.app_bytes}
        files={chosen.size}
        filesBytes={chosenBytes}
      />
    </aside>
  )
}

function UninstallDialog({
  open,
  onClose,
  app,
  appBytes,
  files,
  filesBytes,
}: {
  open: boolean
  onClose: () => void
  app: AppSummary
  appBytes: number
  files: number
  filesBytes: number
}) {
  const uninstall = useAppsStore((s) => s.uninstall)
  const busy = useAppsStore((s) => s.uninstalling)
  const error = useAppsStore((s) => s.error)
  const [quit, setQuit] = useState(true)
  return (
    <Modal open={open} onClose={() => !busy && onClose()} labelledBy="uninstall-title" width={500}>
      <div className="flex flex-col gap-3 p-6">
        <h2 id="uninstall-title" className="text-lg font-semibold">
          Uninstall {app.name}?
        </h2>
        <p className="text-muted">
          The app and {formatCount(files)} related file{files === 1 ? '' : 's'} go to the Trash (
          {formatBytes(appBytes + filesBytes)}). You can undo this until the Trash is emptied.
        </p>
        {app.running && (
          <label className="flex items-start gap-2 rounded-lg bg-warn-soft px-3 py-2">
            <input
              type="checkbox"
              className="mt-1"
              checked={quit}
              onChange={(e) => setQuit(e.target.checked)}
            />
            <span>
              <span className="font-medium">Quit {app.name} first</span>
              <span className="block text-xs text-muted">
                It is open. MyMacStorage asks it to quit the normal way; it may ask you to save.
              </span>
            </span>
          </label>
        )}
        {error && (
          <p role="alert" className="text-bad">
            {error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <button type="button" className="btn" data-autofocus onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-danger"
            disabled={busy || (app.running && !quit)}
            onClick={() => void uninstall(quit).then((ok) => ok && onClose())}
          >
            <Icon name="trash" size={13} />
            {busy ? 'Moving…' : 'Move to Trash'}
          </button>
        </div>
      </div>
    </Modal>
  )
}

export default function Applications() {
  const navigate = useNavigate()
  const dataVersion = useScanStore((s) => s.dataVersion)
  const { scanning, list, error, selectedId, scan, select } = useAppsStore()
  const [query, setQuery] = useState('')
  const [sortBy, setSortBy] = useState<SortBy>('size')
  const [arch, setArch] = useState<AppArch | 'any'>('any')
  const [source, setSource] = useState<AppSource | 'any'>('any')

  // Read the apps when the room opens and whenever files change (uninstall, Undo).
  useEffect(() => {
    void scan()
  }, [scan, dataVersion])

  const rows = useMemo(() => {
    const q = query.trim().toLowerCase()
    const r = (list?.apps ?? []).filter(
      (a) =>
        (!q || a.name.toLowerCase().includes(q) || a.bundle_id.toLowerCase().includes(q)) &&
        (arch === 'any' || a.arch === arch) &&
        (source === 'any' || a.source === source),
    )
    const size = (a: AppSummary) => a.app_bytes + a.data_bytes
    r.sort((x, y) =>
      sortBy === 'size'
        ? size(y) - size(x)
        : sortBy === 'used'
          ? (x.last_used ?? 0) - (y.last_used ?? 0)
          : x.name.localeCompare(y.name),
    )
    return r
  }, [list, query, sortBy, arch, source])

  if (!list) {
    return (
      <section className="flex h-full flex-col items-center justify-center gap-3 p-8 text-center">
        <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-accent-soft text-accent-ink">
          <Icon name="applications" size={26} />
        </div>
        <h1 className="text-2xl font-semibold">
          {error ? 'Could not read your apps' : 'Reading your apps…'}
        </h1>
        <p className="max-w-sm text-muted">
          {error ??
            'MyMacStorage looks at every app and the settings, caches and data it keeps in your Library.'}
        </p>
        {error && (
          <button type="button" className="btn" onClick={() => void scan()}>
            Try again
          </button>
        )}
      </section>
    )
  }

  const totalBytes = list.apps.reduce((a, x) => a + x.app_bytes + x.data_bytes, 0)

  return (
    <div className="flex h-full min-h-0">
      <div className="flex min-w-0 flex-1 flex-col bg-panel">
        <div className="flex flex-col gap-3 border-b border-line px-5 py-4">
          <div className="flex flex-wrap items-center gap-3">
            <label className="relative min-w-[200px] flex-1">
              <span className="sr-only">Filter apps</span>
              <Icon
                name="find"
                size={14}
                className="absolute top-1/2 left-3 -translate-y-1/2 text-faint"
              />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Filter by name"
                spellCheck={false}
                className="h-8 w-full rounded-lg border border-line-strong bg-raised pr-3 pl-8 outline-none focus:border-accent"
              />
            </label>
            <div className="seg" role="group" aria-label="Sort by">
              {(
                [
                  ['size', 'Size'],
                  ['used', 'Last used'],
                  ['name', 'Name'],
                ] as const
              ).map(([v, l]) => (
                <button
                  key={v}
                  type="button"
                  aria-pressed={sortBy === v}
                  onClick={() => setSortBy(v)}
                >
                  {l}
                </button>
              ))}
            </div>
            <select
              aria-label="Runs on"
              className="btn h-7 pr-6 text-xs"
              value={arch}
              onChange={(e) => setArch(e.target.value as AppArch | 'any')}
            >
              <option value="any">Any chip</option>
              <option value="AppleSilicon">Apple silicon</option>
              <option value="Universal">Universal</option>
              <option value="Intel">Intel only</option>
            </select>
            <select
              aria-label="Source"
              className="btn h-7 pr-6 text-xs"
              value={source}
              onChange={(e) => setSource(e.target.value as AppSource | 'any')}
            >
              <option value="any">Any source</option>
              <option value="AppStore">App Store</option>
              <option value="Other">Downloaded</option>
              <option value="System">macOS</option>
            </select>
          </div>
          {list.leftover_items > 0 && (
            <button
              type="button"
              onClick={() => navigate('/cleanup')}
              className="flex items-center gap-3 rounded-lg bg-accent-soft px-3 py-2 text-left"
            >
              <Icon name="info" size={14} className="text-accent-ink" />
              <span className="flex-1">
                <span className="font-medium">
                  {formatBytes(list.leftover_bytes)} left behind by apps you removed
                </span>
                <span className="text-muted">
                  {' '}
                  · {formatCount(list.leftover_items)} items, listed under Leftovers in Cleanup
                </span>
              </span>
              <Icon name="chevron" size={13} className="text-faint" />
            </button>
          )}
        </div>

        <div className="grid grid-cols-[1fr_110px_120px_110px] gap-3 border-b border-line px-5 py-1.5 text-[11px] font-semibold tracking-wide text-faint uppercase">
          <span>App</span>
          <span>Last opened</span>
          <span>Runs on</span>
          <span className="text-right">Size</span>
        </div>
        <ul className="min-h-0 flex-1 overflow-auto" aria-label="Applications">
          {rows.map((a) => (
            <li key={a.id}>
              <button
                type="button"
                onClick={() => void select(a.id)}
                aria-current={selectedId === a.id}
                className={`grid w-full grid-cols-[1fr_110px_120px_110px] items-center gap-3 px-5 py-2 text-left ${selectedId === a.id ? 'bg-selected' : 'hover:bg-hover'}`}
              >
                <span className="flex min-w-0 items-center gap-3">
                  <AppIcon app={a} size={28} />
                  <span className="min-w-0">
                    <span className="flex items-center gap-2">
                      <span className="truncate font-medium">{a.name}</span>
                      {a.running && (
                        <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-ok" title="Open" />
                      )}
                      {!a.removable && (
                        <Icon
                          name="lock"
                          size={11}
                          className="shrink-0 text-faint"
                          aria-label="Cannot be removed"
                        />
                      )}
                    </span>
                    <span className="block truncate text-xs text-muted">
                      {SOURCE[a.source]}
                      {a.version ? ` · ${a.version}` : ''}
                    </span>
                  </span>
                </span>
                <span className="text-xs">
                  {a.last_used ? formatAge(a.last_used) : '—'}
                  {unused(a) && (
                    <span className="badge badge-review mt-0.5 flex w-fit">Unused 12+ mo</span>
                  )}
                </span>
                <span className="text-xs" title={ARCH[a.arch].hint}>
                  {a.arch === 'Intel' ? (
                    <span className="badge badge-review">Intel</span>
                  ) : (
                    ARCH[a.arch].label
                  )}
                </span>
                <span className="text-right">
                  <span className="num block font-semibold">
                    {formatBytes(a.app_bytes + a.data_bytes)}
                  </span>
                  {a.data_bytes > 0 && (
                    <span className="num block text-[11px] text-muted">
                      {formatBytes(a.data_bytes)} data
                    </span>
                  )}
                </span>
              </button>
            </li>
          ))}
          {rows.length === 0 && <li className="p-10 text-center text-muted">No apps match.</li>}
        </ul>
        <div className="flex items-center gap-3 border-t border-line px-5 py-1.5 text-xs text-muted">
          <span className="num">
            {formatCount(list.apps.length)} apps · {formatBytes(totalBytes)} with their data
          </span>
          {scanning && <span className="ml-auto">Refreshing…</span>}
          {!scanning && (
            <button
              type="button"
              className="ml-auto text-accent-ink hover:underline"
              onClick={() => void scan()}
            >
              Refresh
            </button>
          )}
        </div>
      </div>
      <Details />
    </div>
  )
}
