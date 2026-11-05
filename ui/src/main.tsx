import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import Settings from './Settings'
import { windowRole } from './backend'
import './rail.css'

// Anything that goes wrong before React is up would otherwise be a blank
// window with no clue in it. Put the error on the page instead.
window.addEventListener('error', (e) => showFatal(String(e.error ?? e.message)))
window.addEventListener('unhandledrejection', (e) => showFatal(String(e.reason)))

function showFatal(message: string) {
  const root = document.getElementById('root')
  if (!root || root.childElementCount > 0) return
  root.innerHTML = `<pre style="padding:16px;white-space:pre-wrap;font:12px monospace;color:#d93a3f">Task Rail could not start this window:\n${message}</pre>`
}

// One bundle, two windows. The rail is the default; the settings window is
// told apart by its label in the app and by #settings in a browser.
const role = windowRole()
createRoot(document.getElementById('root')!).render(<StrictMode>{role === 'settings' ? <Settings /> : <App />}</StrictMode>)
