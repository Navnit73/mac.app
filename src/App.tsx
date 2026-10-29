import { useEffect } from 'react'
import { Navigate, Route, Routes, useNavigate } from 'react-router-dom'
import FdaBanner from './components/FdaBanner'
import Inspector from './components/Inspector'
import RoomPlaceholder from './components/RoomPlaceholder'
import Sidebar from './components/Sidebar'
import StatusFooter from './components/StatusFooter'
import Toolbar from './components/Toolbar'
import Dev from './rooms/Dev'
import Find from './rooms/Find'
import Overview from './rooms/Overview'
import Space from './rooms/Space'
import { ROOMS } from './rooms'
import { useFindStore } from './state/findStore'
import { initScanListeners } from './state/scanStore'

const BUILT: Record<string, React.ReactElement> = {
  '/overview': <Overview />,
  '/space': <Space />,
  '/find': <Find />,
}

export default function App() {
  const navigate = useNavigate()
  useEffect(() => initScanListeners(), [])

  // ⌘F jumps to Find from anywhere.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === 'f') {
        e.preventDefault()
        navigate('/find')
        useFindStore.getState().requestFocus()
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
    </div>
  )
}
