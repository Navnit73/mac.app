import { useEffect, useState } from 'react'
import { commands } from '../../lib/ipc'
import { useSpaceStore } from '../../state/spaceStore'
import type { Crumb } from '../../types/bindings/Crumb'
import Icon from '../Icon'

/** Back / forward / up buttons and the clickable path of the current folder. */
export default function Breadcrumb({ scanId }: { scanId: number }) {
  const currentId = useSpaceStore((s) => s.currentId)
  const back = useSpaceStore((s) => s.back)
  const forward = useSpaceStore((s) => s.forward)
  const goBack = useSpaceStore((s) => s.goBack)
  const goForward = useSpaceStore((s) => s.goForward)
  const navigate = useSpaceStore((s) => s.navigate)
  const [crumbs, setCrumbs] = useState<Crumb[]>([])

  useEffect(() => {
    if (currentId === null) return
    let live = true
    commands
      .getBreadcrumb(scanId, currentId)
      .then((c) => live && setCrumbs(c))
      .catch(() => live && setCrumbs([]))
    return () => {
      live = false
    }
  }, [scanId, currentId])

  const parent = crumbs.length > 1 ? crumbs[crumbs.length - 2] : undefined

  return (
    <nav aria-label="Folder path" className="flex min-w-0 items-center gap-1">
      <button
        type="button"
        className="btn btn-ghost px-1.5"
        disabled={back.length === 0}
        onClick={goBack}
        aria-label="Back"
        title="Back"
      >
        <Icon name="back" />
      </button>
      <button
        type="button"
        className="btn btn-ghost px-1.5"
        disabled={forward.length === 0}
        onClick={goForward}
        aria-label="Forward"
        title="Forward"
      >
        <Icon name="forward" />
      </button>
      <button
        type="button"
        className="btn btn-ghost px-1.5"
        disabled={!parent}
        onClick={() => parent && navigate(parent.id)}
        aria-label="Up one folder"
        title="Up one folder"
      >
        <Icon name="chevron" className="-rotate-90" />
      </button>
      <ol className="ml-1 flex min-w-0 items-center overflow-hidden">
        {crumbs.map((c, i) => {
          const last = i === crumbs.length - 1
          return (
            <li key={c.id} className="flex min-w-0 items-center">
              {i > 0 && <Icon name="chevron" size={12} className="mx-0.5 shrink-0 text-faint" />}
              <button
                type="button"
                aria-current={last ? 'page' : undefined}
                onClick={() => navigate(c.id)}
                className={`truncate rounded px-1.5 py-0.5 hover:bg-hover ${last ? 'font-semibold' : 'text-muted'}`}
                style={{ maxWidth: last ? 260 : 140 }}
                title={c.name}
              >
                {c.name}
              </button>
            </li>
          )
        })}
      </ol>
    </nav>
  )
}
