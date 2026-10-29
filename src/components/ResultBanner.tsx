import { useState } from 'react'
import { formatBytes, formatCount } from '../lib/format'
import { tildify } from '../lib/text'
import { useTrashStore } from '../state/trashStore'
import Icon from './Icon'

/** Floating result of the last Trash action, with Undo and Empty Trash. */
export default function ResultBanner() {
  const banner = useTrashStore((s) => s.banner)
  const dismiss = useTrashStore((s) => s.dismissBanner)
  const undo = useTrashStore((s) => s.undoBatch)
  const openEmpty = useTrashStore((s) => s.openEmpty)
  const [details, setDetails] = useState(false)
  if (!banner) return null

  if (banner.kind === 'info') {
    return (
      <div
        role="status"
        className="pointer-events-auto card flex max-w-xl items-center gap-3 px-4 py-3"
      >
        <span className={banner.tone === 'ok' ? 'text-ok' : 'text-warn'}>
          <Icon name={banner.tone === 'ok' ? 'check' : 'warning'} size={16} />
        </span>
        <span>{banner.text}</span>
        <button
          type="button"
          className="btn btn-ghost ml-auto px-1.5"
          aria-label="Dismiss"
          onClick={dismiss}
        >
          <Icon name="close" size={13} />
        </button>
      </div>
    )
  }

  const r = banner.result
  const nothing = r.moved_count === 0
  return (
    <div role="status" className="pointer-events-auto card w-[560px] max-w-[90vw] px-4 py-3">
      <div className="flex items-start gap-3">
        <span className={`mt-0.5 ${nothing ? 'text-warn' : 'text-ok'}`}>
          <Icon name={nothing ? 'warning' : 'check'} size={16} />
        </span>
        <div className="min-w-0 flex-1">
          <p className="font-semibold">
            {nothing
              ? 'Nothing was moved.'
              : `Moved ${formatCount(r.moved_count)} item${r.moved_count === 1 ? '' : 's'} (${formatBytes(r.moved_bytes)}) to the Trash.`}
          </p>
          {!nothing && <p className="text-muted">Empty the Trash to reclaim the space.</p>}
          {r.skipped.length > 0 && (
            <button
              type="button"
              className="mt-1 text-xs text-warn hover:underline"
              onClick={() => setDetails(!details)}
            >
              {formatCount(r.skipped.length)} left alone · {details ? 'hide' : 'show why'}
            </button>
          )}
        </div>
        <button
          type="button"
          className="btn btn-ghost px-1.5"
          aria-label="Dismiss"
          onClick={dismiss}
        >
          <Icon name="close" size={13} />
        </button>
      </div>
      {details && (
        <ul className="mt-2 max-h-40 overflow-auto rounded-lg bg-track px-3 py-2 text-xs">
          {r.skipped.map((s) => (
            <li key={s.path} className="py-0.5">
              <span className="font-mono">{tildify(s.path)}</span>
              <span className="text-muted"> · {s.reason}</span>
            </li>
          ))}
        </ul>
      )}
      {!nothing && r.batch_id !== null && (
        <div className="mt-3 flex gap-2 pl-7">
          <button
            type="button"
            className="btn"
            onClick={() => r.batch_id !== null && void undo(r.batch_id)}
          >
            Undo
          </button>
          <button type="button" className="btn" onClick={() => void openEmpty()}>
            Empty Trash…
          </button>
        </div>
      )}
    </div>
  )
}
