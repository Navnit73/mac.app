import { formatBytes } from '../lib/format'
import { useUpdateStore } from '../state/updateStore'
import Modal from './Modal'

/** Update offers from mymacstorage.com and the result of "Check for Updates…". */
export default function UpdateDialog() {
  const dialog = useUpdateStore((s) => s.dialog)
  const busy = useUpdateStore((s) => s.busy)
  const { download, later, skip, close } = useUpdateStore.getState()

  const update = dialog?.kind === 'update' ? dialog.update : null
  const required = update?.required ?? false
  const canSkip = update !== null && !update.critical && !update.required

  return (
    <Modal open={dialog !== null} onClose={close} labelledBy="update-title" width={460}>
      <div className="flex flex-col gap-4 p-6">
        {update && (
          <>
            <div>
              <h2 id="update-title" className="text-lg font-semibold">
                {required ? 'Update required' : `${update.title} is available`}
              </h2>
              <p className="mt-1 text-muted">
                {required
                  ? 'This version of MyMacStorage is no longer supported. Download the new version to keep using it.'
                  : update.critical
                    ? 'This update contains an important fix.'
                    : `Version ${update.version} is ready to download.`}
              </p>
            </div>
            {update.notes && (
              <div className="max-h-48 overflow-auto rounded-xl bg-track p-3 text-sm whitespace-pre-wrap">
                {update.notes}
              </div>
            )}
            <p className="text-xs text-muted">
              The download opens in your browser
              {update.size ? ` (${formatBytes(update.size)})` : ''}. Open the disk image and drag
              MyMacStorage into Applications to replace this copy.
            </p>
            <div className="flex justify-end gap-2">
              {canSkip && (
                <button type="button" className="btn btn-ghost mr-auto" onClick={() => void skip()}>
                  Skip This Version
                </button>
              )}
              <button
                type="button"
                className="btn"
                disabled={busy}
                onClick={() => void later()}
                data-autofocus
              >
                {required ? 'Quit' : 'Later'}
              </button>
              <button
                type="button"
                className="btn btn-primary"
                disabled={busy}
                onClick={() => void download()}
              >
                Download
              </button>
            </div>
          </>
        )}
        {dialog?.kind === 'checking' && (
          <h2 id="update-title" className="text-lg font-semibold">
            Checking for updates…
          </h2>
        )}
        {(dialog?.kind === 'upToDate' || dialog?.kind === 'error') && (
          <>
            <div>
              <h2 id="update-title" className="text-lg font-semibold">
                {dialog.kind === 'upToDate' ? "You're up to date" : 'Could not check for updates'}
              </h2>
              <p className="mt-1 text-muted">
                {dialog.kind === 'upToDate'
                  ? 'You have the latest version of MyMacStorage.'
                  : dialog.message}
              </p>
            </div>
            <div className="flex justify-end">
              <button type="button" className="btn btn-primary" onClick={close} data-autofocus>
                OK
              </button>
            </div>
          </>
        )}
      </div>
    </Modal>
  )
}
