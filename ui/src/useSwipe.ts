import { useRef, useState, type PointerEvent } from 'react'

export type SwipeDirection = 'left' | 'right'

/**
 * Flick-to-complete. Horizontal pulls follow the pointer; a fast or long one
 * sends the card off screen and fires `onFlick`. Anything that did not move
 * counts as a tap. Vertical movement is left alone so press-and-hold drag
 * (dnd-kit) keeps working on the same card.
 */
export function useSwipe(enabled: boolean, onFlick: (direction: SwipeDirection) => void, onTap: () => void) {
  const [dx, setDx] = useState(0)
  const [leaving, setLeaving] = useState<SwipeDirection | null>(null)
  const start = useRef<{ x: number; y: number; t: number; engaged: boolean } | null>(null)

  const onPointerDown = (e: PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return
    start.current = { x: e.clientX, y: e.clientY, t: performance.now(), engaged: false }
  }

  const onPointerMove = (e: PointerEvent<HTMLElement>) => {
    const s = start.current
    if (!s || !enabled) return
    const ddx = e.clientX - s.x
    const ddy = e.clientY - s.y
    if (!s.engaged) {
      if (Math.abs(ddx) > 12 && Math.abs(ddx) > Math.abs(ddy) * 1.5) {
        s.engaged = true
        e.currentTarget.setPointerCapture(e.pointerId)
      } else {
        return
      }
    }
    setDx(ddx)
  }

  const onPointerUp = (e: PointerEvent<HTMLElement>) => {
    const s = start.current
    start.current = null
    if (!s) return
    const ddx = e.clientX - s.x
    const ddy = e.clientY - s.y
    if (!s.engaged) {
      if (Math.abs(ddx) < 6 && Math.abs(ddy) < 6) onTap()
      return
    }
    const dt = Math.max(performance.now() - s.t, 1)
    const velocity = Math.abs(ddx) / dt
    if (Math.abs(ddx) > 110 || velocity > 0.7) {
      const direction: SwipeDirection = ddx > 0 ? 'right' : 'left'
      setLeaving(direction)
      window.setTimeout(() => onFlick(direction), 180)
    } else {
      setDx(0)
    }
  }

  const onPointerCancel = () => {
    start.current = null
    setDx(0)
  }

  return {
    dx,
    leaving,
    handlers: { onPointerDown, onPointerMove, onPointerUp, onPointerCancel },
  }
}
