import { create } from 'zustand'
import { commands, describeError, events } from '../lib/ipc'
import type { DupDoneEvent } from '../types/bindings/DupDoneEvent'
import type { DupGroup } from '../types/bindings/DupGroup'
import type { DupOptions } from '../types/bindings/DupOptions'
import type { DupProgress } from '../types/bindings/DupProgress'
import type { DupSavings } from '../types/bindings/DupSavings'
import type { DupSummary } from '../types/bindings/DupSummary'
import type { FileKind } from '../types/bindings/FileKind'
import type { KeepRule } from '../types/bindings/KeepRule'
import { useScanStore } from './scanStore'

/** setup: choose options. running: staged progress. results: groups to review. */
export type DupPhase = 'setup' | 'running' | 'results'

const NO_SAVINGS: DupSavings = { bytes: 0, files: 0, groups: 0 }
/** Groups shown open when results first load, so the list explains itself. */
const OPEN_FIRST = 4

export const DEFAULT_DUP_OPTIONS: DupOptions = {
  min_size: 1_000_000,
  kinds: [],
  scope: null,
  include_media_libraries: false,
  include_app_data: false,
}

interface DupesStore {
  /** File scan these results belong to; results of another scan are dropped. */
  forScan: number | null
  phase: DupPhase
  options: DupOptions
  progress: DupProgress | null
  summary: DupSummary | null
  error: string | null
  groups: DupGroup[]
  total: number
  nextPage: number | null
  loading: boolean
  kind: FileKind | null
  /** The rule last applied to every group (Rust applies Oldest after each scan). */
  rule: KeepRule
  savings: DupSavings
  potential: number
  expanded: Set<number>
  /** Replace copies with APFS clones instead of only moving them (advanced, off by default). */
  clones: boolean
  /** Bumped when the list is reloaded from the top, so the view scrolls back up. */
  listVersion: number

  /** Pick up the state Rust holds for this scan (results survive leaving the room). */
  sync: (scanId: number) => Promise<void>
  setOptions: (o: Partial<DupOptions>) => void
  start: () => Promise<void>
  cancel: () => Promise<void>
  /** Back to the options, keeping nothing on screen (results stay in Rust until the next run). */
  editOptions: () => void
  onProgress: (p: DupProgress) => void
  onDone: (e: DupDoneEvent) => void
  reload: () => Promise<void>
  loadMore: () => Promise<void>
  setKind: (k: FileKind | null) => Promise<void>
  applyRule: (rule: KeepRule, folderNode?: number) => Promise<void>
  toggle: (groupId: number, nodeId: number, on: boolean) => Promise<void>
  toggleExpanded: (groupId: number) => void
  setClones: (on: boolean) => void
  /** Called after a removal: refresh totals and the list. */
  afterRemoval: () => Promise<void>
}

const scanId = () => useScanStore.getState().scanId

/** Reclaimable bytes of one group, recomputed locally after a checkbox change. */
function withSelection(g: DupGroup, nodeId: number, on: boolean): DupGroup {
  const files = g.files.map((f) => (f.node_id === nodeId ? { ...f, selected: on } : f))
  return {
    ...g,
    files,
    reclaimable: files.filter((f) => f.selected).reduce((a, f) => a + f.private_size, 0),
  }
}

export const useDupesStore = create<DupesStore>((set, get) => ({
  forScan: null,
  phase: 'setup',
  options: DEFAULT_DUP_OPTIONS,
  progress: null,
  summary: null,
  error: null,
  groups: [],
  total: 0,
  nextPage: null,
  loading: false,
  kind: null,
  rule: 'Oldest',
  savings: NO_SAVINGS,
  potential: 0,
  expanded: new Set(),
  clones: false,
  listVersion: 0,

  sync: async (id) => {
    if (get().forScan !== id) {
      set({
        forScan: id,
        phase: 'setup',
        progress: null,
        summary: null,
        error: null,
        groups: [],
        total: 0,
        nextPage: null,
        savings: NO_SAVINGS,
        potential: 0,
        rule: 'Oldest',
      })
    }
    try {
      const st = await commands.getDuplicateStatus(id)
      if (st.running) {
        set({ phase: 'running' })
      } else if (st.summary) {
        const fresh = get().phase !== 'results'
        set({
          phase: 'results',
          summary: st.summary,
          savings: st.savings,
          potential: st.potential_bytes,
        })
        if (fresh) await get().reload()
      }
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  setOptions: (o) => set((s) => ({ options: { ...s.options, ...o } })),

  start: async () => {
    const id = scanId()
    if (id === null) return
    set({ phase: 'running', progress: null, error: null, forScan: id })
    try {
      await commands.startDuplicateScan(id, get().options)
    } catch (e) {
      set({ phase: 'setup', error: describeError(e) })
    }
  },

  cancel: async () => {
    try {
      await commands.cancelDuplicateScan()
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  editOptions: () => set({ phase: 'setup', error: null }),

  onProgress: (progress) => {
    if (get().phase === 'running') set({ progress })
  },

  onDone: (e) => {
    if (e.error || !e.summary) {
      set({ phase: 'setup', error: e.error ?? 'The duplicate scan did not finish.' })
      return
    }
    set({
      phase: 'results',
      summary: e.summary,
      potential: e.summary.potential_bytes,
      rule: 'Oldest',
      kind: null,
      clones: false,
    })
    void get()
      .reload()
      .then(async () => {
        const id = scanId()
        if (id === null) return
        const st = await commands.getDuplicateStatus(id)
        set({ savings: st.savings, potential: st.potential_bytes })
      })
  },

  reload: async () => {
    const id = scanId()
    if (id === null) return
    set({ loading: true })
    try {
      const p = await commands.getDuplicateGroups(id, 0, get().kind)
      set((s) => ({
        groups: p.groups,
        total: p.total,
        nextPage: p.next_page,
        error: null,
        listVersion: s.listVersion + 1,
        expanded: new Set(p.groups.slice(0, OPEN_FIRST).map((g) => g.id)),
      }))
    } catch (e) {
      set({ error: describeError(e) })
    } finally {
      set({ loading: false })
    }
  },

  loadMore: async () => {
    const id = scanId()
    const { nextPage, loading, kind } = get()
    if (id === null || nextPage === null || loading) return
    set({ loading: true })
    try {
      const p = await commands.getDuplicateGroups(id, nextPage, kind)
      set((s) => {
        // The order follows reclaimable bytes, which can shift between pages: skip repeats.
        const seen = new Set(s.groups.map((g) => g.id))
        return {
          groups: [...s.groups, ...p.groups.filter((g) => !seen.has(g.id))],
          total: p.total,
          nextPage: p.next_page,
        }
      })
    } catch (e) {
      set({ error: describeError(e) })
    } finally {
      set({ loading: false })
    }
  },

  setKind: async (kind) => {
    set({ kind })
    await get().reload()
  },

  applyRule: async (rule, folderNode) => {
    const id = scanId()
    if (id === null) return
    try {
      const savings = await commands.applyKeepRule(id, rule, folderNode ?? null, null)
      set({ rule, savings, error: null })
      await get().reload()
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  toggle: async (groupId, nodeId, on) => {
    const id = scanId()
    if (id === null) return
    try {
      const savings = await commands.setDuplicateSelected(id, groupId, nodeId, on)
      set((s) => ({
        savings,
        error: null,
        groups: s.groups.map((g) => (g.id === groupId ? withSelection(g, nodeId, on) : g)),
      }))
    } catch (e) {
      set({ error: describeError(e).replace(/^\w+: /, '') })
    }
  },

  toggleExpanded: (groupId) =>
    set((s) => {
      const expanded = new Set(s.expanded)
      if (expanded.has(groupId)) expanded.delete(groupId)
      else expanded.add(groupId)
      return { expanded }
    }),

  setClones: (clones) => set({ clones }),

  afterRemoval: async () => {
    const id = scanId()
    if (id === null) return
    try {
      const st = await commands.getDuplicateStatus(id)
      set({ savings: st.savings, potential: st.potential_bytes })
    } catch (e) {
      set({ error: describeError(e) })
    }
    await get().reload()
  },
}))

/** Subscribe to duplicate scan events once for the app's lifetime. */
export function initDupesListeners(): () => void {
  const { onProgress, onDone } = useDupesStore.getState()
  const pending = [events.onDupesProgress(onProgress), events.onDupesDone(onDone)]
  return () => {
    for (const p of pending) void p.then((un) => un())
  }
}
