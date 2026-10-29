import { create } from 'zustand'
import { commands } from '../lib/ipc'
import type { FdaStatus } from '../types/bindings/FdaStatus'

interface FdaStore {
  status: FdaStatus | 'checking'
  dismissed: boolean
  check: () => Promise<void>
  openSettings: () => Promise<void>
  dismiss: () => void
}

export const useFdaStore = create<FdaStore>((set) => ({
  status: 'checking',
  dismissed: false,
  check: async () => {
    try {
      set({ status: await commands.getFdaStatus() })
    } catch {
      set({ status: 'Unknown' })
    }
  },
  openSettings: async () => {
    try {
      await commands.openFdaSettings()
    } catch {
      // Opening Settings is a convenience; the banner still explains the steps.
    }
  },
  dismiss: () => set({ dismissed: true }),
}))
