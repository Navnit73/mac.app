import { create } from 'zustand'

export type SpaceView = 'list' | 'sunburst' | 'treemap'
export type SpaceTab = 'folders' | 'large'
export type ColorBy = 'folder' | 'kind' | 'age'

interface SpaceStore {
  view: SpaceView
  tab: SpaceTab
  colorBy: ColorBy
  /** Folder currently shown (node id), or null before the first scan finishes. */
  currentId: number | null
  back: number[]
  forward: number[]
  selectedId: number | null
  setView: (v: SpaceView) => void
  setTab: (t: SpaceTab) => void
  setColorBy: (c: ColorBy) => void
  /** Show a folder; pushes the previous one on the back stack. */
  navigate: (id: number) => void
  goBack: () => void
  goForward: () => void
  select: (id: number | null) => void
  /** Forget everything about a previous scan. */
  reset: (rootId: number | null) => void
}

export const useSpaceStore = create<SpaceStore>((set, get) => ({
  view: 'list',
  tab: 'folders',
  colorBy: 'folder',
  currentId: null,
  back: [],
  forward: [],
  selectedId: null,
  setView: (view) => set({ view }),
  setTab: (tab) => set({ tab }),
  setColorBy: (colorBy) => set({ colorBy }),
  navigate: (id) => {
    const { currentId, back } = get()
    if (id === currentId) return
    set({
      currentId: id,
      back: currentId === null ? back : [...back, currentId],
      forward: [],
      selectedId: null,
    })
  },
  goBack: () => {
    const { back, currentId, forward } = get()
    const prev = back[back.length - 1]
    if (prev === undefined) return
    set({
      currentId: prev,
      back: back.slice(0, -1),
      forward: currentId === null ? forward : [currentId, ...forward],
      selectedId: null,
    })
  },
  goForward: () => {
    const { forward, currentId, back } = get()
    const next = forward[0]
    if (next === undefined) return
    set({
      currentId: next,
      forward: forward.slice(1),
      back: currentId === null ? back : [...back, currentId],
      selectedId: null,
    })
  },
  select: (selectedId) => set({ selectedId }),
  reset: (rootId) => set({ currentId: rootId, back: [], forward: [], selectedId: null }),
}))
