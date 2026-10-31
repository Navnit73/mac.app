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

/** Opened in a plain browser: there is no Rust side, so say so instead of failing quietly. */
function renderNoRuntime() {
  createRoot(mountPoint).render(
    <StrictMode>
      <main className="flex h-full flex-col items-center justify-center gap-2 p-8 text-center">
        <h1 className="text-xl font-semibold">MyMacStorage runs as a Mac app</h1>
        <p className="max-w-md text-muted">
          This page needs the MyMacStorage window to read your disk. Start it with{' '}
          <code className="font-mono">npm run tauri dev</code>.
        </p>
      </main>
    </StrictMode>,
  )
}

if ('__TAURI_INTERNALS__' in window) render()
else renderNoRuntime()
