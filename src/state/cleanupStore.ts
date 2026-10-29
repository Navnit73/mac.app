import { create } from 'zustand'
import { commands, describeError } from '../lib/ipc'
import type { CategoryId } from '../types/bindings/CategoryId'
import type { CleanupItem } from '../types/bindings/CleanupItem'
import type { CleanupSummary } from '../types/bindings/CleanupSummary'
import type { SelectMode } from '../types/bindings/SelectMode'
import { useScanStore } from './scanStore'
import { useTrashStore } from './trashStore'

interface ItemList {
  items: CleanupItem[]
  total: number
  nextPage: number | null
  loading: boolean
}

interface CleanupStore {
  summary: CleanupSummary | null
  loading: boolean
  error: string | null
  expanded: Set<CategoryId>
  lists: Partial<Record<CategoryId, ItemList>>
  /** Load (or reload) the analysis. `refreshApps` re-reads which apps are open. */
  load: (refreshApps: boolean) => Promise<void>
  toggleExpanded: (c: CategoryId) => void
  loadItems: (c: CategoryId, more: boolean) => Promise<void>
  setSelected: (item: CleanupItem, on: boolean, confirmed?: boolean) => Promise<void>
  selectCategory: (c: CategoryId | null, mode: SelectMode) => Promise<void>
  /** Open the Review sheet for the current selection. */
  review: () => Promise<void>
}

const scanId = () => useScanStore.getState().scanId

export const useCleanupStore = create<CleanupStore>((set, get) => ({
  summary: null,
  loading: false,
  error: null,
  expanded: new Set(),
  lists: {},

  load: async (refreshApps) => {
    const id = scanId()
    if (id === null) return
    set({ loading: true })
    try {
      const summary = await commands.getCleanup(id, refreshApps)
      set({ summary, error: null })
      // Reload the open categories so their rows match the new analysis.
      await Promise.all([...get().expanded].map((c) => get().loadItems(c, false)))
    } catch (e) {
      set({ error: describeError(e) })
    } finally {
      set({ loading: false })
    }
  },

  toggleExpanded: (c) => {
    const expanded = new Set(get().expanded)
    if (expanded.has(c)) {
      expanded.delete(c)
    } else {
      expanded.add(c)
      if (!get().lists[c]) void get().loadItems(c, false)
    }
    set({ expanded })
  },

  loadItems: async (c, more) => {
    const id = scanId()
    const cur = get().lists[c]
    if (id === null || (more && (!cur || cur.nextPage === null || cur.loading))) return
    const page = more && cur?.nextPage != null ? cur.nextPage : 0
    set((s) => ({
      lists: {
        ...s.lists,
        [c]: { items: cur?.items ?? [], total: cur?.total ?? 0, nextPage: null, loading: true },
      },
    }))
    try {
      const p = await commands.listCleanupItems(id, c, page)
      set((s) => ({
        lists: {
          ...s.lists,
          [c]: {
            items: more ? [...(s.lists[c]?.items ?? []), ...p.items] : p.items,
            total: p.total,
            nextPage: p.next_page,
            loading: false,
          },
        },
      }))
    } catch (e) {
      set((s) => ({
        error: describeError(e),
        lists: { ...s.lists, [c]: { ...(s.lists[c] as ItemList), loading: false } },
      }))
    }
  },

  setSelected: async (item, on, confirmed = false) => {
    const id = scanId()
    if (id === null) return
    try {
      const summary = await commands.setCleanupSelected(id, [item.node_id], on, confirmed)
      set((s) => {
        const list = s.lists[item.category]
        return {
          summary,
          error: null,
          lists: list
            ? {
                ...s.lists,
                [item.category]: {
                  ...list,
                  items: list.items.map((i) =>
                    i.node_id === item.node_id ? { ...i, selected: on } : i,
                  ),
                },
              }
            : s.lists,
        }
      })
    } catch (e) {
      set({ error: describeError(e).replace(/^\w+: /, '') })
    }
  },

  selectCategory: async (c, mode) => {
    const id = scanId()
    if (id === null) return
    try {
      const summary = await commands.selectCleanupCategory(id, c, mode)
      set({ summary, error: null })
      const open = c === null ? [...get().expanded] : get().expanded.has(c) ? [c] : []
      await Promise.all(open.map((x) => get().loadItems(x, false)))
    } catch (e) {
      set({ error: describeError(e) })
    }
  },

  review: async () => {
    const id = scanId()
    if (id === null) return
    try {
      const plan = await commands.buildCleanupPlan(id)
      await useTrashStore.getState().openReview(plan.node_ids, 'Cleanup', {
        action: ({ quitApps }) => commands.executeCleanupPlan(id, plan.node_ids, quitApps),
        note: 'Each item is checked again right before it moves: still inside its folder, not changed since the scan, and not in use by an open app.',
        runningApps: plan.running_apps,
      })
    } catch (e) {
      set({ error: describeError(e).replace(/^\w+: /, '') })
    }
  },
}))
