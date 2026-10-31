import { formatBytes, formatCount } from '../lib/format'
import { useTrashStore } from '../state/trashStore'
import Icon from './Icon'
import Modal from './Modal'

/** The only permanent deletion in MyMacStorage, behind its own confirmation. */
export default function EmptyTrashDialog() {
  const empty = useTrashStore((s) => s.empty)
  const close = useTrashStore((s) => s.closeEmpty)
  const confirm = useTrashStore((s) => s.confirmEmpty)
  const p = empty?.prep

  return (
    <Modal
      open={empty !== null}
      onClose={() => !empty?.busy && close()}
      labelledBy="empty-title"
      width={480}
    >
      <div className="flex flex-col gap-4 p-6">
        <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-bad-soft text-bad">
          <Icon name="trash" size={22} />
        </div>
        <div>
          <h2 id="empty-title" className="text-lg font-semibold">
            Empty the Trash?
          </h2>
          <p className="mt-1 text-muted">
            Everything in the Trash is deleted for good, including items you put there outside
            MyMacStorage. This cannot be undone.
          </p>
        </div>
        {p && (
          <dl className="grid grid-cols-2 gap-3 rounded-xl bg-track p-3">
            <div>
              <dt className="eyebrow">Whole Trash</dt>
              <dd className="num text-lg font-semibold">
                {p.trash_bytes === null ? 'Unknown' : formatBytes(p.trash_bytes)}
              </dd>
            </div>
            <div>
              <dt className="eyebrow">From MyMacStorage</dt>
              <dd className="num text-lg font-semibold">
                {formatBytes(p.mymacstorage_bytes)}
                <span className="ml-1 text-xs font-normal text-muted">
                  {formatCount(p.mymacstorage_items)} items
                </span>
              </dd>
            </div>
          </dl>
        )}
        <p className="text-xs text-muted">
          MyMacStorage asks Finder to empty the Trash. The first time, macOS asks whether
          MyMacStorage may control Finder.
        </p>
        {empty?.error && (
          <p role="alert" className="rounded-lg bg-bad-soft px-3 py-2 text-bad">
            {empty.error}
          </p>
        )}
        <div className="flex justify-end gap-2">
          <button
            type="button"
            className="btn"
            onClick={close}
            disabled={empty?.busy}
            data-autofocus
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn-danger"
            disabled={!p || empty?.busy}
            onClick={() => void confirm()}
          >
            {empty?.busy ? 'Emptying…' : 'Empty Trash'}
          </button>
        </div>
      </div>
    </Modal>
  )
}
