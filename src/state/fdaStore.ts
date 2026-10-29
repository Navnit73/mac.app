import { create } from 'zustand'
import { commands } from '../lib/ipc'
import type { FdaStatus } from '../types/bindings/FdaStatus'
import type { FdaTarget } from '../types/bindings/FdaTarget'

interface FdaStore {
  status: FdaStatus | 'checking'
  /** The app to turn on in System Settings; null until known or when it cannot be told. */
  target: FdaTarget | null
  dismissed: boolean
  check: () => Promise<void>
  openSettings: () => Promise<void>
  revealTarget: () => Promise<void>
  dismiss: () => void
}

export const useFdaStore = create<FdaStore>((set) => ({
  status: 'checking',
  target: null,
  dismissed: false,
  check: async () => {
    try {
      set({ status: await commands.getFdaStatus() })
    } catch {
      set({ status: 'Unknown' })
    }
    try {
      set({ target: await commands.getFdaTarget() })
    } catch {
      set({ target: null })
    }
  },
  openSettings: async () => {
    try {
      await commands.openFdaSettings()
    } catch {
      // Opening Settings is a convenience; the banner still explains the steps.
    }
  },
  revealTarget: async () => {
    try {
      await commands.revealFdaTarget()
    } catch {
      // Revealing is a convenience; the steps still name the app.
    }
  },
  dismiss: () => set({ dismissed: true }),
}))
