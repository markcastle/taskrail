import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import Settings from './Settings'
import { windowRole } from './backend'
import './rail.css'

// One bundle, two windows. The rail is the default; the settings window is
// told apart by its label in the app and by #settings in a browser.
windowRole().then((role) => {
  createRoot(document.getElementById('root')!).render(<StrictMode>{role === 'settings' ? <Settings /> : <App />}</StrictMode>)
})
