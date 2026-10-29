import { formatBytes, formatCount } from '../lib/format'
import { middleTruncate, tildify } from '../lib/text'
import { useTrashStore } from '../state/trashStore'
import Icon from './Icon'
import Modal from './Modal'
import EntryIcon from './space/EntryIcon'

const SHOWN = 400
const RISK = {
  Safe: { cls: 'badge-safe', label: 'Safe' },
  Review: { cls: 'badge-review', label: 'Review' },
  Careful: { cls: 'badge-careful', label: 'Careful' },
} as const

function listNames(names: string[]): string {
  if (names.length <= 2) return names.join(' and ')
  return `${names.slice(0, 2).join(', ')} and ${names.length - 2} more`
}

/** The one place every removal is confirmed. Nothing moves until the user presses the button. */
export default function ReviewSheet() {
  const review = useTrashStore((s) => s.review)
  const close = useTrashStore((s) => s.closeReview)
  const confirm = useTrashStore((s) => s.confirmReview)
  const setQuitApps = useTrashStore((s) => s.setQuitApps)
  const p = review?.preview

  return (
    <Modal
      open={review !== null}
      onClose={() => !review?.moving && close()}
      labelledBy="review-title"
    >
      <div className="flex items-start justify-between gap-4 border-b border-line px-5 py-4">
        <div>
          <h2 id="review-title" className="text-base font-semibold">
            Review before moving to the Trash
          </h2>
          <p className="mt-0.5 text-muted">
            Items go to the Trash, not away for good. You can undo this until the Trash is emptied.
          </p>
          {review?.note && (
            <p className="mt-2 flex items-start gap-1.5 rounded-lg bg-accent-soft px-3 py-2">
              <Icon name="shield" size={13} className="mt-0.5 shrink-0 text-accent-ink" />
              {review.note}
            </p>
          )}
        </div>
        <button
          type="button"
          className="btn btn-ghost px-2"
          aria-label="Close"
          onClick={close}
          disabled={review?.moving}
        >
          <Icon name="close" />
        </button>
      </div>

      {!p && !review?.error && (
        <p className="px-5 py-10 text-center text-muted">Checking each item…</p>
      )}

      {p && (
        <ul className="max-h-[48vh] overflow-auto px-5 py-2" aria-label="Items to move">
          {p.items.slice(0, SHOWN).map((it) => (
            <li
              key={it.node_id}
              className={`flex items-center gap-3 border-b border-line py-2 last:border-0 ${it.blocked ? 'opacity-60' : ''}`}
            >
              <EntryIcon kind={it.file_kind} isDir={it.is_dir} isPackage={false} />
              <div className="min-w-0 flex-1">
                <p
                  className={`truncate font-medium ${it.blocked ? 'line-through' : ''}`}
                  title={it.name}
                >
                  {it.name}
                </p>
                <p className="truncate text-xs text-muted" title={it.path}>
                  {it.blocked ? (
                    <span className="flex items-center gap-1 text-warn">
                      <Icon name="lock" size={11} /> {it.blocked}
                    </span>
                  ) : (
                    middleTruncate(tildify(it.path), 90)
                  )}
                </p>
              </div>
              {!it.blocked && (
                <span className={`badge ${RISK[it.risk].cls}`}>{RISK[it.risk].label}</span>
              )}
              <span className="num w-20 text-right font-semibold">{formatBytes(it.size)}</span>
            </li>
          ))}
          {p.items.length > SHOWN && (
            <li className="py-2 text-center text-muted">
              and {formatCount(p.items.length - SHOWN)} more
            </li>
          )}
        </ul>
      )}

      {review?.error && (
        <p role="alert" className="mx-5 my-3 rounded-lg bg-bad-soft px-3 py-2 text-bad">
          {review.error}
        </p>
      )}

      {review?.runningApps && review.runningApps.length > 0 && (
        <label className="mx-5 my-3 flex items-start gap-2 rounded-lg bg-warn-soft px-3 py-2">
          <input
            type="checkbox"
            className="mt-1"
            checked={review.quitApps}
            disabled={review.moving}
            onChange={(e) => setQuitApps(e.target.checked)}
          />
          <span>
            <span className="font-medium">Quit {listNames(review.runningApps)} first</span>
            <span className="block text-xs text-muted">
              They are open and use some of these items. DuoBolt asks them to quit the normal way
              (they may ask to save). Without this, their items are left alone.
            </span>
          </span>
        </label>
      )}

      <div className="flex items-center gap-3 border-t border-line px-5 py-4">
        <div className="text-muted">
          {p && p.blocked_count > 0 && (
            <span className="flex items-center gap-1.5">
              <Icon name="lock" size={12} /> {formatCount(p.blocked_count)} item
              {p.blocked_count === 1 ? '' : 's'} will be left alone
            </span>
          )}
        </div>
        <div className="ml-auto flex gap-2">
          <button
            type="button"
            className="btn"
            onClick={close}
            disabled={review?.moving}
            data-autofocus
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-primary"
            disabled={!p || p.allowed_count === 0 || review?.moving}
            onClick={() => void confirm()}
          >
            <Icon name="trash" size={13} />
            {review?.moving
              ? 'Moving…'
              : p
                ? `Move ${formatCount(p.allowed_count)} item${p.allowed_count === 1 ? '' : 's'} (${formatBytes(p.allowed_bytes)}) to Trash`
                : 'Move to Trash'}
          </button>
        </div>
      </div>
    </Modal>
  )
}
