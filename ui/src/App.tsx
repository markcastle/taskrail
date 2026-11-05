import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react'
import {
  DndContext,
  KeyboardSensor,
  PointerSensor,
  closestCorners,
  useDroppable,
  useSensor,
  useSensors,
  type DragEndEvent,
} from '@dnd-kit/core'
import { SortableContext, sortableKeyboardCoordinates, useSortable, verticalListSortingStrategy } from '@dnd-kit/sortable'
import { CSS } from '@dnd-kit/utilities'
import {
  createBackend,
  type ConnectorSummary,
  type DiscoveryResult,
  type Item,
  type ItemKind,
  type ModelSettings,
  type Provider,
  type RailBackend,
  type RailSettings,
  type RailView,
  type ServerSpec,
  type Status,
  type SyncError,
} from './backend'
import { dueLabel, formatTime, relativeStart } from './format'
import { useSwipe } from './useSwipe'

const backend: RailBackend = createBackend()

const SECTION_DROP_PREFIX = 'section:'

export default function App() {
  const [view, setView] = useState<RailView | null>(null)
  const [settings, setSettings] = useState<RailSettings | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [expandedTop, setExpandedTop] = useState(false)
  const [openCard, setOpenCard] = useState<string | null>(null)
  const [quickAddOpen, setQuickAddOpen] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)

  const refresh = useCallback(() => {
    backend
      .view(expandedTop)
      .then(setView)
      .catch((e) => setError(String(e)))
  }, [expandedTop])

  useEffect(() => {
    refresh()
    backend.getSettings().then(setSettings).catch((e) => setError(String(e)))
  }, [refresh])

  useEffect(() => backend.onChanged(refresh), [refresh])
  useEffect(
    () =>
      backend.onSyncError((e: SyncError) => {
        setNotice(`${e.connector}: ${e.message}`)
      }),
    [],
  )
  useEffect(() => {
    if (!notice) return
    const t = window.setTimeout(() => setNotice(null), 8000)
    return () => window.clearTimeout(t)
  }, [notice])
  useEffect(
    () =>
      backend.onQuickAdd(() => {
        setSettingsOpen(false)
        setQuickAddOpen(true)
      }),
    [],
  )

  // Every action collapses the open card: the card has usually just moved,
  // and a details panel following it around reads as a glitch.
  const act = useCallback(
    (work: () => Promise<unknown>) => {
      setOpenCard(null)
      work().then(refresh).catch((e) => setError(String(e)))
    },
    [refresh],
  )

  // Direction decides the gesture, not timing: eight pixels up or down picks
  // the card up, but twelve pixels sideways first hands it to the flick.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: { y: 8 }, tolerance: { x: 12 } } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  const topTasks = useMemo(() => view?.top.filter((i) => i.kind === 'task') ?? [], [view])
  const inProgress = view?.in_progress ?? []

  const onDragEnd = (event: DragEndEvent) => {
    const { active, over } = event
    if (!over || !view) return
    const id = String(active.id)
    const overId = String(over.id)
    let status: Status
    let index: number
    if (overId.startsWith(SECTION_DROP_PREFIX)) {
      status = overId.slice(SECTION_DROP_PREFIX.length) as Status
      index = status === 'todo' ? topTasks.length : inProgress.length
    } else {
      const inTop = topTasks.findIndex((i) => i.id === overId)
      if (inTop >= 0) {
        status = 'todo'
        index = inTop
      } else {
        const inProg = inProgress.findIndex((i) => i.id === overId)
        if (inProg < 0) return
        status = 'in_progress'
        index = inProg
      }
    }
    if (overId === id) return
    act(() => backend.moveTask(id, status, index))
  }

  if (error) {
    return (
      <div className="rail rail--empty">
        <p>Could not load the rail.</p>
        <pre>{error}</pre>
        <button onClick={() => (setError(null), refresh())}>Try again</button>
      </div>
    )
  }
  if (!view) return <div className="rail rail--empty">Loading…</div>

  const cardProps = (item: Item) => ({
    item,
    open: openCard === item.id,
    active: view.active === item.id,
    onToggle: () => setOpenCard((cur) => (cur === item.id ? null : item.id)),
    onStart: () => act(() => backend.start(item.id)),
    onComplete: () => act(() => backend.complete(item.id)),
    onReopen: () => act(() => backend.reopen(item.id)),
    onBackToTodo: () => act(() => backend.moveTask(item.id, 'todo', 0)),
    onSetActive: () => act(() => backend.setActive(view.active === item.id ? null : item.id)),
    onRemove: () => act(() => backend.remove(item.id)),
  })

  return (
    <div className="rail">
      <header className="rail__header">
        <span className="rail__mark" aria-hidden="true" />
        <h1>Task Rail</h1>
        <button
          className="icon-button"
          title="Settings"
          aria-label="Settings"
          aria-pressed={settingsOpen}
          onClick={() => (setQuickAddOpen(false), setSettingsOpen((o) => !o))}
        >
          ⚙
        </button>
        <button
          className="icon-button icon-button--primary"
          title="Add a task (Ctrl+Shift+Space)"
          aria-label="Add a task"
          onClick={() => (setSettingsOpen(false), setQuickAddOpen(true))}
        >
          +
        </button>
      </header>

      {quickAddOpen && (
        <QuickAdd
          onSubmit={(title) => {
            setQuickAddOpen(false)
            act(() => backend.addTask(title))
          }}
          onCancel={() => setQuickAddOpen(false)}
        />
      )}

      {settingsOpen && settings && (
        <SettingsPanel
          settings={settings}
          onSave={(next) => {
            setSettingsOpen(false)
            backend
              .updateSettings(next)
              .then((saved) => {
                setSettings(saved)
                refresh()
              })
              .catch((e) => setError(String(e)))
          }}
          onCancel={() => setSettingsOpen(false)}
          onQuit={() => backend.quit().catch((e) => setError(String(e)))}
          onNotice={setNotice}
        />
      )}

      {notice && (
        <div className="notice" role="status">
          <span>{notice}</span>
          <button className="notice__close" aria-label="Dismiss" onClick={() => setNotice(null)}>
            ×
          </button>
        </div>
      )}

      {view.up_next && <UpNext item={view.up_next} />}

      <DndContext sensors={sensors} collisionDetection={closestCorners} onDragEnd={onDragEnd}>
        <Section title="Top Priorities" tone="priority" count={view.top.length + view.top_hidden} dropId={`${SECTION_DROP_PREFIX}todo`}>
          <SortableContext items={topTasks.map((i) => i.id)} strategy={verticalListSortingStrategy}>
            {view.top.map((item, i) => (
              <Card key={item.id} {...cardProps(item)} faded={!expandedTop && view.top_hidden > 0 && i === view.top.length - 1} />
            ))}
          </SortableContext>
          {view.top_hidden > 0 && (
            <button className="section__more" onClick={() => setExpandedTop(true)}>
              {view.top_hidden} more
            </button>
          )}
          {expandedTop && view.top_hidden === 0 && view.top.length > 0 && (
            <button className="section__more" onClick={() => setExpandedTop(false)}>
              Show fewer
            </button>
          )}
        </Section>

        <Section title="In Progress" tone="progress" count={inProgress.length} dropId={`${SECTION_DROP_PREFIX}in_progress`}>
          <SortableContext items={inProgress.map((i) => i.id)} strategy={verticalListSortingStrategy}>
            {inProgress.map((item) => (
              <Card key={item.id} {...cardProps(item)} />
            ))}
          </SortableContext>
          {inProgress.length === 0 && <div className="section__hint">Pull a card here to start it</div>}
        </Section>
      </DndContext>

      <Section title="Completed" tone="done" count={view.done_today.length} collapsed>
        {view.done_today.map((item) => (
          <Card key={item.id} {...cardProps(item)} />
        ))}
        {view.done_today.length === 0 && <div className="section__hint">Nothing finished yet today</div>}
      </Section>
    </div>
  )
}

function QuickAdd({ onSubmit, onCancel }: { onSubmit: (title: string) => void; onCancel: () => void }) {
  const [title, setTitle] = useState('')
  const input = useRef<HTMLInputElement>(null)
  useEffect(() => input.current?.focus(), [])
  return (
    <form
      className="quickadd"
      onSubmit={(e) => {
        e.preventDefault()
        if (title.trim()) onSubmit(title)
      }}
    >
      <input
        ref={input}
        className="quickadd__input"
        placeholder="What needs doing?"
        aria-label="New task title"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => e.key === 'Escape' && onCancel()}
      />
      <button type="submit" className="quickadd__submit" disabled={!title.trim()}>
        Add
      </button>
    </form>
  )
}

function SettingsPanel({
  settings,
  onSave,
  onCancel,
  onQuit,
  onNotice,
}: {
  settings: RailSettings
  onSave: (next: RailSettings) => void
  onCancel: () => void
  onQuit: () => void
  onNotice: (message: string) => void
}) {
  const [draft, setDraft] = useState<RailSettings>(settings)
  const set = <K extends keyof RailSettings>(key: K, value: RailSettings[K]) => setDraft((d) => ({ ...d, [key]: value }))
  return (
    <form
      className="settings"
      onSubmit={(e) => {
        e.preventDefault()
        onSave(draft)
      }}
    >
      <ModelPanel onNotice={onNotice} />
      <ConnectorsPanel onNotice={onNotice} />
      <fieldset className="settings__row">
        <legend>Dock to</legend>
        <label>
          <input type="radio" name="edge" checked={draft.edge === 'left'} onChange={() => set('edge', 'left')} /> Left
        </label>
        <label>
          <input type="radio" name="edge" checked={draft.edge === 'right'} onChange={() => set('edge', 'right')} /> Right
        </label>
      </fieldset>
      <label className="settings__row">
        <span>Width (px)</span>
        <input type="number" min={240} max={600} step={10} value={draft.width_px} onChange={(e) => set('width_px', Number(e.target.value))} />
      </label>
      <label className="settings__row">
        <span>Top priorities shown</span>
        <input type="number" min={1} max={10} value={draft.top_limit} onChange={(e) => set('top_limit', Number(e.target.value))} />
      </label>
      <label className="settings__row">
        <span>Pull in events starting within (min)</span>
        <input
          type="number"
          min={0}
          max={720}
          step={5}
          value={draft.event_lead_minutes}
          onChange={(e) => set('event_lead_minutes', Number(e.target.value))}
        />
      </label>
      <div className="settings__actions">
        <button type="button" className="button--danger settings__quit" onClick={onQuit}>
          Quit Task Rail
        </button>
        <button type="button" onClick={onCancel}>
          Cancel
        </button>
        <button type="submit" className="button--primary">
          Save
        </button>
      </div>
    </form>
  )
}

const PROVIDERS: { value: Provider; label: string; modelHint: string }[] = [
  { value: 'anthropic', label: 'Anthropic', modelHint: 'claude-opus-5-5 (default)' },
  { value: 'open_router', label: 'OpenRouter', modelHint: 'e.g. anthropic/claude-opus-5.5' },
  { value: 'open_ai', label: 'OpenAI', modelHint: 'e.g. gpt-5' },
]

function ModelPanel({ onNotice }: { onNotice: (message: string) => void }) {
  const [settings, setSettings] = useState<ModelSettings | null>(null)
  const [provider, setProvider] = useState<Provider>('anthropic')
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    backend
      .getModelSettings()
      .then((s) => {
        setSettings(s)
        setProvider(s.provider)
        setModel(s.model ?? '')
      })
      .catch((e) => onNotice(String(e)))
  }, [onNotice])

  const save = () => {
    setBusy(true)
    backend
      .saveModelSettings({ provider, model: model.trim() || null, base_url: null }, apiKey.trim() ? apiKey.trim() : undefined)
      .then((s) => {
        setSettings(s)
        setApiKey('')
        onNotice(s.has_key ? 'Model settings saved' : 'Model settings saved, but no API key yet')
      })
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }
  const test = () => {
    setBusy(true)
    backend
      .testModel()
      .then(onNotice)
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }
  const hint = PROVIDERS.find((p) => p.value === provider)?.modelHint ?? ''

  return (
    <fieldset className="connectors model">
      <legend>Model</legend>
      <div className="connectors__empty">Used once per connector, to work out what a server is. Bring your own key.</div>
      <label className="settings__row">
        <span>Provider</span>
        <select aria-label="Provider" value={provider} onChange={(e) => setProvider(e.target.value as Provider)}>
          {PROVIDERS.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </label>
      <label className="settings__row">
        <span>Model</span>
        <input type="text" aria-label="Model name" placeholder={hint} value={model} onChange={(e) => setModel(e.target.value)} />
      </label>
      <label className="settings__row">
        <span>API key</span>
        <input
          type="password"
          aria-label="API key"
          placeholder={settings?.has_key ? 'saved in your keychain' : 'paste a key'}
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          autoComplete="off"
        />
      </label>
      <div className="connectors__actions">
        <button type="button" disabled={busy} onClick={save}>
          Save model
        </button>
        <button type="button" disabled={busy || !settings?.has_key} onClick={test}>
          Test
        </button>
      </div>
    </fieldset>
  )
}

function ConnectorsPanel({ onNotice }: { onNotice: (message: string) => void }) {
  const [connectors, setConnectors] = useState<ConnectorSummary[]>([])
  const [adding, setAdding] = useState<'none' | 'profile' | 'server'>('none')
  const [profileJson, setProfileJson] = useState('')
  const [busy, setBusy] = useState(false)

  const reload = useCallback(() => {
    backend.listConnectors().then(setConnectors).catch((e) => onNotice(String(e)))
  }, [onNotice])
  useEffect(() => {
    reload()
  }, [reload])

  const run = (work: () => Promise<unknown>, done?: () => void) => {
    setBusy(true)
    work()
      .then(() => {
        done?.()
        reload()
      })
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }

  return (
    <fieldset className="connectors">
      <legend>Connectors</legend>
      {connectors.length === 0 && <div className="connectors__empty">No connectors yet. Your cards are local only.</div>}
      {connectors.map((c) => (
        <div className="connector" key={c.id} data-testid={`connector-${c.id}`}>
          <div className="connector__main">
            <span className="connector__name">{c.name}</span>
            <span className="connector__meta">
              {c.item_count} items
              {c.capabilities.write_status ? ', two-way' : ', read-only'}
              {c.last_error && <span className="connector__error"> · {c.last_error}</span>}
            </span>
          </div>
          <button type="button" disabled={busy} onClick={() => run(() => backend.syncConnector(c.id))} title="Pull now">
            Sync
          </button>
          <button type="button" disabled={busy} className="button--danger" onClick={() => run(() => backend.removeConnector(c.id))}>
            Remove
          </button>
        </div>
      ))}
      <div className="connectors__actions">
        <button type="button" disabled={busy} onClick={() => run(() => backend.addDemoConnector())}>
          Add demo tracker
        </button>
        <button type="button" disabled={busy} onClick={() => setAdding((a) => (a === 'server' ? 'none' : 'server'))} aria-expanded={adding === 'server'}>
          Add server
        </button>
        <button type="button" disabled={busy} onClick={() => setAdding((a) => (a === 'profile' ? 'none' : 'profile'))} aria-expanded={adding === 'profile'}>
          Paste profile
        </button>
      </div>
      {adding === 'server' && (
        <AddServer
          onNotice={onNotice}
          onAdded={() => {
            setAdding('none')
            reload()
          }}
        />
      )}
      {adding === 'profile' && (
        <div className="connectors__add">
          <textarea
            aria-label="Connector profile JSON"
            placeholder="Paste a Connector Profile (JSON)"
            rows={6}
            value={profileJson}
            onChange={(e) => setProfileJson(e.target.value)}
          />
          <button
            type="button"
            className="button--primary"
            disabled={busy || !profileJson.trim()}
            onClick={() =>
              run(
                () => backend.addConnectorProfile(profileJson),
                () => {
                  setProfileJson('')
                  setAdding('none')
                },
              )
            }
          >
            Add connector
          </button>
        </div>
      )}
    </fieldset>
  )
}

/// The discovery flow: describe a server, let the model write a profile,
/// check the sample it pulled, then save.
function AddServer({ onNotice, onAdded }: { onNotice: (message: string) => void; onAdded: () => void }) {
  const [name, setName] = useState('')
  const [transport, setTransport] = useState<'stdio' | 'http'>('http')
  const [url, setUrl] = useState('')
  const [command, setCommand] = useState('')
  const [kind, setKind] = useState<ItemKind>('task')
  const [hint, setHint] = useState('')
  const [allowUnannotated, setAllowUnannotated] = useState(false)
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<DiscoveryResult | null>(null)
  const [profileJson, setProfileJson] = useState('')

  const id = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
  const server: ServerSpec | null =
    transport === 'http'
      ? url.trim()
        ? { transport: 'http', url: url.trim(), bearer_secret: null }
        : null
      : command.trim()
        ? (() => {
            const parts = command.trim().split(/\s+/)
            return { transport: 'stdio', command: parts[0], args: parts.slice(1), env: {} }
          })()
        : null
  const ready = id.length > 0 && server !== null

  const discover = (allow = allowUnannotated) => {
    if (!server) return
    setBusy(true)
    setResult(null)
    backend
      .discoverConnector({ id, name: name.trim(), server, kind, hint, allow_unannotated_list: allow })
      .then((r) => {
        setResult(r)
        if (r.status === 'ok') setProfileJson(JSON.stringify(r.profile, null, 2))
      })
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }
  const add = () => {
    setBusy(true)
    backend
      .addConnectorProfile(profileJson)
      .then(() => {
        onNotice(`Connected ${name.trim()}`)
        onAdded()
      })
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }

  return (
    <div className="connectors__add" data-testid="add-server">
      <label className="settings__row">
        <span>Name</span>
        <input type="text" aria-label="Server name" placeholder="Tuesday" value={name} onChange={(e) => setName(e.target.value)} />
      </label>
      <fieldset className="settings__row">
        <legend>Reach it by</legend>
        <label>
          <input type="radio" name="transport" checked={transport === 'http'} onChange={() => setTransport('http')} /> URL
        </label>
        <label>
          <input type="radio" name="transport" checked={transport === 'stdio'} onChange={() => setTransport('stdio')} /> Command
        </label>
      </fieldset>
      {transport === 'http' ? (
        <input type="url" aria-label="Server URL" placeholder="https://example.com/mcp" value={url} onChange={(e) => setUrl(e.target.value)} />
      ) : (
        <input type="text" aria-label="Server command" placeholder="npx some-mcp-server --flag" value={command} onChange={(e) => setCommand(e.target.value)} />
      )}
      <label className="settings__row">
        <span>It holds</span>
        <select aria-label="Item kind" value={kind} onChange={(e) => setKind(e.target.value as ItemKind)}>
          <option value="task">Tasks</option>
          <option value="event">Calendar events</option>
        </select>
      </label>
      <input type="text" aria-label="Hint for the model" placeholder="Anything the model should know (optional)" value={hint} onChange={(e) => setHint(e.target.value)} />
      <div className="connectors__actions">
        <button type="button" className="button--primary" disabled={busy || !ready} onClick={() => discover()}>
          {busy ? 'Discovering…' : 'Discover'}
        </button>
      </div>
      {result?.status === 'needs_confirmation' && (
        <div className="discovery__confirm">
          The model wants to list items with <code>{result.tool}</code>, which the server does not mark as read-only. Discovery will call it once.
          <div className="connectors__actions">
            <button
              type="button"
              disabled={busy}
              onClick={() => {
                setAllowUnannotated(true)
                discover(true)
              }}
            >
              Allow and continue
            </button>
          </div>
        </div>
      )}
      {result?.status === 'ok' && (
        <div className="discovery" data-testid="discovery-result">
          <div className="discovery__summary">
            Found {result.sample.length} items in {result.rounds} {result.rounds === 1 ? 'round' : 'rounds'}, from {result.tools.length} tools.
            {result.notes.length > 0 && <span className="connector__meta"> Earlier tries: {result.notes.join('; ')}</span>}
          </div>
          <ul className="discovery__sample">
            {result.sample.slice(0, 5).map((item) => (
              <li key={item.external_id}>
                <span>{item.title}</span>
                {item.status && <span className="connector__meta"> · {item.status}</span>}
              </li>
            ))}
            {result.sample.length > 5 && <li className="connector__meta">and {result.sample.length - 5} more</li>}
          </ul>
          <textarea aria-label="Discovered profile JSON" rows={6} value={profileJson} onChange={(e) => setProfileJson(e.target.value)} />
          <div className="connectors__actions">
            <button type="button" className="button--primary" disabled={busy || !profileJson.trim()} onClick={add}>
              Add connector
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

function UpNext({ item }: { item: Item }) {
  return (
    <div className="upnext" data-testid="upnext">
      <span className="upnext__label">Up next</span>
      <span className="upnext__title">{item.title}</span>
      {item.window && <time className="upnext__time">{relativeStart(item.window.start)}</time>}
    </div>
  )
}

function Section({
  title,
  tone,
  count,
  collapsed = false,
  dropId,
  children,
}: {
  title: string
  tone: 'priority' | 'progress' | 'done'
  count: number
  collapsed?: boolean
  dropId?: string
  children: ReactNode
}) {
  const [open, setOpen] = useState(!collapsed)
  const { setNodeRef, isOver } = useDroppable({ id: dropId ?? `inert:${title}`, disabled: !dropId })
  return (
    <section className={`section section--${tone}${isOver ? ' section--over' : ''}`} data-testid={`section-${tone}`}>
      <button className="section__header" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <span className="section__bar" aria-hidden="true" />
        <span className="section__title">{title}</span>
        <span className="section__count">{count}</span>
      </button>
      {open && (
        <div className="section__cards" ref={setNodeRef}>
          {children}
        </div>
      )}
    </section>
  )
}

interface CardProps {
  item: Item
  open: boolean
  active: boolean
  faded?: boolean
  onToggle: () => void
  onStart: () => void
  onComplete: () => void
  onReopen: () => void
  onBackToTodo: () => void
  onSetActive: () => void
  onRemove: () => void
}

function Card(props: CardProps) {
  const { item, open, active, faded = false, onToggle } = props
  const isTask = item.kind === 'task'
  const sortable = isTask && item.status !== 'done'
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.id, disabled: !sortable })

  const swipe = useSwipe(isTask && item.status !== 'done' && !isDragging, () => props.onComplete(), onToggle)

  const style: CSSProperties = {
    transform: CSS.Transform.toString(transform),
    transition,
  }
  const bodyStyle: CSSProperties = swipe.leaving
    ? { transform: `translateX(${swipe.leaving === 'right' ? 420 : -420}px)`, opacity: 0, transition: 'transform 180ms ease-in, opacity 180ms ease-in' }
    : swipe.dx !== 0
      ? { transform: `translateX(${swipe.dx}px)`, transition: 'none' }
      : {}

  const classes = ['card', `card--${item.status}`, `card--${item.kind}`]
  if (active) classes.push('card--active')
  if (faded) classes.push('card--faded')
  if (open) classes.push('card--open')
  if (isDragging) classes.push('card--dragging')
  if (Math.abs(swipe.dx) > 60 || swipe.leaving) classes.push('card--will-complete')

  return (
    <div ref={setNodeRef} style={style} className={classes.join(' ')} data-testid={`card-${item.id}`} {...attributes} {...listeners}>
      <div className="card__body" style={bodyStyle} {...swipe.handlers}>
        <span className="card__mark" aria-hidden="true" />
        <span className="card__title">{item.title}</span>
        {item.origin && <span className="card__source" title={item.origin.connector}>{item.origin.connector}</span>}
        {active && <span className="card__active-tag">Active</span>}
        {item.window ? (
          <time className="card__meta">{formatTime(item.window.start)}</time>
        ) : (
          item.priority !== 'normal' && <span className={`card__badge card__badge--${item.priority}`}>{item.priority}</span>
        )}
      </div>
      {open && <CardDetails {...props} />}
    </div>
  )
}

function CardDetails(props: CardProps) {
  const { item, active } = props
  const source = item.origin ? item.origin.connector : 'Local'
  return (
    <div className="card__details" onPointerDown={(e) => e.stopPropagation()}>
      <dl className="card__facts">
        <div>
          <dt>Source</dt>
          <dd>{item.origin?.url ? <a href={item.origin.url} target="_blank" rel="noreferrer">{source}</a> : source}</dd>
        </div>
        {item.due && (
          <div>
            <dt>Due</dt>
            <dd>{dueLabel(item.due).replace(/^Due /, '')}</dd>
          </div>
        )}
        {item.window && (
          <div>
            <dt>When</dt>
            <dd>
              {formatTime(item.window.start)} to {formatTime(item.window.end)}
            </dd>
          </div>
        )}
        {item.kind === 'task' && (
          <div>
            <dt>Priority</dt>
            <dd>{item.priority}</dd>
          </div>
        )}
      </dl>
      {item.kind === 'task' && (
        <div className="card__actions">
          {item.status === 'todo' && (
            <>
              <button onClick={props.onStart}>Start</button>
              <button onClick={props.onComplete}>Complete</button>
            </>
          )}
          {item.status === 'in_progress' && (
            <>
              <button onClick={props.onSetActive} className={active ? 'button--primary' : ''}>
                {active ? 'Clear active' : 'Make active'}
              </button>
              <button onClick={props.onComplete}>Complete</button>
              <button onClick={props.onBackToTodo}>Back to To Do</button>
            </>
          )}
          {item.status === 'done' && <button onClick={props.onReopen}>Reopen</button>}
          <button onClick={props.onRemove} className="button--danger">
            Delete
          </button>
        </div>
      )}
    </div>
  )
}
