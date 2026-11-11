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
  /** Where it sits in its source system, outermost first: board, group, parent. Absent for local items. */
  trail?: string[]
}

export type Choice =
  | { kind: 'switch'; id: string; label: string; default?: boolean }
  | { kind: 'pick'; id: string; label: string; containers?: string; trail?: string }

export interface Scope {
  switches: Record<string, boolean>
  picks: Record<string, string[]>
}

export interface ConnectorScope {
  choices: Choice[]
  scope: Scope
  options: Record<string, string[]>
}

export interface RailView {
  up_next: Item | null
  top: Item[]
  top_hidden: number
  /** The items behind the fold, in order. */
  later: Item[]
  in_progress: Item[]
  active: string | null
  done_today: Item[]
  /** Today's calendar events that have not ended, soonest first. */
  events_today: Item[]
}

export type Theme = 'system' | 'light' | 'dark'

/** The narrowest and widest the rail goes, shared by the slider and the drag handle. */
export const WIDTH_MIN_PX = 240
export const WIDTH_MAX_PX = 600

export interface RailSettings {
  edge: Edge
  width_px: number
  /** Tuck the rail into a thin strip at the edge until the pointer rests on it. */
  auto_hide: boolean
  top_limit: number
  event_lead_minutes: number
  theme: Theme
  monitor: string | null
  /** Install new releases without asking. */
  auto_update: boolean
  /** Motion on the rail. The system's reduce-motion setting also turns it off. */
  animations: boolean
  /** A quiet tick on complete, a softer one on start. */
  sounds: boolean
}

export interface AvailableUpdate {
  version: string
  notes: string | null
  date: string | null
}

export interface UpdateStatus {
  current: string
  available: AvailableUpdate | null
}

export interface MonitorInfo {
  name: string
  width: number
  height: number
  x: number
  y: number
  scale: number
  primary: boolean
}

export interface ConnectorSummary {
  id: string
  name: string
  /** "mcp" for a profile-driven server, "calendar" for an iCalendar feed. */
  kind: 'mcp' | 'calendar'
  server: string
  capabilities: { read: boolean; write_status: boolean; create: boolean }
  poll_seconds: number
  last_sync: string | null
  last_error: string | null
  item_count: number
  /** Paused: its tasks stay on the rail, nothing is pulled or written back. */
  paused: boolean
}

export interface SyncError {
  connector: string
  message: string
}

export type Provider = 'anthropic' | 'open_ai' | 'open_router'

export interface ModelSettings {
  provider: Provider
  model: string | null
  base_url: string | null
  has_key: boolean
}

export type ServerSpec =
  | { transport: 'stdio'; command: string; args: string[]; env: Record<string, string> }
  | { transport: 'http'; url: string; bearer_secret: string | null; oauth_secret?: string | null }

export interface DiscoverRequest {
  id: string
  name: string
  server: ServerSpec
  kind: ItemKind
  hint: string
  allow_unannotated_list: boolean
  bearer?: string | null
  /** A blueprint to use instead of the registry. */
  blueprint?: string | null
}

export interface ToolSummary {
  name: string
  description: string
  read_only: boolean
}

export interface DiscoveryOutcome {
  /** The blueprint that fitted the server, when no model was needed. */
  blueprint: string | null
  profile: Record<string, unknown>
  sample: { external_id: string; title: string; status: string | null }[]
  tools: ToolSummary[]
  rounds: number
  notes: string[]
}

export type DiscoveryResult =
  | ({ status: 'ok' } & DiscoveryOutcome)
  | { status: 'needs_confirmation'; tool: string; profile: Record<string, unknown> }
  | { status: 'sign_in_required' }

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
  /** Close the app. Does nothing in a plain browser. */
  quit(): Promise<void>
  /** Widen the rail for settings, or give the width back. */
  expandRail(expanded: boolean): Promise<void>
  /** With auto-hide on: out of the strip over the desktop, or back into it. */
  revealRail(revealed: boolean): Promise<void>
  /** A width being dragged out: the window follows, nothing is saved. `null` ends the preview. */
  previewWidth(widthPx: number | null): Promise<void>
  /** Fires when the tray or the shortcut brought an auto-hidden rail out. Returns an unsubscribe. */
  onReveal(callback: () => void): () => void
  /** The displays attached right now, primary first. */
  listMonitors(): Promise<MonitorInfo[]>
  /** Fires when the tray asks the rail to show settings. Returns an unsubscribe. */
  onOpenSettings(callback: () => void): () => void
  /** The tail of the app's log file. */
  readLog(lines?: number): Promise<string>
  /** Version and commit of the running build. */
  appVersion(): Promise<string>
  /** Ask the release feed now. */
  checkForUpdate(): Promise<UpdateStatus>
  /** Download, verify and install the newest release, then restart. Never resolves on success. */
  installUpdate(): Promise<void>
  /** Fires when a periodic check finds a newer release. Returns an unsubscribe. */
  onUpdateAvailable(callback: (update: AvailableUpdate) => void): () => void
  /** Reveal the log folder in the file manager. */
  openLogFolder(): Promise<void>
  /** Write a line from this page into the app's log. Never rejects. */
  logFromUi(level: 'info' | 'warn' | 'error', message: string): Promise<void>
  listConnectors(): Promise<ConnectorSummary[]>
  /** Add a connector from a Connector Profile, then pull from it. A bearer token, if given, goes to the keychain. */
  addConnectorProfile(profileJson: string, bearer?: string): Promise<ConnectorSummary>
  /** Add the built-in demo tracker, then pull from it. */
  addDemoConnector(): Promise<ConnectorSummary>
  /** Add a calendar by its iCalendar address. Read once to check it, then pulled. */
  addCalendar(name: string, url: string): Promise<ConnectorSummary>
  removeConnector(id: string): Promise<void>
  /** Pause or resume a connector. Resuming pulls straight away. */
  setConnectorPaused(id: string, paused: boolean): Promise<ConnectorSummary>
  syncConnector(id: string): Promise<number>
  /** Fires when a pull or a write-back failed. Returns an unsubscribe. */
  onSyncError(callback: (error: SyncError) => void): () => void
  getModelSettings(): Promise<ModelSettings>
  /** `apiKey` undefined leaves the stored key alone; empty string removes it. */
  saveModelSettings(config: { provider: Provider; model: string | null; base_url: string | null }, apiKey?: string): Promise<ModelSettings>
  /** One cheap round trip; resolves to a line describing who replied. */
  testModel(): Promise<string>
  /** Ask the model to write a profile for a server and validate it live. */
  discoverConnector(request: DiscoverRequest): Promise<DiscoveryResult>
  /** A saved connector's profile as a blueprint: no server, no secrets, pretty JSON. */
  exportBlueprint(id: string): Promise<string>
  /** What the connector's server really returns for its list call, trimmed. */
  connectorSample(id: string): Promise<string>
  /** The choices a connector offers, the person's answers, and what each pick can be chosen from. */
  getConnectorScope(id: string): Promise<ConnectorScope>
  /** Save the answers and pull with them straight away. */
  setConnectorScope(id: string, scope: Scope): Promise<void>
  /** Open one of the project's GitHub pages in the browser. Refuses anything else. */
  openProjectPage(url: string): Promise<void>
  /** Run the MCP sign-in flow in the browser for a server. Resolves when the browser comes back. */
  signIn(url: string): Promise<void>
  /** Fires when something changed behind the UI's back. Returns an unsubscribe. */
  onChanged(callback: () => void): () => void
  /** Fires when the global quick-add shortcut was pressed. Returns an unsubscribe. */
  onQuickAdd(callback: () => void): () => void
}

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** In a browser, `#settings` shows the settings page on its own for development. */
export function windowRole(): 'rail' | 'settings' {
  return window.location.hash === '#settings' ? 'settings' : 'rail'
}

export function createBackend(): RailBackend {
  return inTauri ? tauriBackend() : browserBackend()
}

function tauriBackend(): RailBackend {
  const core = import('@tauri-apps/api/core')
  const event = import('@tauri-apps/api/event')
  const call = async <T>(command: string, args?: Record<string, unknown>) => (await core).invoke<T>(command, args)
  const listen = <T,>(name: string, callback: (payload: T) => void) => {
    let unlisten: (() => void) | null = null
    let cancelled = false
    event.then((m) => m.listen<T>(name, (e) => callback(e.payload))).then((u) => (cancelled ? u() : (unlisten = u)))
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
    quit: () => call('quit'),
    expandRail: (expanded) => call('expand_rail', { expanded }),
    revealRail: (revealed) => call('reveal_rail', { revealed }),
    previewWidth: (widthPx) => call('preview_width', { widthPx }),
    onReveal: (cb) => listen('rail:reveal', cb),
    listMonitors: () => call('list_monitors'),
    onOpenSettings: (cb) => listen('rail:open-settings', cb),
    readLog: (lines) => call('read_log', { lines: lines ?? null }),
    appVersion: () => call('app_version'),
    checkForUpdate: () => call('check_for_update'),
    installUpdate: () => call('install_update'),
    onUpdateAvailable: (cb) => listen<AvailableUpdate>('rail:update-available', cb),
    openLogFolder: () => call('open_log_folder'),
    logFromUi: (level, message) => call<void>('log_from_ui', { level, message }).catch(() => undefined),
    listConnectors: () => call('list_connectors'),
    addConnectorProfile: (profileJson, bearer) => call('add_connector_profile', { profileJson, bearer: bearer ?? null }),
    addDemoConnector: () => call('add_demo_connector'),
    addCalendar: (name, url) => call('add_calendar', { name, url }),
    removeConnector: (id) => call('remove_connector', { id }),
    setConnectorPaused: (id, paused) => call('set_connector_paused', { id, paused }),
    syncConnector: (id) => call('sync_connector', { id }),
    onChanged: (cb) => listen('rail:changed', cb),
    onQuickAdd: (cb) => listen('rail:quick-add', cb),
    onSyncError: (cb) => listen<SyncError>('rail:sync-error', cb),
    getModelSettings: () => call('get_model_settings'),
    saveModelSettings: (config, apiKey) => call('save_model_settings', { config, apiKey: apiKey ?? null }),
    testModel: () => call('test_model'),
    discoverConnector: (request) => call('discover_connector', { request }),
    exportBlueprint: (id) => call('export_blueprint', { id }),
    connectorSample: (id) => call('connector_sample', { id }),
    getConnectorScope: (id) => call('get_connector_scope', { id }),
    setConnectorScope: (id, scope) => call('set_connector_scope', { id, scope }),
    openProjectPage: (url) => call('open_project_page', { url }),
    signIn: (url) => call('sign_in', { url }),
  }
}

// ---------------------------------------------------------------------------
// Browser stand-in. Same rules as the core, written small. State lives in
// localStorage so the rail tab and the settings tab see the same data, the
// way the two windows do in the real app.

const TOP_LIMIT_MAX = 10
const STORAGE_KEY = 'taskrail-dev-state'
const CHANNEL = 'taskrail-dev'

interface DevState {
  items: Item[]
  active: string | null
  settings: RailSettings
  connectors: ConnectorSummary[]
  modelSettings: ModelSettings
  signedIn?: string[]
}

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
    // Two hours ago, but never before today began: the day's count must hold whatever the clock says.
    completed_at: status === 'done' ? new Date(Math.max(new Date().setHours(0, 0, 0, 0) + 60_000, now - 120 * 60_000)).toISOString() : null,
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
    task('book-venue', 'Book venue', 'todo', 'normal', minutes(-24 * 60)),
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
  let settings: RailSettings = { edge: 'right', width_px: 320, auto_hide: false, top_limit: 5, event_lead_minutes: 60, theme: 'system', monitor: null, auto_update: false, animations: true, sounds: false }
  let connectors: ConnectorSummary[] = []
  let modelSettings: ModelSettings = { provider: 'anthropic', model: null, base_url: null, has_key: false }
  let signedIn: string[] = []
  const devLog: string[] = [`${new Date().toISOString()} [info] browser stand-in started`]
  const changedListeners = new Set<() => void>()
  const updateListeners = new Set<(update: AvailableUpdate) => void>()
  const scopes: Record<string, Scope> = {}
  const quickAddListeners = new Set<() => void>()
  const syncErrorListeners = new Set<(e: SyncError) => void>()

  const load = () => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (!raw) return
      const state = JSON.parse(raw) as DevState
      items = state.items
      active = state.active
      settings = {
        ...state.settings,
        theme: state.settings.theme ?? 'system',
        auto_hide: state.settings.auto_hide ?? false,
        monitor: state.settings.monitor ?? null,
        auto_update: state.settings.auto_update ?? false,
        animations: state.settings.animations ?? true,
        sounds: state.settings.sounds ?? false,
      }
      connectors = state.connectors
      modelSettings = state.modelSettings
      signedIn = state.signedIn ?? []
    } catch {
      /* fresh state */
    }
  }
  const persist = () => {
    try {
      const state: DevState = { items, active, settings, connectors, modelSettings, signedIn }
      localStorage.setItem(STORAGE_KEY, JSON.stringify(state))
    } catch {
      /* private mode or blocked storage: keep going in memory */
    }
  }
  const channel = typeof BroadcastChannel !== 'undefined' ? new BroadcastChannel(CHANNEL) : null
  const notify = () => {
    persist()
    changedListeners.forEach((cb) => cb())
    channel?.postMessage('changed')
  }
  channel?.addEventListener('message', (event) => {
    // An update found in one tab is announced in every tab, the way the app
    // emits the event to its one window.
    if (event.data && typeof event.data === 'object' && event.data.type === 'update') {
      updateListeners.forEach((cb) => cb(event.data.update))
      return
    }
    load()
    changedListeners.forEach((cb) => cb())
  })
  load()
  persist()
  const demoSummary = (): ConnectorSummary => ({
    id: 'demo-tracker',
    name: 'Demo tracker',
    kind: 'mcp',
    server: 'taskrail --fake-mcp-server',
    capabilities: { read: true, write_status: true, create: true },
    poll_seconds: 30,
    last_sync: new Date().toISOString(),
    last_error: null,
    item_count: 3,
    paused: false,
  })

  const refuseIfPaused = (id: string) => {
    const origin = items.find((i) => i.id === id)?.origin
    const c = origin && connectors.find((c) => c.id === origin.connector)
    if (c?.paused) throw new Error(`${c.name} is paused. Resume it to change its tasks.`)
  }
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

  const async = <T>(fn: () => T): Promise<T> =>
    Promise.resolve().then(() => {
      load()
      return fn()
    })

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
        const later = top.slice(limit)
        const top_hidden = later.length
        top = top.slice(0, limit)
        const in_progress = section('in_progress').sort((a, b) => Number(b.id === active) - Number(a.id === active))
        const midnight = new Date().setHours(0, 0, 0, 0)
        const done_today = items
          .filter((i) => i.kind === 'task' && i.status === 'done' && i.completed_at && Date.parse(i.completed_at) >= midnight)
          .sort((a, b) => Date.parse(b.completed_at!) - Date.parse(a.completed_at!))
        const endOfToday = new Date().setHours(24, 0, 0, 0)
        const events_today = upcoming.filter((i) => Date.parse(i.window!.start) < endOfToday)
        return {
          events_today,
          up_next: upcoming[0] ?? null,
          top,
          top_hidden,
          later,
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
    start: (id) => async(() => (refuseIfPaused(id), moveTo(id, 'in_progress', 0), notify(), find(id))),
    complete: (id) =>
      async(() => {
        refuseIfPaused(id)
        const item = find(id)
        item.status = 'done'
        item.completed_at = new Date().toISOString()
        if (active === id) active = null
        notify()
        return item
      }),
    reopen: (id) => async(() => (refuseIfPaused(id), moveTo(id, 'todo', 0), notify(), find(id))),
    moveTask: (id, status, index) =>
      async(() => {
        if (items.find((i) => i.id === id)?.status !== status) refuseIfPaused(id)
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
    quit: () => async(() => undefined),
    expandRail: () => async(() => undefined),
    revealRail: (revealed) =>
      async(() => {
        devLog.push(`${new Date().toISOString()} [info] stand-in rail ${revealed ? 'out' : 'tucked away'}`)
      }),
    previewWidth: () => async(() => undefined),
    onReveal: () => () => undefined,
    listMonitors: () =>
      async(() => [
        { name: '\\\\.\\DISPLAY1', width: 2560, height: 1440, x: 0, y: 0, scale: 1, primary: true },
        { name: '\\\\.\\DISPLAY2', width: 1920, height: 1080, x: 2560, y: 0, scale: 1, primary: false },
        { name: '\\\\.\\DISPLAY3', width: 3840, height: 1100, x: 4480, y: 0, scale: 1, primary: false },
      ]),
    onOpenSettings: () => () => undefined,
    readLog: () => async(() => devLog.join('\n')),
    appVersion: () => async(() => 'browser stand-in'),
    // The stand-in always has a newer release waiting, so the banner and the
    // settings page can be seen and tested. Installing just logs.
    checkForUpdate: () =>
      new Promise((resolve) => {
        window.setTimeout(() => {
          const update = { version: '0.2.0', notes: 'Smoother gestures and a quieter tray.', date: new Date().toISOString() }
          updateListeners.forEach((cb) => cb(update))
          channel?.postMessage({ type: 'update', update })
          resolve({ current: '0.1.0', available: update })
        }, 300)
      }),
    installUpdate: () =>
      new Promise((_, reject) => {
        devLog.push(`${new Date().toISOString()} [info] stand-in would install 0.2.0 and restart`)
        window.setTimeout(() => reject(new Error('the browser stand-in cannot restart itself')), 300)
      }),
    onUpdateAvailable: (cb) => (updateListeners.add(cb), () => updateListeners.delete(cb)),
    openLogFolder: () => async(() => undefined),
    logFromUi: (level, message) =>
      async(() => {
        devLog.push(`${new Date().toISOString()} [${level}] ${message}`)
      }),
    listConnectors: () => async(() => connectors),
    addConnectorProfile: (profileJson) =>
      async(() => {
        let profile: { id?: string; name?: string }
        try {
          profile = JSON.parse(profileJson)
        } catch (e) {
          throw new Error(`not a valid profile: ${e}`)
        }
        if (!profile.id || !profile.name) throw new Error('a profile needs an id and a name')
        if (connectors.some((c) => c.id === profile.id)) throw new Error(`a connector with id "${profile.id}" already exists`)
        const summary: ConnectorSummary = { ...demoSummary(), id: profile.id, name: profile.name, item_count: 0 }
        connectors = [...connectors, summary]
        notify()
        return summary
      }),
    addCalendar: (name, url) =>
      async(() => {
        if (!/^(https?|webcal):\/\//.test(url.trim())) throw new Error('a calendar address starts with https:// or webcal://')
        if (url.includes('broken')) throw new Error('that address did not return a calendar: not an iCalendar feed')
        const title = name.trim() || 'Calendar'
        const id = title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'calendar'
        if (connectors.some((c) => c.id === id)) throw new Error(`a connector with id "${id}" already exists`)
        const summary: ConnectorSummary = { ...demoSummary(), id, name: title, kind: 'calendar', server: 'iCalendar feed', capabilities: { read: true, write_status: false, create: false }, poll_seconds: 300, item_count: 2 }
        connectors = [...connectors, summary]
        const origin = (ext: string) => ({ connector: id, external_id: ext, url: null })
        items = [
          ...items,
          { ...task(`${id}:ev-1`, 'Design review', 'todo'), kind: 'event', window: { start: minutes(90), end: minutes(150) }, origin: origin('ev-1') },
          { ...task(`${id}:ev-2`, 'Dentist', 'todo'), kind: 'event', window: { start: minutes(24 * 60 + 60), end: minutes(24 * 60 + 120) }, origin: origin('ev-2') },
        ]
        notify()
        return summary
      }),
    addDemoConnector: () =>
      async(() => {
        if (connectors.some((c) => c.id === 'demo-tracker')) throw new Error('a connector with id "demo-tracker" already exists')
        const summary = demoSummary()
        connectors = [...connectors, summary]
        const origin = (id: string) => ({ connector: 'demo-tracker', external_id: id, url: `https://tracker.example/tickets/${id}` })
        items = [
          ...items,
          { ...task('demo-tracker:T-201', 'Fix login timeout', 'todo', 'highest'), origin: origin('T-201'), trail: ['Work', 'Now'] },
          { ...task('demo-tracker:T-202', 'Write release notes', 'todo'), origin: origin('T-202'), trail: ['Work', 'Now', 'Fix login timeout'] },
          { ...task('demo-tracker:T-203', 'Migrate billing job', 'in_progress'), origin: origin('T-203') },
        ]
        notify()
        return summary
      }),
    setConnectorPaused: (id, paused) =>
      async(() => {
        const c = connectors.find((c) => c.id === id)
        if (!c) throw new Error(`no connector ${id}`)
        c.paused = paused
        notify()
        return c
      }),
    removeConnector: (id) =>
      async(() => {
        connectors = connectors.filter((c) => c.id !== id)
        items = items.filter((i) => i.origin?.connector !== id)
        notify()
      }),
    syncConnector: (id) => async(() => items.filter((i) => i.origin?.connector === id).length),
    onSyncError: (cb) => (syncErrorListeners.add(cb), () => syncErrorListeners.delete(cb)),
    getModelSettings: () => async(() => modelSettings),
    saveModelSettings: (config, apiKey) =>
      async(() => {
        const has_key = apiKey === undefined ? modelSettings.has_key : apiKey.trim().length > 0
        modelSettings = { ...config, has_key }
        notify()
        return modelSettings
      }),
    testModel: () =>
      new Promise((resolve, reject) =>
        setTimeout(() => {
          load()
          if (!modelSettings.has_key) return reject(new Error('no API key saved; add one under Model in settings'))
          resolve(`${modelSettings.provider} · ${modelSettings.model ?? 'default model'} replied: Hello, Task Rail.`)
        }, 400),
      ),
    discoverConnector: (request) =>
      new Promise((resolve, reject) =>
        setTimeout(() => {
          load()
          // No name given: the blueprint's title, else the server's own (here, its host). The id steps aside from taken ones.
          const settle = (blueprintTitle: string | null) => {
            const host = request.server.transport === 'http' ? new URL(request.server.url).hostname : request.server.command
            const name = request.name.trim() || blueprintTitle || host
            const base = request.id.trim() || name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'connector'
            let id = base
            for (let n = 2; connectors.some((c) => c.id === id); n++) id = `${base}-${n}`
            return { id, name }
          }
          // A pasted blueprint, or a server whose URL says "blueprint", is one the registry knows: no model needed.
          let pasted: string | null = null
          if (request.blueprint?.trim()) {
            try {
              pasted = String(JSON.parse(request.blueprint).title ?? 'pasted')
            } catch (e) {
              return reject(new Error(`that is not a blueprint: ${(e as Error).message}`))
            }
          }
          if (pasted || (request.server.transport === 'http' && request.server.url.includes('blueprint'))) {
            return resolve({
              status: 'ok',
              blueprint: pasted ?? 'Demo tracker',
              profile: { ...settle(pasted ?? 'Demo tracker'), server: request.server, list: { tool: 'list_tickets' }, identity: { tool: 'whoami', id: '$.id' } },
              sample: [
                { external_id: 'T-101', title: 'Prepare board deck', status: 'Selected' },
                { external_id: 'T-102', title: 'Reply to supplier', status: 'Selected' },
              ],
              tools: [{ name: 'list_tickets', description: '', read_only: true }, { name: 'whoami', description: '', read_only: true }],
              rounds: 0,
              notes: [],
            })
          }
          if (!modelSettings.has_key) return reject(new Error('no blueprint matches this server, and no API key is saved; add one under Model in settings'))
          if (request.server.transport === 'stdio' && request.server.command.includes('risky') && !request.allow_unannotated_list) {
            return resolve({ status: 'needs_confirmation', tool: 'transition_ticket', profile: { id: settle(null).id } })
          }
          if (request.server.transport === 'http' && request.server.url.includes('oauth') && !signedIn.includes(request.server.url)) {
            return resolve({ status: 'sign_in_required' })
          }
          resolve({
            status: 'ok',
            blueprint: null,
            profile: {
              ...settle(null),
              server: request.server,
              kind: request.kind,
              list: { tool: 'list_tickets', arguments: { assignee: 'me' }, items: '$.tickets[*]', fields: { external_id: '$.ticket_id', title: '$.summary', status: '$.state' } },
              status_map: { to_section: { Open: 'todo', Doing: 'in_progress', Closed: 'done' }, canonical: { todo: 'Open', in_progress: 'Doing', done: 'Closed' } },
            },
            sample: [
              { external_id: 'T-1', title: 'Fix login timeout', status: 'Open' },
              { external_id: 'T-2', title: 'Write release notes', status: 'Doing' },
            ],
            tools: [
              { name: 'list_tickets', description: 'List tickets', read_only: true },
              { name: 'transition_ticket', description: 'Move a ticket', read_only: false },
            ],
            rounds: 1,
            notes: [],
          })
        }, 300),
      ),
    exportBlueprint: (id) =>
      async(() =>
        JSON.stringify(
          { id, title: connectors.find((c) => c.id === id)?.name ?? id, description: 'What this system is and what the profile pulls.', match: { tools: ['list_tickets', 'transition_ticket'] }, profile: { kind: 'task', list: { tool: 'list_tickets' } } },
          null,
          2,
        ),
      ),
    getConnectorScope: (id) =>
      async(() => ({
        choices: [
          { kind: 'switch', id: 'mine', label: 'Only tasks assigned to me', default: true },
          { kind: 'pick', id: 'boards', label: 'Boards', containers: '$.name' },
          { kind: 'pick', id: 'groups', label: 'Groups', trail: 'group' },
        ],
        scope: scopes[id] ?? { switches: {}, picks: {} },
        options: { boards: ['Work', 'Home'], groups: ['Later', 'Now'] },
      })),
    setConnectorScope: (id, scope) =>
      async(() => {
        scopes[id] = scope
        notify()
      }),
    connectorSample: (id) =>
      async(
        () =>
          `=== list_tickets (${id}) ===\n` +
          JSON.stringify({ tickets: [{ ticket_id: 'T-101', summary: 'Prepare board deck', state: 'Selected', assignee: 'mark', board: { id: 'b-1111', name: 'Work' } }] }, null, 2),
      ),
    openProjectPage: (url) =>
      async(() => {
        devLog.push(`${new Date().toISOString()} [info] stand-in would open ${url}`)
      }),
    signIn: (url) =>
      new Promise((resolve) =>
        setTimeout(() => {
          load()
          signedIn = [...signedIn, url]
          persist()
          resolve()
        }, 300),
      ),
    onChanged: (cb) => (changedListeners.add(cb), () => changedListeners.delete(cb)),
    onQuickAdd: (cb) => (quickAddListeners.add(cb), () => quickAddListeners.delete(cb)),
  }
}
