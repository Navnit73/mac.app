import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { HashRouter } from 'react-router-dom'
import App from './App'
import './index.css'

const container = document.getElementById('root')
if (!container) throw new Error('root element missing')
const mountPoint: HTMLElement = container

function render(sample = false) {
  createRoot(mountPoint).render(
    <StrictMode>
      <HashRouter>
        <App />
      </HashRouter>
      {sample && (
        // Never let made-up numbers pass for the real disk.
        <div
          role="note"
          className="pointer-events-none fixed top-3 left-1/2 z-50 -translate-x-1/2 rounded-full bg-warn px-3 py-1 text-xs font-semibold text-black shadow-lg"
        >
          Sample data — browser preview. Run npm run tauri dev to see your Mac.
        </div>
      )}
    </StrictMode>,
  )
}

async function boot() {
  // Browser-only preview (no Tauri): install a mocked bridge with sample data.
  if (import.meta.env.DEV && !('__TAURI_INTERNALS__' in window)) {
    const mock = await import('./lib/mockBridge')
    mock.installMockBridge()
    const [{ useScanStore }, { useSpaceStore }] = await Promise.all([
      import('./state/scanStore'),
      import('./state/spaceStore'),
    ])
    render(true)
    // Let the app register its event listeners before the mock scan starts.
    setTimeout(() => void mock.applyMockState({ scan: useScanStore, space: useSpaceStore }), 250)
    return
  }
  render()
}

void boot()
