import { create } from 'zustand'
import { commands, describeError, events } from '../lib/ipc'
import type { DupDoneEvent } from '../types/bindings/DupDoneEvent'
import type { DupOptions } from '../types/bindings/DupOptions'
import type { DupProgress } from '../types/bindings/DupProgress'
import type { DupSavings } from '../types/bindings/DupSavings'
import type { DupSummary } from '../types/bindings/DupSummary'
import { useScanStore } from './scanStore'

/** idle: choose options. running: staged progress. done: groups to review. */
export type DupPhase = 'idle' | 'running' | 'done'

const NO_SAVINGS: DupSavings = { bytes: 0, files: 0, groups: 0 }

export const DEFAULT_DUP_OPTIONS: DupOptions = {
  min_size: 1_000_000,
  kinds: [],
  scope: null,
  include_media_libraries: false,
  include_app_data: false,
}

interface DupesStore {
  /** File scan the state below belongs to. */
  forScan: number | null
  phase: DupPhase
  /** The user went back to the options; refreshes must not jump to the old results. */
  editing: boolean
  options: DupOptions
  progress: DupProgress | null
  summary: DupSummary | null
  error: string | null
  savings: DupSavings
  potential: number
  /** Bumped whenever the groups may have changed, so the list refetches. */
  version: number
  setOptions: (o: Partial<DupOptions>) => void
  start: () => Promise<void>
  cancel: () => Promise<void>
  editOptions: () => void
  setSavings: (s: DupSavings) => void
  /** Pull the state Rust holds for the current scan. Results survive leaving the room. */
  refresh: () => Promise<void>
  onProgress: (p: DupProgress) => void
  onDone: (e: DupDoneEvent) => void
}

// Responses to a refresh that started before a newer action are ignored.
let generation = 0

export const useDupesStore = create<DupesStore>((set, get) => ({
  forScan: null,
  phase: 'idle',
  editing: false,
  options: DEFAULT_DUP_OPTIONS,
  progress: null,
  summary: null,
  error: null,
  savings: NO_SAVINGS,
  potential: 0,
  version: 0,

  setOptions: (o) => set((s) => ({ options: { ...s.options, ...o } })),

  start: async () => {
    const id = useScanStore.getState().scanId
    if (id === null) return
    generation++
    set({ phase: 'running', progress: null, error: null, editing: false, forScan: id })
    try {
      await commands.startDuplicateScan(id, get().options)
    } catch (e) {
      set({ phase: 'idle', error: describeError(e).replace(/^\w+: /, '') })
    }
  },

  cancel: async () => {
    try {
      await commands.cancelDuplicateScan()
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  editOptions: () => {
    generation++
    set({ phase: 'idle', editing: true, error: null })
  },

  setSavings: (savings) => set({ savings }),

  refresh: async () => {
    const id = useScanStore.getState().scanId
    if (id === null) return
    if (get().forScan !== id) {
      // A new file scan: the old duplicate results no longer apply.
      set({
        forScan: id,
        phase: 'idle',
        editing: false,
        // The scope is a node id of the old scan; in the new one it names another folder.
        options: { ...get().options, scope: null },
        summary: null,
        progress: null,
        savings: NO_SAVINGS,
        potential: 0,
      })
    }
    const mine = ++generation
    try {
      const st = await commands.getDuplicateStatus(id)
      if (mine !== generation) return
      if (st.running) {
        set({ phase: 'running' })
      } else if (st.summary) {
        set((s) => ({
          phase: s.editing ? 'idle' : 'done',
          summary: st.summary,
          savings: st.savings,
          potential: st.potential_bytes,
          version: s.version + 1,
        }))
      } else if (get().phase !== 'running') {
        set({ phase: 'idle' })
      }
    } catch (e) {
      if (mine === generation) set({ error: describeError(e) })
    }
  },

  onProgress: (progress) => {
    if (get().phase === 'running') set({ progress })
  },

  onDone: (e) => {
    generation++
    if (e.error || !e.summary) {
      set({ phase: 'idle', error: e.error ?? 'The duplicate scan did not finish.' })
      return
    }
    set({ phase: 'done', summary: e.summary, potential: e.summary.potential_bytes })
    // Rust applies the default keep rule; fetch the resulting selection.
    void get().refresh()
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
