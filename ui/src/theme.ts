import type { RailBackend, Theme } from './backend'

/** Put the chosen theme on the root element; CSS does the rest. */
export function applyTheme(theme: Theme) {
  const root = document.documentElement
  if (theme === 'system') delete root.dataset.theme
  else root.dataset.theme = theme
}

/** Follow the saved theme for the life of the page. Returns an unsubscribe. */
export function followTheme(backend: RailBackend): () => void {
  const sync = () =>
    backend
      .getSettings()
      .then((s) => applyTheme(s.theme ?? 'system'))
      .catch(() => {})
  sync()
  return backend.onChanged(sync)
}
