import { useEffect } from 'react'
import { useFdaStore } from '../state/fdaStore'
import { useScanStore } from '../state/scanStore'
import Icon from './Icon'

/** Explains why some folders are skipped and how to fix it. Shown only when access is denied. */
export default function FdaBanner() {
  const status = useFdaStore((s) => s.status)
  const dismissed = useFdaStore((s) => s.dismissed)
  const check = useFdaStore((s) => s.check)
  const openSettings = useFdaStore((s) => s.openSettings)
  const dismiss = useFdaStore((s) => s.dismiss)
  const phase = useScanStore((s) => s.phase)

  useEffect(() => {
    void check()
    // Re-check when the user comes back from System Settings.
    const onFocus = () => void check()
    window.addEventListener('focus', onFocus)
    return () => window.removeEventListener('focus', onFocus)
  }, [check])

  // Re-check after every scan so the banner clears once access is granted.
  useEffect(() => {
    if (phase === 'done') void check()
  }, [phase, check])

  if (status !== 'Denied' || dismissed) return null

  return (
    <div
      role="status"
      className="flex items-start gap-3 border-b border-line bg-warn-soft px-5 py-3"
    >
      <span className="mt-0.5 text-warn">
        <Icon name="shield" size={18} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="font-semibold">Some folders are off limits without Full Disk Access</p>
        <p className="mt-0.5 text-muted">
          Mail, Messages, Safari, the Trash and parts of Library are skipped, so totals can be lower
          than real usage. Turn DuoBolt on under Privacy &amp; Security, then scan again. While
          developing, grant it to the app that launches DuoBolt (Terminal or your editor).
        </p>
        <div className="mt-2 flex gap-2">
          <button type="button" className="btn btn-primary" onClick={() => void openSettings()}>
            Open System Settings
          </button>
          <button type="button" className="btn" onClick={() => void check()}>
            Check again
          </button>
        </div>
      </div>
      <button type="button" className="btn btn-ghost px-2" aria-label="Dismiss" onClick={dismiss}>
        <Icon name="close" size={14} />
      </button>
    </div>
  )
}
