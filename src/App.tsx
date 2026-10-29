import { useEffect } from 'react'
import { Navigate, Route, Routes } from 'react-router-dom'
import RoomPlaceholder from './components/RoomPlaceholder'
import Sidebar from './components/Sidebar'
import StatusFooter from './components/StatusFooter'
import Toolbar from './components/Toolbar'
import Dev from './rooms/Dev'
import { ROOMS } from './rooms'
import { initScanListeners } from './state/scanStore'

export default function App() {
  useEffect(() => initScanListeners(), [])

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <div className="flex min-w-0 flex-1 flex-col">
          <Toolbar />
          <main className="min-h-0 flex-1 overflow-auto">
            <Routes>
              <Route path="/" element={<Navigate to="/overview" replace />} />
              {ROOMS.map((room) => (
                <Route key={room.path} path={room.path} element={<RoomPlaceholder room={room} />} />
              ))}
              <Route path="/dev" element={<Dev />} />
            </Routes>
          </main>
        </div>
      </div>
      <StatusFooter />
    </div>
  )
}
