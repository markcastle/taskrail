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

function dayWord(date: Date, now: Date): string | null {
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const startOfThat = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime()
  const days = Math.round((startOfThat - startOfToday) / 86_400_000)
  if (days === 0) return 'Today'
  if (days === 1) return 'Tomorrow'
  if (days === -1) return 'Yesterday'
  if (days > 1 && days < 7) return date.toLocaleDateString(undefined, { weekday: 'short' })
  return null
}

/** When a task is due, the way a person says it: "Due today", "Overdue", "Due Tue", "Due 12 Nov". */
export function dueWord(iso: string, now = new Date()): { text: string; overdue: boolean } {
  const due = new Date(iso)
  const word = dayWord(due, now)
  if (word === 'Today') return { text: 'Due today', overdue: due.getTime() < now.getTime() }
  if (due.getTime() < now.getTime()) return { text: 'Overdue', overdue: true }
  if (word) return { text: `Due ${word.toLowerCase() === 'tomorrow' ? 'tomorrow' : word}`, overdue: false }
  return { text: `Due ${due.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}`, overdue: false }
}

/** When an event starts: "Today, 14:30", "Tomorrow, 09:00", "Tue, 11:00", "12 Nov, 10:00". */
export function whenWord(iso: string, now = new Date()): string {
  const start = new Date(iso)
  const word = dayWord(start, now) ?? start.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
  return `${word}, ${formatTime(iso)}`
}

/** "Thursday, 5 November", in that order whatever the locale puts first. */
export function todayLine(now = new Date()): string {
  const weekday = now.toLocaleDateString(undefined, { weekday: 'long' })
  const month = now.toLocaleDateString(undefined, { month: 'long' })
  return `${weekday}, ${now.getDate()} ${month}`
}

/** The quick-add shortcut the way this platform writes it. */
export function shortcutHint(): string {
  const mac = /Mac|iPhone|iPad/.test(navigator.platform)
  return mac ? '⌃⇧Space' : 'Ctrl ⇧ Space'
}
