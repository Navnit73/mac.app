import { useEffect } from 'react'
import { Navigate, Route, Routes, useLocation, useNavigate } from 'react-router-dom'
import EmptyTrashDialog from './components/EmptyTrashDialog'
import FdaBanner from './components/FdaBanner'
import Inspector from './components/Inspector'
import ResultBanner from './components/ResultBanner'
import ReviewSheet from './components/ReviewSheet'
import Sidebar from './components/Sidebar'
import StatusFooter from './components/StatusFooter'
import Toolbar from './components/Toolbar'
import UpdateDialog from './components/UpdateDialog'
import Activity from './rooms/Activity'
import Applications from './rooms/Applications'
import Cleanup from './rooms/Cleanup'
import Dev from './rooms/Dev'
import Duplicates from './rooms/Duplicates'
import Find from './rooms/Find'
import Overview from './rooms/Overview'
import Space from './rooms/Space'
import { ROOMS } from './rooms'
import { initAppsListeners } from './state/appsStore'
import { initDupesListeners } from './state/dupesStore'
import { useFindStore } from './state/findStore'
import { initScanListeners } from './state/scanStore'
import { initUpdateListeners } from './state/updateStore'
import { useSpaceStore } from './state/spaceStore'
import { useTrashStore } from './state/trashStore'
import { commands } from './lib/ipc'

const ROOM_VIEWS: Record<string, React.ReactElement> = {
  '/overview': <Overview />,
  '/space': <Space />,
  '/find': <Find />,
  '/activity': <Activity />,
  '/duplicates': <Duplicates />,
  '/cleanup': <Cleanup />,
  '/applications': <Applications />,
}

export default function App() {
  const navigate = useNavigate()
  useEffect(() => initScanListeners(), [])
  useEffect(() => initDupesListeners(), [])
  useEffect(() => initAppsListeners(), [])
  useEffect(() => initUpdateListeners(), [])

  // Anonymous usage stats: which rooms get opened (e.g. `duplicates.opened`). Room name only.
  const { pathname } = useLocation()
  useEffect(() => {
    const room = pathname.split('/')[1]
    if (room && ROOMS.some((r) => r.path === `/${room}`)) {
      void commands.trackEvent(`${room}.opened`).catch(() => undefined)
    }
  }, [pathname])

  // ⌘F: Find. ⌘⌫: review the selection for the Trash. ⌘Z: undo the last batch.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.metaKey) return
      const t = e.target as HTMLElement | null
      const typing =
        !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)
      const key = e.key.toLowerCase()
      const trash = useTrashStore.getState()
      // A Review or Empty Trash dialog is open: its own buttons decide, not background shortcuts.
      const dialogOpen = trash.review !== null || trash.empty !== null
      if (key === 'f') {
        e.preventDefault()
        navigate('/find')
        useFindStore.getState().requestFocus()
      } else if (key === 'backspace' && !typing && !dialogOpen) {
        // Only rooms whose selection the Inspector shows; elsewhere ⌘⌫ would act on a
        // selection the user cannot see.
        const onFind = window.location.hash.startsWith('#/find')
        const onSpace = window.location.hash.startsWith('#/space')
        if (!onFind && !onSpace) return
        const findSel = useFindStore.getState().selected
        const spaceSel = useSpaceStore.getState().selectedId
        const ids = onFind ? findSel : spaceSel !== null ? [spaceSel] : []
        if (ids.length > 0) {
          e.preventDefault()
          void trash.openReview(ids, onFind ? 'Find' : 'Space')
        }
      } else if (key === 'z' && !e.shiftKey && !typing && !dialogOpen) {
        e.preventDefault()
        void trash.undoLast()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [navigate])

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <div className="flex min-w-0 flex-1 flex-col">
          <Toolbar />
          <FdaBanner />
          <div className="flex min-h-0 flex-1">
            <main className="min-w-0 flex-1 overflow-auto">
              <Routes>
                <Route path="/" element={<Navigate to="/overview" replace />} />
                {ROOMS.map((room) => (
                  <Route
                    key={room.path}
                    path={room.path}
                    element={ROOM_VIEWS[room.path] ?? <Navigate to="/overview" replace />}
                  />
                ))}
                <Route path="/dev" element={<Dev />} />
              </Routes>
            </main>
            <Inspector />
          </div>
          <StatusFooter />
        </div>
      </div>
      <ReviewSheet />
      <EmptyTrashDialog />
      <UpdateDialog />
      <div className="pointer-events-none fixed inset-x-0 bottom-10 z-30 flex justify-center">
        <ResultBanner />
      </div>
    </div>
  )
}
