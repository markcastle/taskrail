export function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}

/** "in 40 min", "now", "in 3 h", or a clock time when it is further away. */
export function relativeStart(iso: string, now = Date.now()): string {
  const diff = Date.parse(iso) - now
  const minutes = Math.round(diff / 60_000)
  if (minutes <= 0) return 'now'
  if (minutes < 60) return `in ${minutes} min`
  if (minutes < 6 * 60) return `in ${Math.round(minutes / 60)} h`
  return formatTime(iso)
}

/** "Due 16:00" today, "Due Tue" this week, otherwise "Due 12 Nov". */
export function dueLabel(iso: string, now = new Date()): string {
  const due = new Date(iso)
  const sameDay = due.toDateString() === now.toDateString()
  if (sameDay) return `Due ${formatTime(iso)}`
  const days = (due.getTime() - now.getTime()) / 86_400_000
  if (days > 0 && days < 6) return `Due ${due.toLocaleDateString(undefined, { weekday: 'short' })}`
  return `Due ${due.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`
}
