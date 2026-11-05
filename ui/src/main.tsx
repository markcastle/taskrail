import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import Settings from './Settings'
import './rail.css'

// One bundle, two windows. The rail loads the root; the settings window
// loads the same page with #settings.
const isSettings = window.location.hash === '#settings'

createRoot(document.getElementById('root')!).render(<StrictMode>{isSettings ? <Settings /> : <App />}</StrictMode>)
