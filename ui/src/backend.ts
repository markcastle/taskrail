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

export interface ConnectorSummary {
  id: string
  name: string
  server: string
  capabilities: { read: boolean; write_status: boolean; create: boolean }
  poll_seconds: number
  last_sync: string | null
  last_error: string | null
  item_count: number
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
}

export interface ToolSummary {
  name: string
  description: string
  read_only: boolean
}

export interface DiscoveryOutcome {
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
  /** Open (or focus) the settings window. In a plain browser, opens the settings route in a new tab. */
  openSettings(): Promise<void>
  listConnectors(): Promise<ConnectorSummary[]>
  /** Add a connector from a Connector Profile, then pull from it. A bearer token, if given, goes to the keychain. */
  addConnectorProfile(profileJson: string, bearer?: string): Promise<ConnectorSummary>
  /** Add the built-in demo tracker, then pull from it. */
  addDemoConnector(): Promise<ConnectorSummary>
  removeConnector(id: string): Promise<void>
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
  /** Run the MCP sign-in flow in the browser for a server. Resolves when the browser comes back. */
  signIn(id: string, url: string): Promise<void>
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
    openSettings: () => call('open_settings'),
    listConnectors: () => call('list_connectors'),
    addConnectorProfile: (profileJson, bearer) => call('add_connector_profile', { profileJson, bearer: bearer ?? null }),
    addDemoConnector: () => call('add_demo_connector'),
    removeConnector: (id) => call('remove_connector', { id }),
    syncConnector: (id) => call('sync_connector', { id }),
    onChanged: (cb) => listen('rail:changed', cb),
    onQuickAdd: (cb) => listen('rail:quick-add', cb),
    onSyncError: (cb) => listen<SyncError>('rail:sync-error', cb),
    getModelSettings: () => call('get_model_settings'),
    saveModelSettings: (config, apiKey) => call('save_model_settings', { config, apiKey: apiKey ?? null }),
    testModel: () => call('test_model'),
    discoverConnector: (request) => call('discover_connector', { request }),
    signIn: (id, url) => call('sign_in', { id, url }),
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
  let connectors: ConnectorSummary[] = []
  let modelSettings: ModelSettings = { provider: 'anthropic', model: null, base_url: null, has_key: false }
  let signedIn: string[] = []
  const changedListeners = new Set<() => void>()
  const quickAddListeners = new Set<() => void>()
  const syncErrorListeners = new Set<(e: SyncError) => void>()

  const load = () => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (!raw) return
      const state = JSON.parse(raw) as DevState
      items = state.items
      active = state.active
      settings = state.settings
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
  channel?.addEventListener('message', () => {
    load()
    changedListeners.forEach((cb) => cb())
  })
  load()
  persist()
  const demoSummary = (): ConnectorSummary => ({
    id: 'demo-tracker',
    name: 'Demo tracker',
    server: 'taskrail --fake-mcp-server',
    capabilities: { read: true, write_status: true, create: true },
    poll_seconds: 30,
    last_sync: new Date().toISOString(),
    last_error: null,
    item_count: 3,
  })

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
    quit: () => async(() => undefined),
    openSettings: () =>
      async(() => {
        window.open('/#settings', 'taskrail-settings')
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
        const summary = { ...demoSummary(), id: profile.id, name: profile.name, item_count: 0 }
        connectors = [...connectors, summary]
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
          { ...task('demo-tracker:T-201', 'Fix login timeout', 'todo', 'highest'), origin: origin('T-201') },
          { ...task('demo-tracker:T-202', 'Write release notes', 'todo'), origin: origin('T-202') },
          { ...task('demo-tracker:T-203', 'Migrate billing job', 'in_progress'), origin: origin('T-203') },
        ]
        notify()
        return summary
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
      async(() => {
        if (!modelSettings.has_key) throw new Error('no API key saved; add one under Model in settings')
        return `${modelSettings.provider} · ${modelSettings.model ?? 'default model'} replied: Hello, Task Rail.`
      }),
    discoverConnector: (request) =>
      new Promise((resolve, reject) =>
        setTimeout(() => {
          load()
          if (!modelSettings.has_key) return reject(new Error('no API key saved; add one under Model in settings'))
          if (request.server.transport === 'stdio' && request.server.command.includes('risky') && !request.allow_unannotated_list) {
            return resolve({ status: 'needs_confirmation', tool: 'transition_ticket', profile: { id: request.id } })
          }
          if (request.server.transport === 'http' && request.server.url.includes('oauth') && !signedIn.includes(request.id)) {
            return resolve({ status: 'sign_in_required' })
          }
          resolve({
            status: 'ok',
            profile: {
              id: request.id,
              name: request.name,
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
    signIn: (id) =>
      new Promise((resolve) =>
        setTimeout(() => {
          load()
          signedIn = [...signedIn, id]
          persist()
          resolve()
        }, 300),
      ),
    onChanged: (cb) => (changedListeners.add(cb), () => changedListeners.delete(cb)),
    onQuickAdd: (cb) => (quickAddListeners.add(cb), () => quickAddListeners.delete(cb)),
  }
}
