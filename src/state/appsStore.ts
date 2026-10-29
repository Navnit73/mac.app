import { create } from 'zustand'
import { commands, describeError, events } from '../lib/ipc'
import type { AppDetails } from '../types/bindings/AppDetails'
import type { AppsList } from '../types/bindings/AppsList'
import { useScanStore } from './scanStore'
import { useTrashStore } from './trashStore'

interface AppsStore {
  /** True while Rust reads the apps; the last list stays on screen meanwhile. */
  scanning: boolean
  list: AppsList | null
  error: string | null
  selectedId: number | null
  details: AppDetails | null
  /** Related files (node ids) chosen to go with the app. */
  chosen: Set<number>
  /** Icon file per app id; null when the app has none. */
  icons: Record<number, string | null>
  uninstalling: boolean
  scan: () => Promise<void>
  onDone: (error: string | null) => void
  loadList: () => Promise<void>
  select: (id: number | null) => Promise<void>
  toggleChosen: (nodeId: number) => void
  loadIcon: (id: number) => void
  uninstall: (quit: boolean) => Promise<boolean>
}

const scanId = () => useScanStore.getState().scanId
const iconRequests = new Set<number>()

export const useAppsStore = create<AppsStore>((set, get) => ({
  scanning: false,
  list: null,
  error: null,
  selectedId: null,
  details: null,
  chosen: new Set(),
  icons: {},
  uninstalling: false,

  scan: async () => {
    set({ scanning: true, error: null })
    try {
      await commands.startAppsScan()
    } catch (e) {
      set({ scanning: false, error: describeError(e) })
    }
  },

  onDone: (error) => {
    if (error) {
      set({ scanning: false, error })
      return
    }
    // App ids are positions in the new list, so cached icons and details are stale.
    iconRequests.clear()
    set({ icons: {}, selectedId: null, details: null })
    void get()
      .loadList()
      .finally(() => set({ scanning: false }))
  },

  loadList: async () => {
    try {
      set({ list: await commands.listApps(scanId()), error: null })
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  select: async (id) => {
    set({ selectedId: id, details: null, chosen: new Set() })
    if (id === null) return
    try {
      const details = await commands.getAppDetails(id)
      if (get().selectedId !== id) return
      set({
        details,
        chosen: new Set(details.related.filter((r) => r.suggested).map((r) => r.node_id)),
      })
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  toggleChosen: (nodeId) =>
    set((s) => {
      const chosen = new Set(s.chosen)
      if (chosen.has(nodeId)) chosen.delete(nodeId)
      else chosen.add(nodeId)
      return { chosen }
    }),

  loadIcon: (id) => {
    if (iconRequests.has(id)) return
    iconRequests.add(id)
    commands
      .appIcon(id)
      .then((path) => set((s) => ({ icons: { ...s.icons, [id]: path } })))
      .catch(() => set((s) => ({ icons: { ...s.icons, [id]: null } })))
  },

  uninstall: async (quit) => {
    const { selectedId, details, chosen } = get()
    // The bundle id lets Rust refuse if the id now names a different app.
    if (selectedId === null || details === null || details.app.id !== selectedId) return false
    set({ uninstalling: true, error: null })
    try {
      const result = await commands.uninstallApp(
        selectedId,
        details.app.bundle_id,
        [...chosen],
        quit,
        scanId(),
      )
      useTrashStore.setState({ banner: { kind: 'moved', result } })
      set({ selectedId: null, details: null })
      // Other rooms refresh; this one reads the apps again.
      useScanStore.getState().bumpData()
      return true
    } catch (e) {
      set({ error: describeError(e).replace(/^\w+: /, '') })
      return false
    } finally {
      set({ uninstalling: false })
    }
  },
}))

/** Subscribe to the apps scan event once for the app's lifetime. */
export function initAppsListeners(): () => void {
  const pending = events.onAppsDone((e) => useAppsStore.getState().onDone(e.error))
  return () => void pending.then((un) => un())
}
