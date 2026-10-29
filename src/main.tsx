import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { HashRouter } from 'react-router-dom'
import App from './App'
import './index.css'

const container = document.getElementById('root')
if (!container) throw new Error('root element missing')
const mountPoint: HTMLElement = container

function render() {
  createRoot(mountPoint).render(
    <StrictMode>
      <HashRouter>
        <App />
      </HashRouter>
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
    render()
    // Let the app register its event listeners before the mock scan starts.
    setTimeout(() => void mock.applyMockState({ scan: useScanStore, space: useSpaceStore }), 250)
    return
  }
  render()
}

void boot()
