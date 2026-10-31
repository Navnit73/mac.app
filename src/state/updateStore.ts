import { create } from 'zustand'
import { commands, describeError, events } from '../lib/ipc'
import type { AppUpdate } from '../types/bindings/AppUpdate'

type Dialog =
  | { kind: 'update'; update: AppUpdate }
  | { kind: 'checking' }
  | { kind: 'upToDate' }
  | { kind: 'error'; message: string }

interface UpdateStore {
  dialog: Dialog | null
  busy: boolean
  /** Show an update offered by the heartbeat (skipped builds were filtered in Rust). */
  offer: (update: AppUpdate) => void
  /** The "Check for Updates…" menu item. */
  checkNow: () => Promise<void>
  download: () => Promise<void>
  later: () => Promise<void>
  skip: () => Promise<void>
  close: () => void
}

export const useUpdateStore = create<UpdateStore>((set, get) => ({
  dialog: null,
  busy: false,
  offer: (update) => {
    const d = get().dialog
    // Never replace a prompt the user is looking at with an older one.
    if (d?.kind === 'update' && d.update.build >= update.build) return
    set({ dialog: { kind: 'update', update } })
    void commands
      .trackEvent('update.prompt_shown', { build: update.build, required: update.required })
      .catch(() => undefined)
  },
  checkNow: async () => {
    if (get().dialog?.kind === 'update') return
    set({ dialog: { kind: 'checking' } })
    try {
      const update = await commands.checkForUpdates()
      if (update) get().offer(update)
      else set({ dialog: { kind: 'upToDate' } })
    } catch (e) {
      set({ dialog: { kind: 'error', message: describeError(e) } })
    }
  },
  download: async () => {
    const d = get().dialog
    if (d?.kind !== 'update') return
    set({ busy: true })
    try {
      await commands.openUpdateDownload(d.update.build)
      if (d.update.required) {
        await commands.quitApp()
        return
      }
      set({ dialog: null, busy: false })
    } catch (e) {
      set({ dialog: { kind: 'error', message: describeError(e) }, busy: false })
    }
  },
  later: async () => {
    const d = get().dialog
    // A required update offers Quit instead of Later.
    if (d?.kind === 'update' && d.update.required) {
      await commands.quitApp().catch(() => undefined)
      return
    }
    set({ dialog: null })
  },
  skip: async () => {
    const d = get().dialog
    if (d?.kind !== 'update' || d.update.critical || d.update.required) return
    await commands.skipUpdate(d.update.build).catch(() => undefined)
    set({ dialog: null })
  },
  close: () => {
    const d = get().dialog
    // A required update cannot be dismissed.
    if (d?.kind === 'update' && d.update.required) return
    set({ dialog: null })
  },
}))

/** Listen for offers and the menu item; also pick up an offer made before the UI loaded. */
export function initUpdateListeners(): () => void {
  const store = useUpdateStore.getState()
  const unlisten = [
    events.onUpdateAvailable((u) => useUpdateStore.getState().offer(u)),
    events.onCheckUpdates(() => void useUpdateStore.getState().checkNow()),
  ]
  void commands
    .getOfferedUpdate()
    .then((u) => u && store.offer(u))
    .catch(() => undefined)
  return () => {
    for (const p of unlisten) void p.then((f) => f())
  }
}
