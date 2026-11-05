import { useEffect, useRef } from 'react'

/** Debounce a save so a slider in motion does not fire one per pixel. */
export function useDebouncedSave<T>(value: T | null, save: (value: T) => void, delay = 400) {
  const first = useRef(true)
  useEffect(() => {
    if (value === null) return
    if (first.current) {
      first.current = false
      return
    }
    const t = window.setTimeout(() => save(value), delay)
    return () => window.clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value])
}
