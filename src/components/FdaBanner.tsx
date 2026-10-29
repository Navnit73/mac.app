import { useEffect, useState } from 'react'
import { useFdaStore } from '../state/fdaStore'
import { useScanStore } from '../state/scanStore'
import Icon from './Icon'

/**
 * Explains why some folders are skipped and how to fix it. Shown only when access is denied.
 *
 * macOS never lists an app under Full Disk Access by itself: it has to be added with the +
 * button. While developing, DuoBolt is a bare binary, so macOS checks the app that started it
 * (an editor or Terminal); Rust works out which one and the steps name it.
 */
export default function FdaBanner() {
  const status = useFdaStore((s) => s.status)
  const target = useFdaStore((s) => s.target)
  const dismissed = useFdaStore((s) => s.dismissed)
  const check = useFdaStore((s) => s.check)
  const openSettings = useFdaStore((s) => s.openSettings)
  const revealTarget = useFdaStore((s) => s.revealTarget)
  const dismiss = useFdaStore((s) => s.dismiss)
  const phase = useScanStore((s) => s.phase)
  const [open, setOpen] = useState(false)

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

  const app = target?.name ?? 'DuoBolt'
  const steps = [
    <>Open System Settings &gt; Privacy &amp; Security &gt; Full Disk Access.</>,
    <>
      Click <b>+</b> under the list and choose <b>{app}</b>
      {target ? ' (or drag it in from Finder)' : ''}. If it is already listed, turn its switch on.
    </>,
    <>
      Quit <b>{app}</b> completely and open it again. macOS applies the change only to apps opened
      after it.
    </>,
    <>Scan again.</>,
  ]

  return (
    <div role="status" className="border-b border-line bg-warn-soft">
      {/* One slim line by default; the explanation and steps open on request. */}
      <div className="flex items-center gap-3 px-5 py-2">
        <Icon name="shield" size={15} className="shrink-0 text-warn" />
        <p className="min-w-0 flex-1 truncate">
          <span className="font-semibold">Full Disk Access is off.</span>{' '}
          <span className="text-muted">
            Mail, Safari, the Trash and parts of Library are skipped, so totals can be low.
            {target && !target.is_self && <> Turn it on for {app}.</>}
          </span>
        </p>
        <button
          type="button"
          className="btn h-7 text-xs"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
        >
          How to fix
          <Icon
            name="chevron"
            size={11}
            strokeWidth={2.4}
            className={`transition-transform ${open ? '-rotate-90' : 'rotate-90'}`}
          />
        </button>
        <button
          type="button"
          className="btn btn-primary h-7 text-xs"
          onClick={() => void openSettings()}
        >
          Open System Settings
        </button>
        <button
          type="button"
          className="btn btn-ghost h-7 px-1.5"
          aria-label="Dismiss"
          title="Hide until next launch"
          onClick={dismiss}
        >
          <Icon name="close" size={13} />
        </button>
      </div>
      {open && (
        <div className="px-5 pb-4 pl-12">
          {target && !target.is_self && (
            <p className="text-muted">
              DuoBolt is running inside <b className="text-fg">{app}</b>, so macOS checks {app}
              &rsquo;s access, not DuoBolt&rsquo;s. Once DuoBolt is installed as an app, you turn on
              DuoBolt itself instead.
            </p>
          )}
          <ol className="mt-3 flex flex-col gap-1.5">
            {steps.map((step, i) => (
              <li key={i} className="flex items-start gap-2.5">
                <span className="num mt-px flex h-[18px] w-[18px] shrink-0 items-center justify-center rounded-full bg-fg text-[10px] font-semibold text-panel">
                  {i + 1}
                </span>
                <span>{step}</span>
              </li>
            ))}
          </ol>
          <div className="mt-3 flex flex-wrap gap-2">
            {target && (
              <button
                type="button"
                className="btn h-7 text-xs"
                onClick={() => void revealTarget()}
                title={target.path}
              >
                <Icon name="reveal" size={12} /> Show {app} in Finder
              </button>
            )}
            <button type="button" className="btn h-7 text-xs" onClick={() => void check()}>
              Check again
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
