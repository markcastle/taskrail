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
import { createBackend, type Item, type RailBackend, type RailView, type Status, type SyncError } from './backend'
import { dueLabel, formatTime, relativeStart } from './format'
import Settings from './Settings'
import { followTheme } from './theme'
import { useSwipe } from './useSwipe'

const backend: RailBackend = createBackend()

const SECTION_DROP_PREFIX = 'section:'

export default function App() {
  const [view, setView] = useState<RailView | null>(null)
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
  }, [refresh])

  useEffect(() => backend.onChanged(refresh), [refresh])
  useEffect(() => followTheme(backend), [])
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
  useEffect(() => backend.onQuickAdd(() => setQuickAddOpen(true)), [])
  useEffect(() => backend.onOpenSettings(() => setSettingsOpen(true)), [])

  // Settings live inside the rail, which widens while they are open.
  const openSettings = () => {
    setSettingsOpen(true)
    backend.expandRail(true).catch((e) => setNotice(String(e)))
  }
  const closeSettings = () => {
    setSettingsOpen(false)
    backend.expandRail(false).catch((e) => setNotice(String(e)))
  }
  useEffect(() => {
    if (settingsOpen) backend.expandRail(true).catch((e) => setNotice(String(e)))
  }, [settingsOpen])

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
  if (settingsOpen) return <Settings onClose={closeSettings} />

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
        <img className="rail__mark" src="/icon.png" alt="" width={26} height={26} />
        <h1>Task Rail</h1>
        <button
          className="icon-button"
          title="Settings"
          aria-label="Settings"
          onClick={openSettings}
        >
          ⚙
        </button>
        <button className="icon-button icon-button--primary" title="Add a task (Ctrl+Shift+Space)" aria-label="Add a task" onClick={() => setQuickAddOpen(true)}>
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

      {notice && (
        <div className="toast toast--error toast--inline" role="alert">
          <span>{notice}</span>
          <button className="toast__close" aria-label="Dismiss" onClick={() => setNotice(null)}>
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

  const classes = ['card', `card--${item.status}`, `card--${item.kind}`, `card--accent-${accentFor(item)}`]
  if (active) classes.push('card--active')
  if (faded) classes.push('card--faded')
  if (open) classes.push('card--open')
  if (isDragging) classes.push('card--dragging')
  if (Math.abs(swipe.dx) > 60 || swipe.leaving) classes.push('card--will-complete')

  const chips: ReactNode[] = []
  if (active) chips.push(<span key="active" className="card__active-tag">Active</span>)
  if (item.window) chips.push(<time key="time" className="card__meta">{formatTime(item.window.start)}</time>)
  if (!item.window && item.priority !== 'normal') chips.push(<span key="prio" className={`card__badge card__badge--${item.priority}`}>{item.priority}</span>)
  if (item.due && !item.window) chips.push(<span key="due" className="card__meta">{dueLabel(item.due)}</span>)
  if (item.origin) chips.push(<span key="src" className="card__source" title={item.origin.connector}>{item.origin.connector}</span>)

  return (
    <div ref={setNodeRef} style={style} className={classes.join(' ')} data-testid={`card-${item.id}`} {...attributes} {...listeners}>
      <div className="card__body" style={bodyStyle} {...swipe.handlers}>
        <span className="card__mark" aria-hidden="true" />
        <div className="card__text">
          <span className="card__title">{item.title}</span>
          {chips.length > 0 && <span className="card__chips">{chips}</span>}
        </div>
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

/** The colour of a card's left bar: what it is, then how urgent it is. */
function accentFor(item: Item): 'red' | 'orange' | 'blue' | 'green' | 'purple' | 'slate' {
  if (item.kind === 'event') return 'purple'
  if (item.status === 'done') return 'green'
  if (item.status === 'in_progress') return 'blue'
  if (item.priority === 'highest') return 'red'
  if (item.priority === 'high') return 'orange'
  if (item.priority === 'normal') return 'blue'
  return 'slate'
}
