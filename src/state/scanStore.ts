import { homeDir } from '@tauri-apps/api/path'
import { create } from 'zustand'
import { commands, describeError, events } from '../lib/ipc'
import type { ScanDoneEvent } from '../types/bindings/ScanDoneEvent'
import type { ScanProgress } from '../types/bindings/ScanProgress'
import type { ScanProgressEvent } from '../types/bindings/ScanProgressEvent'
import type { ScanSummary } from '../types/bindings/ScanSummary'
import { useFindStore } from './findStore'
import { useSpaceStore } from './spaceStore'

export type ScanPhase = 'idle' | 'running' | 'done' | 'cancelled' | 'failed'
export type ScanScope = 'home' | 'disk'

interface ScanStore {
  scanId: number | null
  phase: ScanPhase
  progress: ScanProgress | null
  summary: ScanSummary | null
  error: string | null
  /** Roots of the current or last scan, for display. */
  roots: string[]
  scope: ScanScope
  /** Bumped whenever files change (Trash, Undo) so views refetch. */
  dataVersion: number
  bumpData: () => void
  setScope: (s: ScanScope) => void
  /** Scan the folder chosen by `scope`. */
  startSelected: () => Promise<void>
  start: (roots: string[]) => Promise<void>
  cancel: () => Promise<void>
  onProgress: (e: ScanProgressEvent) => void
  onDone: (e: ScanDoneEvent) => void
}

/** A single-root scan puts its root at node 1; several roots hang off the virtual root 0. */
export function rootNodeId(roots: string[]): number {
  return roots.length === 1 ? 1 : 0
}

export const useScanStore = create<ScanStore>((set, get) => ({
  scanId: null,
  phase: 'idle',
  progress: null,
  summary: null,
  error: null,
  roots: [],
  scope: 'home',
  dataVersion: 0,

  bumpData: () => set((s) => ({ dataVersion: s.dataVersion + 1 })),
  setScope: (scope) => set({ scope }),

  startSelected: async () => {
    const roots =
      get().scope === 'disk' ? ['/'] : [await homeDir().then((h) => h.replace(/\/$/, ''))]
    await get().start(roots)
  },

  start: async (roots) => {
    set({ phase: 'running', progress: null, summary: null, error: null, roots })
    try {
      const id = await commands.startScan(roots)
      set({ scanId: id })
    } catch (e) {
      set({ phase: 'failed', error: describeError(e) })
    }
  },

  cancel: async () => {
    const id = get().scanId
    if (id === null) return
    try {
      await commands.cancelScan(id)
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  onProgress: (e) => {
    // Ignore events of a previous scan.
    const id = get().scanId
    if (id !== null && e.id !== id) return
    set({ progress: e.progress })
  },

  onDone: (e) => {
    const phase = e.status === 'Done' ? 'done' : e.status === 'Cancelled' ? 'cancelled' : 'failed'
    set({ scanId: e.id, phase, summary: e.summary, error: e.error })
    if (phase !== 'failed') {
      useSpaceStore.getState().reset(rootNodeId(get().roots))
      // Node ids belong to one scan; a kept selection or scope would point at other files.
      useFindStore.getState().set({ selected: [], scope: null })
    }
  },
}))

/** Subscribe to scan events once for the app's lifetime. Returns an unsubscribe function. */
export function initScanListeners(): () => void {
  const { onProgress, onDone } = useScanStore.getState()
  const pending = [events.onScanProgress(onProgress), events.onScanDone(onDone)]
  return () => {
    for (const p of pending) void p.then((un) => un())
  }
}
