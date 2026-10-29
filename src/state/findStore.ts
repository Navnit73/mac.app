import { create } from 'zustand'
import type { FileKind } from '../types/bindings/FileKind'
import type { HiddenMode } from '../types/bindings/HiddenMode'
import type { SearchMode } from '../types/bindings/SearchMode'
import type { SearchQuery } from '../types/bindings/SearchQuery'
import type { SearchSort } from '../types/bindings/SearchSort'

export type SizePreset = 'any' | '1mb' | '100mb' | '1gb'
export type DatePreset = 'any' | 'day' | 'week' | 'month' | 'year' | 'older'

interface FindStore {
  text: string
  mode: SearchMode
  kind: FileKind | null
  size: SizePreset
  date: DatePreset
  hidden: HiddenMode
  scope: { id: number; name: string } | null
  sort: SearchSort
  descending: boolean
  /** Multi-selection of result ids; the Inspector shows the last one clicked. */
  selected: number[]
  /** Bumped to ask the Find field to take focus (⌘F, toolbar). */
  focusTick: number
  set: (patch: Partial<Omit<FindStore, 'set' | 'requestFocus'>>) => void
  requestFocus: () => void
}

export const useFindStore = create<FindStore>((set) => ({
  text: '',
  mode: 'Substring',
  kind: null,
  size: 'any',
  date: 'any',
  hidden: 'Include',
  scope: null,
  sort: 'Relevance',
  descending: false,
  selected: [],
  focusTick: 0,
  set: (patch) => set(patch),
  requestFocus: () => set((s) => ({ focusTick: s.focusTick + 1 })),
}))

const MB = 1_000_000
const DAY = 86_400

/** Build the query sent to Rust from the store's UI-level state. */
export function toQuery(s: FindStore, nowSec = Math.floor(Date.now() / 1000)): SearchQuery {
  const minSize = { any: null, '1mb': MB, '100mb': 100 * MB, '1gb': 1000 * MB }[s.size]
  let after: number | null = null
  let before: number | null = null
  if (s.date === 'day') after = nowSec - DAY
  if (s.date === 'week') after = nowSec - 7 * DAY
  if (s.date === 'month') after = nowSec - 30 * DAY
  if (s.date === 'year') after = nowSec - 365 * DAY
  if (s.date === 'older') before = nowSec - 365 * DAY
  return {
    text: s.text,
    mode: s.mode,
    filters: {
      kind: s.kind,
      min_size: minSize,
      max_size: null,
      // Round to the hour so the query (and its cache key) is stable while typing.
      modified_after: after === null ? null : after - (after % 3600),
      modified_before: before === null ? null : before - (before % 3600),
      hidden: s.hidden,
      scope: s.scope?.id ?? null,
    },
    sort: s.sort,
    descending: s.descending,
  }
}
