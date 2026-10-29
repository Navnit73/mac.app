import { useEffect } from 'react'
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom'
import EmptyTrashDialog from './components/EmptyTrashDialog'
import FdaBanner from './components/FdaBanner'
import Inspector from './components/Inspector'
import ResultBanner from './components/ResultBanner'
import ReviewSheet from './components/ReviewSheet'
import RoomPlaceholder from './components/RoomPlaceholder'
import Sidebar from './components/Sidebar'
import StatusFooter from './components/StatusFooter'
import Toolbar from './components/Toolbar'
import Activity from './rooms/Activity'
import Dev from './rooms/Dev'
import Duplicates from './rooms/Duplicates'
import Find from './rooms/Find'
import Overview from './rooms/Overview'
import Space from './rooms/Space'
import { ROOMS } from './rooms'
import { initDupesListeners } from './state/dupesStore'
import { useFindStore } from './state/findStore'
import { initScanListeners } from './state/scanStore'
import { useSpaceStore } from './state/spaceStore'
import { useTrashStore } from './state/trashStore'

const BUILT: Record<string, React.ReactElement> = {
  '/overview': <Overview />,
  '/space': <Space />,
  '/find': <Find />,
  '/activity': <Activity />,
  '/duplicates': <Duplicates />,
}

export default function App() {
  const navigate = useNavigate()
  useEffect(() => initScanListeners(), [])
  useEffect(() => initDupesListeners(), [])

  // ⌘F: Find. ⌘⌫: review the selection for the Trash. ⌘Z: undo the last batch.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!e.metaKey) return
      const t = e.target as HTMLElement | null
      const typing =
        !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)
      const key = e.key.toLowerCase()
      if (key === 'f') {
        e.preventDefault()
        navigate('/find')
        useFindStore.getState().requestFocus()
      } else if (key === 'backspace' && !typing) {
        const onFind = window.location.hash.startsWith('#/find')
        const findSel = useFindStore.getState().selected
        const spaceSel = useSpaceStore.getState().selectedId
        const ids = onFind && findSel.length > 0 ? findSel : spaceSel !== null ? [spaceSel] : []
        if (ids.length > 0) {
          e.preventDefault()
          void useTrashStore.getState().openReview(ids, onFind ? 'Find' : 'Space')
        }
      } else if (key === 'z' && !e.shiftKey && !typing) {
        e.preventDefault()
        void useTrashStore.getState().undoLast()
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
                    element={BUILT[room.path] ?? <RoomPlaceholder room={room} />}
                  />
                ))}
                <Route path="/dev" element={<Dev />} />
              </Routes>
            </main>
            <Inspector />
          </div>
        </div>
      </div>
      <StatusFooter />
      <ReviewSheet />
      <EmptyTrashDialog />
      <div className="pointer-events-none fixed inset-x-0 bottom-10 z-30 flex justify-center">
        <ResultBanner />
      </div>
    </div>
  )
}
