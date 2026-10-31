import { create } from 'zustand'
import { commands, describeError } from '../lib/ipc'
import type { EmptyTrashPrep } from '../types/bindings/EmptyTrashPrep'
import type { Room } from '../types/bindings/Room'
import type { TrashBatchResult } from '../types/bindings/TrashBatchResult'
import type { TrashPreview } from '../types/bindings/TrashPreview'
import { useFindStore } from './findStore'
import { useScanStore } from './scanStore'
import { useSpaceStore } from './spaceStore'

interface Review {
  /** Scan the node ids belong to, captured when the review opened. */
  scanId: number
  nodeIds: number[]
  room: Room
  /** Replaces the plain move, e.g. Duplicates verifies every copy first. */
  action?: (opts: { quitApps: boolean }) => Promise<TrashBatchResult>
  /** Extra line shown in the sheet, explaining what happens. */
  note?: string
  /** Open apps that own selected items; the sheet offers to quit them first. */
  runningApps?: string[]
  quitApps: boolean
  preview: TrashPreview | null
  error: string | null
  moving: boolean
}

export type Banner =
  { kind: 'moved'; result: TrashBatchResult } | { kind: 'info'; text: string; tone: 'ok' | 'warn' }

interface TrashStore {
  review: Review | null
  banner: Banner | null
  empty: { prep: EmptyTrashPrep | null; error: string | null; busy: boolean } | null
  /** Every destructive action starts here: load the dry run and show the Review sheet. */
  openReview: (
    nodeIds: number[],
    room: Room,
    extra?: {
      action?: (opts: { quitApps: boolean }) => Promise<TrashBatchResult>
      note?: string
      runningApps?: string[]
    },
  ) => Promise<void>
  setQuitApps: (on: boolean) => void
  closeReview: () => void
  confirmReview: () => Promise<void>
  undoBatch: (batchId: number) => Promise<void>
  undoLast: () => Promise<void>
  openEmpty: () => Promise<void>
  closeEmpty: () => void
  confirmEmpty: () => Promise<void>
  dismissBanner: () => void
}

const scanId = () => useScanStore.getState().scanId

export const useTrashStore = create<TrashStore>((set, get) => ({
  review: null,
  banner: null,
  empty: null,

  openReview: async (nodeIds, room, extra) => {
    const id = scanId()
    if (id === null || nodeIds.length === 0) return
    // Never replace a review whose move is in flight: its button would come back enabled.
    if (get().review?.moving) return
    set({
      review: {
        scanId: id,
        nodeIds,
        room,
        preview: null,
        error: null,
        moving: false,
        quitApps: false,
        ...extra,
      },
    })
    try {
      const preview = await commands.previewTrash(id, nodeIds)
      // Only the review this request was made for: a newer one may have replaced it.
      set((s) => (s.review?.nodeIds === nodeIds ? { review: { ...s.review, preview } } : s))
    } catch (e) {
      set((s) =>
        s.review?.nodeIds === nodeIds ? { review: { ...s.review, error: describeError(e) } } : s,
      )
    }
  },

  setQuitApps: (on) => set((s) => (s.review ? { review: { ...s.review, quitApps: on } } : s)),

  closeReview: () => set({ review: null }),

  confirmReview: async () => {
    const r = get().review
    if (!r || !r.preview || r.moving) return
    const id = r.scanId
    const allowed = r.preview.items.filter((i) => i.blocked === null).map((i) => i.node_id)
    set({ review: { ...r, moving: true, error: null } })
    try {
      const result = r.action
        ? await r.action({ quitApps: r.quitApps })
        : await commands.trashItems(id, allowed, r.room)
      set({ review: null, banner: { kind: 'moved', result } })
      useSpaceStore.getState().select(null)
      useFindStore.getState().set({ selected: [] })
      useScanStore.getState().bumpData()
    } catch (e) {
      set({ review: { ...r, moving: false, error: describeError(e) } })
    }
  },

  undoBatch: async (batchId) => {
    try {
      const r = await commands.undoBatch(batchId, null, false, scanId())
      set({ banner: { kind: 'info', ...undoText(r) } })
      useScanStore.getState().bumpData()
    } catch (e) {
      set({ banner: { kind: 'info', text: describeError(e), tone: 'warn' } })
    }
  },

  undoLast: async () => {
    try {
      const r = await commands.undoLast(scanId())
      set({
        banner: r
          ? { kind: 'info', ...undoText(r) }
          : { kind: 'info', text: 'Nothing to undo.', tone: 'ok' },
      })
      if (r) useScanStore.getState().bumpData()
    } catch (e) {
      set({ banner: { kind: 'info', text: describeError(e), tone: 'warn' } })
    }
  },

  openEmpty: async () => {
    set({ empty: { prep: null, error: null, busy: false } })
    try {
      const prep = await commands.prepareEmptyTrash()
      set({ empty: { prep, error: null, busy: false } })
    } catch (e) {
      set({ empty: { prep: null, error: describeError(e), busy: false } })
    }
  },

  closeEmpty: () => set({ empty: null }),

  confirmEmpty: async () => {
    const e = get().empty
    if (!e?.prep) return
    set({ empty: { ...e, busy: true } })
    try {
      await commands.emptyTrash(e.prep.token, scanId())
      set({
        empty: null,
        banner: { kind: 'info', text: 'The Trash is empty. The space is now free.', tone: 'ok' },
      })
      useScanStore.getState().bumpData()
    } catch (err) {
      set({ empty: { ...e, busy: false, error: describeError(err).replace(/^\w+: /, '') } })
    }
  },

  dismissBanner: () => set({ banner: null }),
}))

function undoText(r: {
  restored: number
  renamed: number
  conflicts: number
  missing: number
  failed: unknown[]
}): { text: string; tone: 'ok' | 'warn' } {
  const parts = [`Restored ${r.restored} item${r.restored === 1 ? '' : 's'}`]
  if (r.renamed) parts.push(`${r.renamed} under a "(restored)" name`)
  if (r.conflicts) parts.push(`${r.conflicts} kept in the Trash because the name is taken`)
  if (r.missing) parts.push(`${r.missing} already gone from the Trash`)
  if (r.failed.length) parts.push(`${r.failed.length} could not be restored`)
  return {
    text: `${parts.join(', ')}.`,
    tone: r.conflicts || r.failed.length || r.missing ? 'warn' : 'ok',
  }
}
