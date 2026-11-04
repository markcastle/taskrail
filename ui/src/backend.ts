// Everything the UI needs from the shell, behind one interface.
//
// In the real app this is Tauri commands. In a plain browser it is an
// in-memory stand-in with the same rules, which is what `npm run ui:dev` and
// the Playwright tests use. The types mirror crates/core/src/domain.rs; if
// one changes, change the other.

export type ItemKind = 'task' | 'event'
export type Status = 'todo' | 'in_progress' | 'done'
export type Priority = 'lowest' | 'low' | 'normal' | 'high' | 'highest'
export type Edge = 'left' | 'right'

export interface Origin {
  connector: string
  external_id: string
  url: string | null
}

export interface TimeWindow {
  start: string
  end: string
}

export interface Item {
  id: string
  kind: ItemKind
  title: string
  status: Status
  priority: Priority
  rank: number
  origin: Origin | null
  window: TimeWindow | null
  due: string | null
  completed_at: string | null
  updated_at: string
}

export interface RailView {
  up_next: Item | null
  top: Item[]
  top_hidden: number
  in_progress: Item[]
  active: string | null
  done_today: Item[]
}

export interface RailSettings {
  edge: Edge
  width_px: number
  top_limit: number
  event_lead_minutes: number
}

export interface RailBackend {
  view(expandTop: boolean): Promise<RailView>
  addTask(title: string): Promise<Item>
  start(id: string): Promise<Item>
  complete(id: string): Promise<Item>
  reopen(id: string): Promise<Item>
  moveTask(id: string, status: Status, index: number): Promise<Item>
  remove(id: string): Promise<void>
  setActive(id: string | null): Promise<void>
  getSettings(): Promise<RailSettings>
  updateSettings(settings: RailSettings): Promise<RailSettings>
  /** Fires when something changed behind the UI's back. Returns an unsubscribe. */
  onChanged(callback: () => void): () => void
  /** Fires when the global quick-add shortcut was pressed. Returns an unsubscribe. */
  onQuickAdd(callback: () => void): () => void
}

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export function createBackend(): RailBackend {
  return inTauri ? tauriBackend() : browserBackend()
}

function tauriBackend(): RailBackend {
  const core = import('@tauri-apps/api/core')
  const event = import('@tauri-apps/api/event')
  const call = async <T>(command: string, args?: Record<string, unknown>) => (await core).invoke<T>(command, args)
  const listen = (name: string, callback: () => void) => {
    let unlisten: (() => void) | null = null
    let cancelled = false
    event.then((m) => m.listen(name, () => callback())).then((u) => (cancelled ? u() : (unlisten = u)))
    return () => {
      cancelled = true
      unlisten?.()
    }
  }
  return {
    view: (expandTop) => call('rail_view', { expandTop }),
    addTask: (title) => call('add_task', { title }),
    start: (id) => call('start_task', { id }),
    complete: (id) => call('complete_task', { id }),
    reopen: (id) => call('reopen_task', { id }),
    moveTask: (id, status, index) => call('move_task', { request: { id, status, index } }),
    remove: (id) => call('remove_task', { id }),
    setActive: (id) => call('set_active', { id }),
    getSettings: () => call('get_settings'),
    updateSettings: (settings) => call('update_settings', { settings }),
    onChanged: (cb) => listen('rail:changed', cb),
    onQuickAdd: (cb) => listen('rail:quick-add', cb),
  }
}

// ---------------------------------------------------------------------------
// Browser stand-in. Same rules as the core, written small.

const TOP_LIMIT_MAX = 10

function browserBackend(): RailBackend {
  const now = Date.now()
  const minutes = (n: number) => new Date(now + n * 60_000).toISOString()
  let nextRank = 100
  const task = (id: string, title: string, status: Status, priority: Priority = 'normal', due: string | null = null): Item => ({
    id,
    kind: 'task',
    title,
    status,
    priority,
    rank: (nextRank += 100),
    origin: null,
    window: null,
    due,
    completed_at: status === 'done' ? minutes(-120) : null,
    updated_at: minutes(0),
  })
  const event = (id: string, title: string, startsIn: number, lasts: number): Item => ({
    ...task(id, title, 'todo'),
    kind: 'event',
    window: { start: minutes(startsIn), end: minutes(startsIn + lasts) },
  })

  let items: Item[] = [
    event('q1-board-meeting', 'Q1 board meeting', 40, 60),
    event('team-sync', 'Team sync', 240, 30),
    task('prepare-board-deck', 'Prepare board deck', 'todo', 'highest', minutes(360)),
    task('reply-to-supplier', 'Reply to supplier', 'todo', 'high', minutes(480)),
    task('book-venue', 'Book venue', 'todo'),
    task('update-docs', 'Update docs', 'todo', 'low'),
    task('plan-next-sprint', 'Plan next sprint', 'todo'),
    task('release-notes', 'Release notes', 'todo'),
    task('deploy-to-dev', 'Deploy to dev', 'todo'),
    task('design-dashboard', 'Design dashboard', 'in_progress'),
    task('review-pull-request', 'Review pull request', 'in_progress'),
    task('triage-inbox', 'Triage inbox', 'done'),
    task('update-project-plan', 'Update project plan', 'done'),
  ]
  let active: string | null = 'design-dashboard'
  let settings: RailSettings = { edge: 'right', width_px: 320, top_limit: 5, event_lead_minutes: 60 }
  const changedListeners = new Set<() => void>()
  const quickAddListeners = new Set<() => void>()
  const notify = () => changedListeners.forEach((cb) => cb())

  const find = (id: string) => {
    const item = items.find((i) => i.id === id)
    if (!item) throw new Error(`no item ${id}`)
    if (item.kind === 'event') throw new Error(`${id} is an event`)
    return item
  }
  const section = (status: Status, except?: string) =>
    items.filter((i) => i.kind === 'task' && i.status === status && i.id !== except).sort((a, b) => a.rank - b.rank)
  const moveTo = (id: string, status: Status, index: number) => {
    const item = find(id)
    const others = section(status, id)
    const at = Math.min(index, others.length)
    others.splice(at, 0, item)
    others.forEach((it, i) => (it.rank = (i + 1) * 1000))
    if (item.status !== status) {
      item.status = status
      item.completed_at = status === 'done' ? new Date().toISOString() : null
    }
    item.updated_at = new Date().toISOString()
    return item
  }

  if (typeof window !== 'undefined') {
    window.addEventListener('keydown', (e) => {
      if (e.ctrlKey && e.shiftKey && e.code === 'Space') {
        e.preventDefault()
        quickAddListeners.forEach((cb) => cb())
      }
    })
  }

  const async = <T>(fn: () => T): Promise<T> => Promise.resolve().then(fn)

  return {
    view: (expandTop) =>
      async(() => {
        const t = Date.now()
        const upcoming = items
          .filter((i) => i.kind === 'event' && i.window && Date.parse(i.window.end) > t)
          .sort((a, b) => Date.parse(a.window!.start) - Date.parse(b.window!.start))
        const soon = upcoming.filter((i) => Date.parse(i.window!.start) <= t + settings.event_lead_minutes * 60_000)
        let top = [...soon, ...section('todo')]
        const limit = expandTop ? top.length : Math.min(Math.max(settings.top_limit, 1), TOP_LIMIT_MAX)
        const top_hidden = Math.max(top.length - limit, 0)
        top = top.slice(0, limit)
        const in_progress = section('in_progress').sort((a, b) => Number(b.id === active) - Number(a.id === active))
        const midnight = new Date().setHours(0, 0, 0, 0)
        const done_today = items
          .filter((i) => i.kind === 'task' && i.status === 'done' && i.completed_at && Date.parse(i.completed_at) >= midnight)
          .sort((a, b) => Date.parse(b.completed_at!) - Date.parse(a.completed_at!))
        return {
          up_next: upcoming[0] ?? null,
          top,
          top_hidden,
          in_progress,
          active: in_progress.some((i) => i.id === active) ? active : null,
          done_today,
        }
      }),
    addTask: (title) =>
      async(() => {
        const first = section('todo')[0]
        const item = task(`local-${Date.now()}-${Math.random().toString(16).slice(2, 10)}`, title.trim(), 'todo')
        item.rank = first ? first.rank - 1000 : 1000
        items = [item, ...items]
        notify()
        return item
      }),
    start: (id) => async(() => (moveTo(id, 'in_progress', 0), notify(), find(id))),
    complete: (id) =>
      async(() => {
        const item = find(id)
        item.status = 'done'
        item.completed_at = new Date().toISOString()
        if (active === id) active = null
        notify()
        return item
      }),
    reopen: (id) => async(() => (moveTo(id, 'todo', 0), notify(), find(id))),
    moveTask: (id, status, index) =>
      async(() => {
        const item = moveTo(id, status, index)
        if (status !== 'in_progress' && active === id) active = null
        notify()
        return item
      }),
    remove: (id) =>
      async(() => {
        items = items.filter((i) => i.id !== id)
        if (active === id) active = null
        notify()
      }),
    setActive: (id) => async(() => ((active = id), notify())),
    getSettings: () => async(() => settings),
    updateSettings: (next) => async(() => ((settings = { ...next }), notify(), settings)),
    onChanged: (cb) => (changedListeners.add(cb), () => changedListeners.delete(cb)),
    onQuickAdd: (cb) => (quickAddListeners.add(cb), () => quickAddListeners.delete(cb)),
  }
}
