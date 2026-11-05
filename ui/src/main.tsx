import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import Settings from './Settings'
import { createBackend, windowRole } from './backend'
import './rail.css'

// Anything that goes wrong before React is up would otherwise be a blank
// window with no clue in it. Put the error on the page and in the log.
const backend = createBackend()
window.addEventListener('error', (e) => {
  const text = String(e.error ?? e.message)
  backend.logFromUi('error', text)
  showFatal(text)
})
window.addEventListener('unhandledrejection', (e) => {
  const text = String(e.reason)
  backend.logFromUi('error', text)
  showFatal(text)
})

function showFatal(message: string) {
  const root = document.getElementById('root')
  if (!root || root.childElementCount > 0) return
  root.innerHTML = `<pre style="padding:16px;white-space:pre-wrap;font:12px monospace;color:#d93a3f">Task Rail could not start this window:\n${message}</pre>`
}

// One window. Settings open inside the rail; #settings in a browser shows
// them on their own for development.
const role = windowRole()
backend.logFromUi('info', `page loaded as ${role}`)
createRoot(document.getElementById('root')!).render(
  <StrictMode>{role === 'settings' ? <Settings onClose={() => window.close()} /> : <App />}</StrictMode>,
)
