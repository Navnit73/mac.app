/**
 * Development-only stand-in for the Tauri bridge, so the UI can be opened in a plain browser
 * (and screenshotted) with realistic data. Never bundled into release builds: it is imported
 * dynamically behind `import.meta.env.DEV`.
 *
 * URL options: ?state=done|running|idle &fda=granted|denied &theme=dark|light
 *              &view=list|sunburst|treemap &tab=folders|large &select=<node id>
 */

interface MNode {
  id: number
  parent: number
  name: string
  dir: boolean
  own: number
  mtimeDays: number
  total: number
  files: number
  children: number[]
  pkg?: boolean
}

const GB = 1_000_000_000
const MB = 1_000_000
const NOW = Math.floor(Date.now() / 1000)
const nodes: MNode[] = []
let seed = 7
const rnd = () => {
  seed = (seed * 1664525 + 1013904223) % 4294967296
  return seed / 4294967296
}

function add(
  parent: number,
  name: string,
  dir: boolean,
  own = 0,
  mtimeDays = 5,
  pkg = false,
): number {
  const id = nodes.length
  nodes.push({ id, parent, name, dir, own, mtimeDays, total: 0, files: 0, children: [], pkg })
  const p = nodes[parent]
  if (id !== 0 && p) p.children.push(id)
  return id
}

function build() {
  add(-1, '', true) // virtual root, id 0
  const home = add(0, '/Users/demo', true)
  const lib = add(home, 'Library', true)
  const caches = add(lib, 'Caches', true)
  const brew = add(caches, 'Homebrew', true)
  add(brew, 'downloads', false, 2.1 * GB, 80)
  add(brew, 'api', false, 380 * MB, 20)
  for (const [n, s] of [
    ['com.apple.Safari', 1.4 * GB],
    ['Google', 3.2 * GB],
    ['com.spotify.client', 2.7 * GB],
    ['Slack', 820 * MB],
    ['com.apple.dt.Xcode', 1.1 * GB],
    ['pip', 640 * MB],
  ] as [string, number][]) {
    const d = add(caches, n, true)
    add(d, 'Cache.db', false, s * 0.7, 12)
    add(d, 'blobs', false, s * 0.3, 40)
  }
  const dev = add(lib, 'Developer', true)
  const xcode = add(dev, 'Xcode', true)
  const derived = add(xcode, 'DerivedData', true)
  for (const n of ['DuoBolt-abc', 'Website-def', 'Notes-123', 'Games-9f2']) {
    const d = add(derived, n, true)
    add(d, 'Build', false, (1 + rnd() * 5) * GB, 30)
    add(d, 'Index', false, (0.3 + rnd()) * GB, 10)
  }
  const sims = add(dev, 'CoreSimulator', true)
  add(sims, 'Devices', false, 14.5 * GB, 60)
  const logs = add(lib, 'Logs', true)
  add(logs, 'DiagnosticReports', false, 420 * MB, 20)
  add(logs, 'app.log', false, 80 * MB, 2)
  const appsup = add(lib, 'Application Support', true)
  for (const [n, s] of [
    ['Code', 6.2 * GB],
    ['Slack', 2.4 * GB],
    ['Adobe', 11.8 * GB],
    ['Steam', 28 * GB],
  ] as [string, number][]) {
    const d = add(appsup, n, true)
    add(d, 'data', false, s * 0.8, 15)
    add(d, 'plugins', false, s * 0.2, 200)
  }
  add(lib, 'Mail', true)

  const dl = add(home, 'Downloads', true)
  add(dl, 'Xcode_16.xip', false, 3.6 * GB, 400)
  add(dl, 'macOS-installer.dmg', false, 1.9 * GB, 240)
  add(dl, 'node-v22.pkg', false, 90 * MB, 120)
  add(dl, 'backup-2024.zip', false, 2.8 * GB, 500)
  add(dl, 'invoice.pdf', false, 220_000, 3)
  add(dl, 'photos-export.zip', false, 1.2 * GB, 10)

  const movies = add(home, 'Movies', true)
  add(movies, 'Holiday 2023.mov', false, 9.4 * GB, 700)
  add(movies, 'Wedding.mp4', false, 6.1 * GB, 900)
  add(movies, 'Screen Recording.mov', false, 1.3 * GB, 6)
  add(movies, 'Drone clips', false, 4.7 * GB, 300)

  const pics = add(home, 'Pictures', true)
  const photos = add(pics, 'Photos Library.photoslibrary', true, 0, 2, true)
  add(photos, 'originals', false, 44 * GB, 2)
  add(photos, 'database', false, 1.4 * GB, 2)
  add(pics, 'Screenshots', false, 1.1 * GB, 20)

  const docs = add(home, 'Documents', true)
  for (const [n, s, a] of [
    ['Thesis.pdf', 38 * MB, 300],
    ['Budget.xlsx', 4 * MB, 15],
    ['Slides.key', 420 * MB, 90],
    ['Archive', 5.4 * GB, 800],
  ] as [string, number, number][]) {
    add(docs, n, n === 'Archive' ? false : false, s, a)
  }
  const code = add(home, 'Code', true)
  for (const n of ['duobolt', 'website', 'ml-experiments', 'dotfiles']) {
    const d = add(code, n, true)
    add(d, 'node_modules', false, (0.4 + rnd() * 2) * GB, 90)
    add(d, 'target', false, (0.2 + rnd() * 6) * GB, 8)
    add(d, 'src', false, (5 + rnd() * 30) * MB, 1)
  }
  const music = add(home, 'Music', true)
  add(music, 'Library', false, 12.2 * GB, 120)
  add(home, 'huge-vm.vmdk', false, 31 * GB, 500)
  const trash = add(home, '.Trash', true)
  add(trash, 'old stuff', false, 1.7 * GB, 14)

  // Totals, bottom-up (children always have larger ids than parents).
  for (let i = nodes.length - 1; i >= 0; i--) {
    const n = nodes[i]
    if (!n) continue
    n.total += n.own
    n.files += n.dir ? 0 : 1
    const p = nodes[n.parent]
    if (p) {
      p.total += n.total
      p.files += n.files
    }
  }
}
build()

const kindOf = (n: MNode): string => {
  if (n.dir && !n.pkg) return 'Folder'
  const ext = n.name.split('.').pop()?.toLowerCase() ?? ''
  if (['mov', 'mp4'].includes(ext)) return 'Video'
  if (['zip', 'xip'].includes(ext)) return 'Archives'
  if (['dmg', 'vmdk', 'pkg'].includes(ext)) return 'DiskImages'
  if (['pdf', 'xlsx', 'key'].includes(ext)) return 'Documents'
  if (ext === 'photoslibrary') return 'Images'
  return n.dir ? 'Folder' : 'Other'
}

const row = (n: MNode, parentTotal: number, loc = false) => ({
  id: n.id,
  name: n.name.startsWith('/') ? (n.name.split('/').pop() ?? n.name) : n.name,
  kind: n.dir && !n.pkg ? 'Dir' : n.pkg ? 'Dir' : 'File',
  file_kind: kindOf(n),
  is_package: !!n.pkg,
  is_hidden: n.name.startsWith('.'),
  is_dataless: false,
  is_unreadable: n.name === 'Mail' || n.name === '.Trash',
  size: n.total,
  size_logical: n.total,
  file_count: n.files,
  mtime: NOW - n.mtimeDays * 86400,
  parent_fraction: parentTotal ? n.total / parentTotal : 0,
  has_children: n.children.length > 0,
  location: loc ? pathOf(n.parent) : null,
})

function pathOf(id: number): string {
  const parts: string[] = []
  for (let cur = id; cur > 0;) {
    const n = nodes[cur]
    if (!n) break
    parts.push(n.name)
    cur = n.parent
  }
  return parts.reverse().join('/')
}

const age = (days: number) => (days < 30 ? 'Recent' : days < 365 ? 'ThisYear' : 'Old')

function chart(id: number, depth: number, level = 0): unknown {
  const n = nodes[id]
  if (!n) return null
  const kids = n.children
    .map((c) => nodes[c])
    .filter((c): c is MNode => !!c && c.total > 0)
    .sort((a, b) => b.total - a.total)
  const keep = level < depth ? kids.filter((k) => k.total >= n.total * 0.012).slice(0, 14) : []
  const rest = kids.filter((k) => !keep.includes(k))
  const children = keep.map((k) =>
    k.pkg ? { ...(chart(k.id, 0, depth) as object) } : chart(k.id, depth, level + 1),
  )
  const restSum = rest.reduce((a, k) => a + k.total, 0)
  if (restSum > 0 && level < depth) {
    children.push({
      id: null,
      name: 'Other',
      value: restSum,
      is_dir: false,
      file_kind: 'Other',
      age: 'ThisYear',
      other_count: rest.length,
      children: [],
    })
  }
  return {
    id: n.id,
    name: n.name.startsWith('/') ? (n.name.split('/').pop() ?? n.name) : n.name,
    value: n.total,
    is_dir: n.dir,
    file_kind: kindOf(n),
    age: age(n.mtimeDays),
    other_count: 0,
    children,
  }
}

const params = new URLSearchParams(window.location.search)
const fda = params.get('fda') === 'granted' ? 'Granted' : 'Denied'
type Handler = (a: Record<string, unknown>) => unknown
const homeNode = 1

const listeners = new Map<string, Array<(e: unknown) => void>>()
const callbacks = new Map<number, (e: unknown) => void>()
let cbId = 1
const emit = (event: string, payload: unknown) =>
  (listeners.get(event) ?? []).forEach((cb) => cb({ event, id: 0, payload }))

const home = nodes[homeNode]
const totalAlloc = home?.total ?? 0
let scanState: 'idle' | 'running' | 'done' = 'idle'

const handlers: Record<string, Handler> = {
  'plugin:path|resolve_directory': () => '/Users/demo',
  'plugin:event|listen': (a) => {
    const ev = a.event as string
    const cb = callbacks.get(a.handler as number)
    if (cb) listeners.set(ev, [...(listeners.get(ev) ?? []), cb])
    return 1
  },
  'plugin:event|unlisten': () => null,
  get_fda_status: () => fda,
  open_fda_settings: () => null,
  start_scan: () => {
    scanState = 'running'
    let files = 0
    const t = setInterval(() => {
      files += 41_000
      emit('scan://progress', {
        id: 1,
        progress: {
          files,
          dirs: files / 9,
          bytes: files * 90_000,
          current_path: '/Users/demo/Library/Caches/com.example.app/blobs',
          elapsed_ms: files / 90,
          files_per_sec: 410_000,
        },
      })
    }, 100)
    setTimeout(
      () => {
        clearInterval(t)
        scanState = 'done'
        emit('scan://done', {
          id: 1,
          status: 'Done',
          error: null,
          summary: {
            files: 412_338,
            dirs: 51_204,
            total_alloc: totalAlloc,
            total_logical: totalAlloc,
            skipped: 18,
            cancelled: false,
            elapsed_ms: 1840,
          },
        })
      },
      params.get('state') === 'running' ? 1e9 : 900,
    )
    return 1
  },
  cancel_scan: () => null,
  get_scan_state: () => ({ id: 1, status: scanState }),
  get_scan_report: () => ({
    total: 18,
    next_page: null,
    items: [
      '/Users/demo/.Trash',
      '/Users/demo/Library/Mail',
      '/Users/demo/Library/Messages',
      '/Users/demo/Library/Safari',
      '/Users/demo/Library/Daemon Containers',
      '/Users/demo/Library/HomeKit',
      '/Users/demo/Library/Biome',
    ].map((path) => ({ path, reason: 'PermissionDenied' })),
  }),
  get_overview: () => {
    const used = 612 * GB
    const caches = nodes.find((n) => n.name === 'Caches')
    const brew = nodes.find((n) => n.name === 'Homebrew')
    const derived = nodes.find((n) => n.name === 'DerivedData')
    const sims = nodes.find((n) => n.name === 'CoreSimulator')
    const logs = nodes.find((n) => n.name === 'Logs')
    const dl = nodes.find((n) => n.name === 'Downloads')
    const dev = (brew?.total ?? 0) + (derived?.total ?? 0) + (sims?.total ?? 0)
    const cat = (
      id: string,
      name: string,
      blurb: string,
      bytes: number,
      items: number,
      risk: string,
      analyzed = true,
    ) => ({ id, name, blurb, bytes, items, risk, analyzed })
    const categories = [
      cat(
        'Developer',
        'Developer files',
        'Build output and package caches. Your tools rebuild them.',
        dev,
        1840,
        'Safe',
      ),
      cat(
        'OldDownloads',
        'Old downloads',
        'Items in Downloads you have not touched for 30 days.',
        (dl?.total ?? 0) - 2.4 * GB,
        5,
        'Review',
      ),
      cat(
        'Caches',
        'Caches',
        'App caches. They grow back as you use your apps.',
        (caches?.total ?? 0) - (brew?.total ?? 0),
        2210,
        'Safe',
      ),
      cat('Logs', 'Logs', 'Log files and crash reports.', logs?.total ?? 0, 142, 'Safe'),
      cat(
        'LargeOld',
        'Large old files',
        'Files over 500 MB not modified for a year.',
        46 * GB,
        6,
        'Review',
      ),
      cat(
        'Duplicates',
        'Duplicates',
        'Exact copies of the same file. Arrives with the Duplicates room.',
        0,
        0,
        'Review',
        false,
      ),
      cat(
        'Leftovers',
        'Leftovers',
        'Files from apps you already removed. Arrives with Applications.',
        0,
        0,
        'Review',
        false,
      ),
    ]
    const clearable = categories.filter((c) => c.analyzed).reduce((a, c) => a + c.bytes, 0)
    return {
      volume: {
        name: 'Macintosh HD',
        total: 994 * GB,
        free_raw: 330 * GB,
        available_important: 382 * GB,
        purgeable: 52 * GB,
      },
      roots: ['/Users/demo'],
      covers_volume: params.get('disk') === '1',
      scanned_bytes: totalAlloc,
      categories,
      kept_bytes: totalAlloc - clearable,
      system_other_bytes: used - totalAlloc,
      trash_bytes: fda === 'Granted' ? 1.7 * GB : null,
      files: 412_338,
      dirs: 51_204,
      skipped: 18,
      elapsed_ms: 1840,
      largest_folder: { id: 3, name: 'Library' },
      largest_folder_bytes: nodes[3]?.total ?? 0,
    }
  },
  get_children: (a) => {
    const n = nodes[a.nodeId as number]
    if (!n) throw { kind: 'NotFound', message: 'node' }
    const dir = a.descending ? -1 : 1
    const kids = n.children.map((c) => nodes[c]).filter((c): c is MNode => !!c)
    kids.sort((x, y) => {
      const k = a.sort as string
      const v =
        k === 'Name'
          ? x.name.localeCompare(y.name)
          : k === 'Count'
            ? x.files - y.files
            : k === 'Modified'
              ? y.mtimeDays - x.mtimeDays
              : x.total - y.total
      return v * dir
    })
    return {
      items: kids.map((k) => row(k, n.total)),
      total: kids.length,
      next_cursor: null,
      parent_size: n.total,
    }
  },
  get_top_files: (a) => {
    const f = a.filters as { min_size?: number; kind?: string | null; group?: string | null }
    const files = nodes.filter((n) => (!n.dir || n.pkg) && n.total > 0)
    const hit = files.filter((n) => {
      if (f.kind && kindOf(n) !== f.kind) return false
      if (f.group === 'OldInstallers') return /\.(dmg|pkg)$/.test(n.name) && n.mtimeDays > 30
      if (f.group === 'Videos') return kindOf(n) === 'Video'
      if (f.group === 'Archives') return /\.(zip|xip)$/.test(n.name)
      if (f.group === 'DiskImages') return kindOf(n) === 'DiskImages'
      if (f.group === 'OldDownloads')
        return pathOf(n.id).includes('/Downloads/') && n.mtimeDays > 30
      return n.total >= (f.min_size ?? 0)
    })
    hit.sort((x, y) => y.total - x.total)
    return {
      items: hit.slice(0, 200).map((n) => row(n, 0, true)),
      matched_count: hit.length,
      matched_bytes: hit.reduce((s, n) => s + n.total, 0),
    }
  },
  get_smart_groups: () => [
    { group: 'OldInstallers', count: 2, bytes: 2 * GB },
    { group: 'Archives', count: 3, bytes: 7.6 * GB },
    { group: 'Videos', count: 4, bytes: 21.5 * GB },
    { group: 'DiskImages', count: 3, bytes: 33 * GB },
    { group: 'OldDownloads', count: 5, bytes: 9.2 * GB },
  ],
  get_chart_data: (a) => chart(a.nodeId as number, a.depth as number),
  get_breadcrumb: (a) => {
    const out: { id: number; name: string }[] = []
    for (let cur = a.nodeId as number; cur > 0;) {
      const n = nodes[cur]
      if (!n) break
      out.push({ id: n.id, name: n.name.split('/').pop() ?? n.name })
      cur = n.parent
    }
    return out.reverse()
  },
  get_node_details: (a) => {
    const n = nodes[a.nodeId as number]
    if (!n) throw { kind: 'NotFound', message: 'node' }
    const p = pathOf(n.id)
    return {
      id: n.id,
      name: n.name,
      path: p,
      kind: n.dir ? 'Dir' : 'File',
      file_kind: kindOf(n),
      is_package: !!n.pkg,
      is_hidden: n.name.startsWith('.'),
      exists: true,
      size_logical: n.own || n.total,
      size_alloc: n.own,
      total_alloc: n.total,
      file_count: n.files,
      created: NOW - (n.mtimeDays + 40) * 86400,
      modified: NOW - n.mtimeDays * 86400,
      last_used: n.name.endsWith('.dmg') ? NOW - 300 * 86400 : null,
      owner: 'demo',
      permissions: n.dir ? 'rwxr-xr-x' : 'rw-r--r--',
      hard_links: 1,
      tags: n.name.endsWith('.pdf') ? ['Work', 'Red'] : [],
      where_froms: n.name.endsWith('.dmg') ? ['https://example.com/downloads/installer.dmg'] : [],
      icloud: 'Local',
      protected_reason: null,
      why: p.includes('/Library/Caches')
        ? 'App cache. Apps rebuild these files when they need them.'
        : p.includes('DerivedData')
          ? 'Developer cache or build output. Tools recreate it when you build again.'
          : null,
    }
  },
  search: (a) => {
    const q = a.query as {
      text: string
      mode: string
      filters: { kind?: string | null; min_size?: number | null }
      sort: string
    }
    const t = q.text.normalize('NFC').toLowerCase()
    let re: RegExp | null = null
    if (q.mode === 'Regex' || q.mode === 'Glob') {
      try {
        const src =
          q.mode === 'Glob'
            ? `^${t
                .replace(/[.+^${}()|\\]/g, '\\$&')
                .replace(/\*/g, '.*')
                .replace(/\?/g, '.')}$`
            : t
        re = new RegExp(src, 'i')
      } catch (e) {
        return {
          items: [],
          total: 0,
          next_cursor: null,
          truncated: false,
          pattern_error: String(e),
        }
      }
    }
    const hits = nodes.filter((n) => {
      if (n.id <= 1) return false
      const name = n.name.toLowerCase()
      if (q.filters.kind && kindOf(n) !== q.filters.kind) return false
      if (q.filters.min_size && n.total < q.filters.min_size) return false
      if (!t) return true
      if (re) return re.test(name)
      if (q.mode === 'Prefix') return name.startsWith(t)
      if (q.mode === 'Exact') return name === t
      return name.includes(t)
    })
    if (q.sort === 'Size') hits.sort((x, y) => y.total - x.total)
    if (q.sort === 'Name') hits.sort((x, y) => x.name.localeCompare(y.name))
    return {
      items: hits.slice(0, 200).map((n) => row(n, 0, true)),
      total: hits.length,
      next_cursor: null,
      truncated: false,
      pattern_error: null,
    }
  },
  preview_trash: (a) => {
    const ids = a.nodeIds as number[]
    const items = ids.map((id) => {
      const n = nodes[id]
      const p = pathOf(id)
      return {
        node_id: id,
        name: n?.name ?? '?',
        path: p,
        size: n?.total ?? 0,
        file_kind: n ? kindOf(n) : 'Other',
        is_dir: !!n?.dir,
        risk: p.includes('Caches') || p.includes('DerivedData') ? 'Safe' : 'Review',
        blocked: n?.name === '.Trash' ? 'Already in the Trash.' : null,
      }
    })
    const ok = items.filter((i) => !i.blocked)
    return {
      items,
      allowed_count: ok.length,
      allowed_bytes: ok.reduce((x, i) => x + i.size, 0),
      blocked_count: items.length - ok.length,
    }
  },
  trash_items: (a) => {
    const ids = a.nodeIds as number[]
    const bytes = ids.reduce((x, id) => x + (nodes[id]?.total ?? 0), 0)
    return {
      batch_id: 7,
      moved_count: ids.length,
      moved_bytes: bytes,
      skipped: [
        {
          path: '/Users/demo/Downloads/locked.pkg',
          reason: 'It changed after the scan. Scan again to review it.',
        },
      ],
    }
  },
  undo_batch: () => ({ restored: 3, renamed: 0, conflicts: 0, missing: 0, failed: [] }),
  undo_last: () => ({ restored: 3, renamed: 0, conflicts: 0, missing: 0, failed: [] }),
  prepare_empty_trash: () => ({
    token: 'mock',
    duobolt_items: 14,
    duobolt_bytes: 18.4 * GB,
    trash_bytes: fda === 'Granted' ? 21.1 * GB : null,
  }),
  empty_trash: () => 14,
  list_batches: () =>
    [
      [7, 1, 'Find', 3, 9.2 * GB, 3, 0, 0, 0],
      [6, 3, 'Space', 1, 31 * GB, 0, 0, 1, 0],
      [5, 9, 'Inspector', 12, 2.4 * GB, 4, 8, 0, 0],
      [4, 16, 'Space', 40, 6.8 * GB, 0, 0, 40, 0],
      [3, 30, 'Find', 2, 1.1 * GB, 0, 2, 0, 0],
      [2, 44, 'Space', 5, 12.6 * GB, 0, 0, 5, 0],
      [1, 70, 'Inspector', 1, 0.8 * GB, 0, 0, 1, 0],
    ].map(([id, days, room, item_count, bytes, in_trash, restored, emptied, conflicts]) => ({
      id,
      created_at: NOW - (days as number) * 86400,
      room,
      item_count,
      bytes,
      in_trash,
      restored,
      emptied,
      conflicts,
    })),
  get_batch: (a) => ({
    batch: {
      id: a.batchId,
      created_at: NOW - 86400,
      room: 'Find',
      item_count: 3,
      bytes: 9.2 * GB,
      in_trash: 2,
      restored: 0,
      emptied: 0,
      conflicts: 1,
    },
    items: [
      {
        id: 1,
        original_path: '/Users/demo/Movies/Holiday 2023.mov',
        trashed_path: '/Users/demo/.Trash/Holiday 2023.mov',
        size: 6.1 * GB,
        status: 'InTrash',
        reason: null,
      },
      {
        id: 2,
        original_path: '/Users/demo/Downloads/backup-2024.zip',
        trashed_path: '/Users/demo/.Trash/backup-2024.zip',
        size: 2.8 * GB,
        status: 'Conflict',
        reason: 'Something now exists at the original location.',
      },
      {
        id: 3,
        original_path: '/Users/demo/Downloads/node-v22.pkg',
        trashed_path: '/Users/demo/.Trash/node-v22.pkg',
        size: 0.3 * GB,
        status: 'InTrash',
        reason: null,
      },
    ],
    total: 3,
    next_page: null,
  }),
  get_weekly_stats: () => {
    const week = 7 * 86400
    const start = NOW - (NOW % week) - 11 * week
    const vals = [0, 0.8, 0, 12.6, 0, 1.1, 6.8, 0, 2.4, 31, 0, 9.2]
    return {
      weeks: vals.map((v, i) => ({
        week_start: start + i * week,
        bytes: v * GB,
        batches: v > 0 ? 1 : 0,
      })),
      total_bytes: 63.9 * GB,
      total_batches: 7,
      biggest_bytes: 31 * GB,
      this_week_bytes: 9.2 * GB,
    }
  },
  export_activity_csv: () => '/Users/demo/Downloads/DuoBolt activity 2026-10-29.csv',
  clear_activity: () => null,
  reveal_in_finder: () => null,
  open_node: () => null,
  quick_look: () => null,
  add_exclusion: () => [],
  get_exclusions: () => [],
  remove_exclusion: () => [],
  ping: () => ({ message: 'pong, ui (mock)', core_version: 'mock' }),
  emit_ticks: () => null,
}

export function installMockBridge() {
  const w = window as unknown as Record<string, unknown>
  w.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
      const h = handlers[cmd]
      if (!h) throw { kind: 'Internal', message: `mock: no handler for ${cmd}` }
      return h(args)
    },
    transformCallback: (cb: (e: unknown) => void) => {
      const id = cbId++
      callbacks.set(id, cb)
      return id
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
    convertFileSrc: (p: string) => p,
  }
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => undefined }
  const theme = params.get('theme')
  if (theme) document.documentElement.dataset.theme = theme
}

/** Apply the URL options once the app is mounted. */
export async function applyMockState(stores: {
  scan: { getState: () => { startSelected: () => Promise<void> } }
  space: {
    getState: () => {
      setView: (v: never) => void
      setTab: (t: never) => void
      select: (id: number) => void
      navigate: (id: number) => void
    }
  }
}) {
  const state = params.get('state')
  if (state === 'done' || state === 'running') await stores.scan.getState().startSelected()
  const sp = stores.space.getState()
  const view = params.get('view')
  if (view) sp.setView(view as never)
  const tab = params.get('tab')
  if (tab) sp.setTab(tab as never)
  const nav = params.get('nav')
  const sel = params.get('select')
  const findText = params.get('find')
  const findMode = params.get('mode')
  if (findText !== null || findMode) {
    const { useFindStore } = await import('../state/findStore')
    useFindStore.getState().set({
      ...(findText !== null ? { text: findText } : {}),
      ...(findMode ? { mode: findMode as never } : {}),
    })
  }
  const review = params.get('review')
  const empty = params.get('empty')
  const banner = params.get('banner')
  if (review || empty || banner) {
    const { useTrashStore } = await import('../state/trashStore')
    setTimeout(() => {
      if (review) {
        // Names keep screenshot URLs readable: ?review=Wedding.mp4,.Trash
        const ids = review
          .split(',')
          .map((name) => nodes.find((n) => n.name === name)?.id)
          .filter((id): id is number => id !== undefined)
        void useTrashStore.getState().openReview(ids, 'Find')
      }
      if (empty) void useTrashStore.getState().openEmpty()
      if (banner)
        useTrashStore.setState({
          banner: {
            kind: 'moved',
            result: {
              batch_id: 7,
              moved_count: 3,
              moved_bytes: 9.2 * GB,
              skipped: [
                { path: '/Users/demo/Downloads/locked.pkg', reason: 'It changed after the scan.' },
              ],
            },
          },
        })
    }, 1400)
  }
  setTimeout(() => {
    if (nav) sp.navigate(Number(nav))
    if (sel) sp.select(Number(sel))
  }, 1300)
}
