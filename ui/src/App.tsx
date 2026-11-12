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
import { createBackend, WIDTH_MAX_PX, WIDTH_MIN_PX, type AvailableUpdate, type ConnectorSummary, type Item, type RailBackend, type RailPhase, type RailSettings, type RailView, type Status, type SyncError } from './backend'
import { Segmented } from './controls'
import { agoWord, dueWord, formatTime, relativeStart, shortcutHint, todayLine, whenWord } from './format'
import Settings from './Settings'
import { play, setSoundsEnabled } from './sound'
import { followTheme } from './theme'
import { useSwipe } from './useSwipe'

const ARRIVE_MS = 180

function isField(target: EventTarget | null) {
  return target instanceof HTMLElement && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable)
}

/** Typing in the rail, dragging a row or dragging its edge keeps an auto-hidden rail out. */
function holdsRailOpen() {
  return (
    (document.hasFocus() && isField(document.activeElement)) ||
    document.querySelector('.card--dragging') !== null ||
    document.documentElement.classList.contains('resizing')
  )
}

const backend: RailBackend = createBackend()

const SECTION_DROP_PREFIX = 'section:'

type Scope = 'today' | 'all'

export default function App() {
  const [view, setView] = useState<RailView | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [scope, setScope] = useState<Scope>('today')
  const [query, setQuery] = useState('')
  const [searchOpen, setSearchOpen] = useState(false)
  const [pastDueOnly, setPastDueOnly] = useState(false)
  const [openCard, setOpenCard] = useState<string | null>(null)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [connectors, setConnectors] = useState<ConnectorSummary[]>([])
  const [foldLimit, setFoldLimit] = useState(5)
  const [animations, setAnimations] = useState(true)
  const [update, setUpdate] = useState<AvailableUpdate | null>(null)
  const [installing, setInstalling] = useState(false)
  const [moreProgress, setMoreProgress] = useState(false)
  const [railSettings, setRailSettings] = useState<RailSettings | null>(null)
  // Auto-hide: the phase the shell says the rail is in, and whether the
  // shell wipes the window itself, in which case the page keeps still.
  const [phase, setPhase] = useState<RailPhase>('out')
  const [wipes, setWipes] = useState(false)
  const [arriving, setArriving] = useState(false)
  const addInput = useRef<HTMLInputElement>(null)
  const searchInput = useRef<HTMLInputElement>(null)

  const refresh = useCallback(() => {
    backend
      .view(false)
      .then(setView)
      .catch((e) => setError(String(e)))
    backend
      .listConnectors()
      .then(setConnectors)
      .catch(() => {})
    backend
      .getSettings()
      .then((s) => {
        setFoldLimit(Math.max(1, s.top_limit))
        setAnimations(s.animations)
        setSoundsEnabled(s.sounds)
        setRailSettings(s)
      })
      .catch(() => {})
  }, [])

  useEffect(() => {
    refresh()
  }, [refresh])

  useEffect(() => backend.onChanged(refresh), [refresh])
  // The rail is left open for days. When the day changes, so does the date
  // at the top, what counts as done today, and which events are today's:
  // look twice a minute, and whenever the rail is looked at again.
  const [day, setDay] = useState(() => new Date().toDateString())
  useEffect(() => {
    const check = () => setDay(new Date().toDateString())
    const t = window.setInterval(check, 30_000)
    window.addEventListener('focus', check)
    document.addEventListener('visibilitychange', check)
    return () => {
      window.clearInterval(t)
      window.removeEventListener('focus', check)
      document.removeEventListener('visibilitychange', check)
    }
  }, [])
  useEffect(() => {
    setDay(new Date().toDateString())
  }, [phase])
  useEffect(() => {
    refresh()
  }, [day, refresh])
  useEffect(() => followTheme(backend), [])
  // Motion is off when the person said so, or when the system asks for less of it.
  useEffect(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const apply = () => {
      document.documentElement.dataset.motion = animations && !media.matches ? 'on' : 'off'
    }
    apply()
    media.addEventListener('change', apply)
    return () => media.removeEventListener('change', apply)
  }, [animations])
  useEffect(
    () =>
      backend.onSyncError((e: SyncError) => {
        setNotice(`${e.connector}: ${e.message}`)
        refresh()
      }),
    [refresh],
  )
  useEffect(() => {
    if (!notice) return
    const t = window.setTimeout(() => setNotice(null), 8000)
    return () => window.clearTimeout(t)
  }, [notice])
  useEffect(() => backend.onUpdateAvailable(setUpdate), [])

  // --- auto-hide ---------------------------------------------------------
  // The shell watches the pointer and decides when the rail is out and when
  // it goes back into its strip. The page draws whichever phase it is told,
  // and reports the one thing only it knows: that the rail is in use.
  const autoHide = railSettings?.auto_hide ?? false
  const edge = railSettings?.edge ?? 'right'
  const tucked = phase === 'tucked'
  const tuckedNow = useRef(tucked)
  tuckedNow.current = tucked
  const focusWhenOut = useRef(false)
  useEffect(() => {
    document.documentElement.dataset.edge = edge
  }, [edge])
  useEffect(() => {
    backend
      .railPhase()
      .then((now) => {
        setPhase(now.phase)
        setWipes(now.wipes)
      })
      .catch(() => {})
  }, [autoHide])
  useEffect(
    () =>
      backend.onRailPhase((next) => {
        setPhase((was) => {
          // Coming out of the strip is the one change the page animates itself.
          if (was === 'tucked' && next === 'out') {
            setArriving(true)
            window.setTimeout(() => setArriving(false), ARRIVE_MS + 40)
          }
          return next
        })
      }),
    [],
  )
  // The pin in the header is the Auto-hide switch in Settings, the other way
  // up: pinned is auto-hide off. One setting, two places to reach it.
  const togglePin = () => {
    if (!railSettings) return
    const next = { ...railSettings, auto_hide: !railSettings.auto_hide }
    setRailSettings(next)
    backend.updateSettings(next).catch((e) => setNotice(String(e)))
  }
  // Typing in the rail, dragging a row or dragging its edge keeps it out.
  useEffect(() => {
    if (!autoHide) return
    let last = false
    const report = () => {
      const held = holdsRailOpen()
      if (held === last) return
      last = held
      backend.holdRail(held).catch(() => {})
    }
    const t = window.setInterval(report, 200)
    document.addEventListener('focusin', report)
    document.addEventListener('focusout', report)
    window.addEventListener('blur', report)
    return () => {
      window.clearInterval(t)
      document.removeEventListener('focusin', report)
      document.removeEventListener('focusout', report)
      window.removeEventListener('blur', report)
      backend.holdRail(false).catch(() => {})
    }
  }, [autoHide])
  // Escape in a field clears the field; Escape anywhere else puts the rail away.
  useEffect(() => {
    if (!autoHide) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && !isField(e.target)) backend.revealRail(false).catch(() => {})
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [autoHide])
  // The shortcut brings the rail out itself; the box takes focus once it is drawn.
  useEffect(
    () =>
      backend.onQuickAdd(() => {
        if (tuckedNow.current) focusWhenOut.current = true
        else addInput.current?.focus()
      }),
    [],
  )
  useEffect(() => {
    if (!tucked && focusWhenOut.current) {
      focusWhenOut.current = false
      addInput.current?.focus()
    }
  }, [tucked])

  // --- drag to resize ----------------------------------------------------
  // The inner edge is a handle. The window follows the pointer through a
  // preview that saves nothing; the width is saved once on release, so the
  // slider in Settings shows what was dragged out.
  const onGripDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!railSettings || e.button !== 0) return
    const grip = e.currentTarget
    grip.setPointerCapture(e.pointerId)
    const startX = e.screenX
    const startWidth = railSettings.width_px
    const sign = edge === 'right' ? -1 : 1
    let width = startWidth
    let frame = 0
    const clamp = (w: number) => Math.min(WIDTH_MAX_PX, Math.max(WIDTH_MIN_PX, Math.round(w)))
    const move = (ev: PointerEvent) => {
      width = clamp(startWidth + sign * (ev.screenX - startX))
      if (!frame) {
        frame = window.requestAnimationFrame(() => {
          frame = 0
          backend.previewWidth(width).catch(() => {})
        })
      }
    }
    const up = () => {
      grip.removeEventListener('pointermove', move)
      grip.removeEventListener('pointerup', up)
      grip.removeEventListener('pointercancel', up)
      document.documentElement.classList.remove('resizing')
      window.cancelAnimationFrame(frame)
      frame = 0
      const next = { ...railSettings, width_px: width }
      setRailSettings(next)
      backend.updateSettings(next).catch((err) => setNotice(String(err)))
    }
    document.documentElement.classList.add('resizing')
    grip.addEventListener('pointermove', move)
    grip.addEventListener('pointerup', up)
    grip.addEventListener('pointercancel', up)
  }
  const installUpdate = () => {
    setInstalling(true)
    backend.installUpdate().catch((e) => {
      setInstalling(false)
      setNotice(`Update failed: ${e}`)
    })
  }
  useEffect(() => backend.onOpenSettings(() => setSettingsOpen(true)), [])
  useEffect(() => {
    if (searchOpen) searchInput.current?.focus()
  }, [searchOpen])
  const toggleSearch = (open: boolean) => {
    setSearchOpen(open)
    if (!open) setQuery('')
  }

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

  // Every action collapses the open row: the row has usually just moved,
  // and a details panel following it around reads as a glitch. A refused
  // action is a notice, not a broken rail.
  const act = useCallback(
    (work: () => Promise<unknown>) => {
      setOpenCard(null)
      work()
        .then(refresh)
        .catch((e) => setNotice(String(e).replace(/^Error: /, '')))
    },
    [refresh],
  )

  // Direction decides the gesture, not timing: eight pixels up or down picks
  // the row up, but twelve pixels sideways first hands it to the flick.
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: { y: 8 }, tolerance: { x: 12 } } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  )

  // What each section shows, after the scope and the search have had their say.
  const needle = query.trim().toLowerCase()
  const isOverdue = (item: Item) => item.kind === 'task' && item.status !== 'done' && !!item.due && dueWord(item.due).overdue
  const matches = useCallback(
    (item: Item) => (needle === '' || item.title.toLowerCase().includes(needle)) && (!pastDueOnly || isOverdue(item)),
    [needle, pastDueOnly],
  )
  // Search and the past-due view both look past the fold.
  const filtering = needle !== '' || pastDueOnly
  const showAll = scope === 'all' || filtering
  const priorities = useMemo(() => {
    if (!view) return []
    const list = showAll ? [...view.top, ...view.later] : view.top
    return list.filter(matches)
  }, [view, showAll, matches])
  const later = useMemo(() => (view && !showAll ? view.later : []), [view, showAll])
  const inProgress = useMemo(() => view?.in_progress.filter(matches) ?? [], [view, matches])
  // In progress folds the same way Priorities does, inside its own section.
  const progressFolded = !showAll && !moreProgress && inProgress.length > foldLimit
  const progressShown = progressFolded ? inProgress.slice(0, foldLimit) : inProgress
  const done = useMemo(() => view?.done_today.filter(matches) ?? [], [view, matches])
  const eventsToday = useMemo(() => view?.events_today.filter(matches) ?? [], [view, matches])
  const sortableTodo = useMemo(() => priorities.filter((i) => i.kind === 'task'), [priorities])
  // Rows seen before do not animate in; a new one does. The first view counts
  // as seen at once, so the opening screen is still; later views catch up
  // after the arrival has played.
  const [seen, setSeen] = useState<Set<string> | null>(null)
  useEffect(() => {
    if (!view) return
    const ids = new Set([...view.top, ...view.later, ...view.in_progress, ...view.done_today].map((i) => i.id))
    const t = window.setTimeout(() => setSeen(ids), seen === null ? 0 : 500)
    return () => window.clearTimeout(t)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view])

  const onDragEnd = (event: DragEndEvent) => {
    const { active, over } = event
    if (!over || !view) return
    const id = String(active.id)
    const overId = String(over.id)
    let status: Status
    let index: number
    if (overId.startsWith(SECTION_DROP_PREFIX)) {
      status = overId.slice(SECTION_DROP_PREFIX.length) as Status
      index = status === 'todo' ? sortableTodo.length : inProgress.length
    } else {
      const inTop = sortableTodo.findIndex((i) => i.id === overId)
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

  // The strip an auto-hidden rail leaves at the edge. A click asks for the
  // rail; resting the pointer on the edge does too, and the shell sees that.
  const strip = (
    <button type="button" className="strip" aria-label="Show Task Rail" title="Task Rail" data-testid="strip" onClick={() => backend.revealRail(true).catch(() => {})}>
      <span className="strip__mark" />
    </button>
  )
  // Where the window shrinks to the strip, the strip is all there is to draw.
  // Where the shell shows part of a full-width window instead, the rail stays
  // drawn underneath so it is already there when the wipe uncovers it.
  if (tucked && !wipes) return strip
  if (error) {
    return (
      <div className="rail rail--empty">
        <p>Could not load the rail.</p>
        <pre>{error}</pre>
        <button className="button" onClick={() => (setError(null), refresh())}>
          Try again
        </button>
      </div>
    )
  }
  if (!view) return <div className="rail rail--empty">Loading…</div>
  if (settingsOpen) return <Settings onClose={closeSettings} />

  const connectorNames = new Map(connectors.map((c) => [c.id, c.paused ? `${c.name} · paused` : c.name]))
  const rowProps = (item: Item) => ({
    item,
    names: connectorNames,
    entering: seen !== null && !seen.has(item.id),
    open: openCard === item.id,
    active: view.active === item.id,
    onToggle: () => setOpenCard((cur) => (cur === item.id ? null : item.id)),
    onStart: () => {
      play('start')
      act(() => backend.start(item.id))
    },
    onComplete: () => {
      play('complete')
      act(() => backend.complete(item.id))
    },
    onReopen: () => act(() => backend.reopen(item.id)),
    onBackToTodo: () => act(() => backend.moveTask(item.id, 'todo', 0)),
    onSetActive: () => act(() => backend.setActive(view.active === item.id ? null : item.id)),
    onRemove: () => act(() => backend.remove(item.id)),
  })

  // The day in numbers: every open task plus what was finished today.
  const openTasks = [...view.top, ...view.later, ...view.in_progress].filter((i) => i.kind === 'task')
  const total = openTasks.length + view.done_today.length
  const overdue = openTasks.filter(isOverdue).length
  // The past-due view has nothing to show once those tasks are dealt with.
  if (pastDueOnly && overdue === 0) setPastDueOnly(false)

  // The one thing to do next: what is being focused on, else the next event,
  // else the first thing in progress, else the top priority.
  const activeItem = view.in_progress.find((i) => i.id === view.active) ?? null
  const upNext: Item | null = activeItem ?? view.up_next ?? view.in_progress[0] ?? view.top.find((i) => i.kind === 'task') ?? null

  const railClass = ['rail', arriving && !wipes ? 'rail--arriving' : '', phase === 'leaving' && !wipes ? 'rail--leaving' : ''].filter(Boolean).join(' ')
  return (
    <>
    <div className={railClass}>
      <header className="rail__header">
        <img className="rail__mark" src="/logo.svg" alt="" width={24} height={24} />
        <h1 className="rail__brand">TaskRail</h1>
        <button
          className={`icon-button${autoHide ? '' : ' icon-button--pinned'}`}
          title={autoHide ? 'Hides when not in use. Click to pin it here' : 'Pinned. Click to let the rail hide itself'}
          aria-label="Pin the rail"
          aria-pressed={!autoHide}
          data-testid="pin"
          onClick={togglePin}
        >
          <PinIcon pinned={!autoHide} />
        </button>
        <button className="icon-button" title="Settings" aria-label="Settings" onClick={openSettings}>
          <GearIcon />
        </button>
        <button
          className="icon-button icon-button--primary"
          title={`Add a task (${shortcutHint()})`}
          aria-label="Add a task"
          onClick={() => addInput.current?.focus()}
        >
          <PlusIcon />
        </button>
      </header>

      <div className="hero">
        <span className="hero__date">{todayLine()}</span>
        {overdue > 0 ? (
          <button
            type="button"
            className={`hero__line hero__line--button${pastDueOnly ? ' hero__line--on' : ''}`}
            onClick={() => setPastDueOnly((on) => !on)}
            aria-pressed={pastDueOnly}
            title={pastDueOnly ? 'Show everything' : 'Show only what is past due'}
          >
            {headline(openTasks.length, view.done_today.length, overdue)}
            <ChevronIcon open={pastDueOnly} />
          </button>
        ) : (
          <h2 className="hero__line">{headline(openTasks.length, view.done_today.length, overdue)}</h2>
        )}
        {pastDueOnly && (
          <span className="hero__filter">
            Showing what is past due.{' '}
            <button type="button" className="link" onClick={() => setPastDueOnly(false)}>
              Show everything
            </button>
          </span>
        )}
      </div>

      <div className="toolbar">
        <Segmented<Scope>
          label="Which tasks to show"
          value={scope}
          onChange={setScope}
          options={[
            { value: 'today', label: 'Today' },
            { value: 'all', label: 'All tasks' },
          ]}
        />
        <span className="toolbar__rule" aria-hidden="true" />
        <button
          className={`icon-button icon-button--quiet${searchOpen ? ' icon-button--on' : ''}`}
          title="Search"
          aria-label="Search"
          aria-pressed={searchOpen}
          onClick={() => toggleSearch(!searchOpen)}
        >
          <SearchIcon />
        </button>
      </div>

      {searchOpen && (
        <div className="search">
          <input
            ref={searchInput}
            className="search__input"
            type="search"
            placeholder="Search tasks"
            aria-label="Search tasks"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => e.key === 'Escape' && toggleSearch(false)}
          />
        </div>
      )}

      {update && (
        <div className="update" role="status" data-testid="update-banner">
          <div className="update__text">
            <strong>Task Rail {update.version} is available.</strong>
          </div>
          <div className="update__actions">
            <button className="focus-button" disabled={installing} onClick={installUpdate}>
              {installing ? 'Installing…' : 'Install now'}
            </button>
            <button className="icon-button icon-button--quiet" aria-label="Not now" title="Not now" onClick={() => setUpdate(null)}>
              ×
            </button>
          </div>
        </div>
      )}

      {total > 0 && <DayProgress done={view.done_today.length} total={total} />}

      {notice && (
        <div className="toast toast--error toast--inline" role="alert">
          <span>{notice}</span>
          <button className="toast__close" aria-label="Dismiss" onClick={() => setNotice(null)}>
            ×
          </button>
        </div>
      )}

      {upNext && !filtering && (
        <UpNext
          item={upNext}
          focused={activeItem?.id === upNext.id}
          names={connectorNames}
          onFocus={() =>
            act(async () => {
              if (upNext.status === 'todo') await backend.start(upNext.id)
              await backend.setActive(activeItem?.id === upNext.id ? null : upNext.id)
            })
          }
        />
      )}

      {eventsToday.length > 0 && (
        <Section title="Today" tone="today" count={eventsToday.length}>
          {eventsToday.map((item) => (
            <Row key={item.id} {...rowProps(item)} />
          ))}
        </Section>
      )}

      <DndContext sensors={sensors} collisionDetection={closestCorners} onDragEnd={onDragEnd}>
        <Section title="Priorities" tone="priority" count={priorities.length} dropId={`${SECTION_DROP_PREFIX}todo`}>
          <SortableContext items={sortableTodo.map((i) => i.id)} strategy={verticalListSortingStrategy}>
            {priorities.map((item) => (
              <Row key={item.id} {...rowProps(item)} />
            ))}
          </SortableContext>
          {priorities.length === 0 && <div className="section__hint">{needle ? 'No matches' : 'Nothing waiting'}</div>}
        </Section>

        <Section title="In progress" tone="progress" count={inProgress.length} dropId={`${SECTION_DROP_PREFIX}in_progress`}>
          <SortableContext items={progressShown.map((i) => i.id)} strategy={verticalListSortingStrategy}>
            {progressShown.map((item) => (
              <Row key={item.id} {...rowProps(item)} />
            ))}
          </SortableContext>
          {inProgress.length === 0 && <div className="section__hint">{needle ? 'No matches' : 'Pull a task here to start it'}</div>}
          {progressFolded && (
            <button className="section__more" onClick={() => setMoreProgress(true)}>
              Show {inProgress.length - foldLimit} more
            </button>
          )}
          {moreProgress && !showAll && inProgress.length > foldLimit && (
            <button className="section__more" onClick={() => setMoreProgress(false)}>
              Show fewer
            </button>
          )}
        </Section>
      </DndContext>

      {/* Not "Later": boards have groups called that, and this is only the fold. */}
      {later.length > 0 && (
        <Section title="More priorities" tone="later" count={later.length} collapsed>
          {later.map((item) => (
            <Row key={item.id} {...rowProps(item)} />
          ))}
        </Section>
      )}

      <Section title="Completed" tone="done" count={done.length} collapsed={needle === ''}>
        {done.map((item) => (
          <Row key={item.id} {...rowProps(item)} />
        ))}
        {done.length === 0 && <div className="section__hint">{needle ? 'No matches' : 'Nothing finished yet today'}</div>}
      </Section>

      <div className="rail__bottom">
        <QuickAdd inputRef={addInput} onSubmit={(title) => act(() => backend.addTask(title))} />
        <SyncStatus connectors={connectors} onSynced={refresh} />
      </div>
    </div>
    <div className={`grip grip--${edge}`} role="separator" aria-orientation="vertical" aria-label="Resize the rail" data-testid="grip" onPointerDown={onGripDown} />
    {wipes && phase !== 'out' && strip}
    </>
  )
}

function headline(open: number, done: number, overdue: number): string {
  if (overdue === 1) return 'One task past due.'
  if (overdue > 1) return `${overdue} tasks past due.`
  if (open === 0 && done > 0) return 'All clear. Nice work.'
  if (open === 0) return 'Nothing on the rail yet.'
  return 'Your day, on track.'
}

/** "3 of 8 complete" with one segment per task, or a plain bar when there are too many to count. */
function DayProgress({ done, total }: { done: number; total: number }) {
  const segmented = total <= 12
  return (
    <div className="progress" data-testid="day-progress">
      <span className="progress__label">
        {done} of {total} complete
      </span>
      {segmented ? (
        <div className="progress__bar progress__bar--segments" role="progressbar" aria-valuenow={done} aria-valuemin={0} aria-valuemax={total}>
          {Array.from({ length: total }, (_, i) => (
            <span key={i} className={i < done ? 'progress__seg progress__seg--on' : 'progress__seg'} />
          ))}
        </div>
      ) : (
        <div className="progress__bar" role="progressbar" aria-valuenow={done} aria-valuemin={0} aria-valuemax={total}>
          <span className="progress__fill" style={{ width: `${(done / total) * 100}%` }} />
        </div>
      )}
    </div>
  )
}

function QuickAdd({ inputRef, onSubmit }: { inputRef: React.RefObject<HTMLInputElement | null>; onSubmit: (title: string) => void }) {
  const [title, setTitle] = useState('')
  return (
    <form
      className="quickadd"
      onSubmit={(e) => {
        e.preventDefault()
        if (!title.trim()) return
        onSubmit(title.trim())
        setTitle('')
      }}
    >
      <PlusIcon />
      <input
        ref={inputRef}
        className="quickadd__input"
        placeholder="Add a task…"
        aria-label="New task title"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Escape') {
            setTitle('')
            e.currentTarget.blur()
          }
        }}
      />
      <kbd className="quickadd__hint">{shortcutHint()}</kbd>
    </form>
  )
}

/** A connector is stale when it has gone several of its own intervals without a pull, and at least ten minutes. */
function isStale(c: ConnectorSummary, now: number): boolean {
  if (!c.last_sync) return false
  return now - Date.parse(c.last_sync) > Math.max(c.poll_seconds * 3, 600) * 1000
}

function SyncStatus({ connectors, onSynced }: { connectors: ConnectorSummary[]; onSynced: () => void }) {
  // "4 min ago" has to move on its own, so the footer re-renders twice a minute.
  const [now, setNow] = useState(() => Date.now())
  const [syncing, setSyncing] = useState(false)
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 30_000)
    return () => window.clearInterval(t)
  }, [])
  const running = connectors.filter((c) => !c.paused)
  const failing = running.filter((c) => c.last_error)
  const latest = running.map((c) => c.last_sync).filter((t): t is string => !!t).sort().at(-1)
  const stale = running.filter((c) => isStale(c, now))
  let tone: 'ok' | 'off' | 'error' | 'stale' = 'ok'
  let text = 'Up to date'
  if (connectors.length === 0) {
    tone = 'off'
    text = 'Local tasks only'
  } else if (running.length === 0) {
    tone = 'off'
    text = connectors.length === 1 ? `${connectors[0].name} is paused` : 'All connectors paused'
  } else if (failing.length > 0) {
    tone = 'error'
    text = `${failing[0].name} could not sync`
  } else if (stale.length > 0) {
    // Never "Up to date" over data that is hours old.
    tone = 'stale'
    text = `Last synced ${agoWord(stale.map((c) => c.last_sync!).sort()[0], now)}`
  }
  if (syncing) text = 'Syncing…'
  const syncNow = () => {
    if (syncing || running.length === 0) return
    setSyncing(true)
    Promise.allSettled(running.map((c) => backend.syncConnector(c.id))).finally(() => {
      setSyncing(false)
      onSynced()
    })
  }
  return (
    <button type="button" className={`foot foot--${tone}`} data-testid="sync-status" title={running.length > 0 ? 'Sync now' : undefined} disabled={running.length === 0} onClick={syncNow}>
      <span className="foot__dot" aria-hidden="true" />
      <span className="foot__text">{text}</span>
      {latest && tone === 'ok' && !syncing && <span className="foot__time">Synced {agoWord(latest, now)}</span>}
    </button>
  )
}

function UpNext({ item, focused, names, onFocus }: { item: Item; focused: boolean; names: Map<string, string>; onFocus: () => void }) {
  const isEvent = item.kind === 'event'
  return (
    <section className="upnext" data-testid="upnext">
      <h3 className="section__title section__title--bar">Up next</h3>
      <div key={item.id} className={`upnext__card upnext__card--arrive${focused ? ' upnext__card--focused' : ''}`}>
        <span className="upnext__title">{item.title}</span>
        <div className="upnext__foot">
          <span className="upnext__meta">{metaLine(item, names) ?? statusWord(item)}</span>
          {isEvent ? (
            <span className="upnext__when">{item.window ? relativeStart(item.window.start) : ''}</span>
          ) : (
            <button className={`focus-button${focused ? ' focus-button--on' : ''}`} onClick={onFocus}>
              {focused ? <PauseIcon /> : <PlayIcon />}
              {focused ? 'Focusing' : 'Start focus'}
            </button>
          )}
        </div>
      </div>
    </section>
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
  tone: 'priority' | 'progress' | 'later' | 'done' | 'today'
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
        {collapsed ? <ChevronIcon open={open} /> : <span className="section__bar" aria-hidden="true" />}
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

function statusWord(item: Item): string {
  if (item.kind === 'event') return 'Event'
  if (item.status === 'in_progress') return 'In progress'
  if (item.status === 'done') return 'Done'
  return 'To do'
}

/** "Totum · Due today", "Today, 14:30", "Tuesday · Overdue". */
function metaLine(item: Item, names: Map<string, string>): ReactNode {
  const parts: ReactNode[] = []
  if (item.origin)
    parts.push(
      <span key="src" className="card__source">
        {names.get(item.origin.connector) ?? item.origin.connector}
      </span>,
    )
  if (item.trail && item.trail.length > 0)
    parts.push(
      <span key="trail" className="card__trail" title={item.trail.join(' › ')}>
        {item.trail.join(' › ')}
      </span>,
    )
  if (item.window) parts.push(<span key="when">{whenWord(item.window.start)}</span>)
  else if (item.due) {
    const due = dueWord(item.due)
    parts.push(
      <span key="due" className={due.overdue ? 'meta--overdue' : undefined}>
        {due.text}
      </span>,
    )
  }
  if (item.kind === 'task' && (item.priority === 'high' || item.priority === 'highest')) {
    parts.push(
      <span key="prio" className={`chip chip--${item.priority}`}>
        {item.priority === 'highest' ? 'Urgent' : 'High'}
      </span>,
    )
  }
  if (parts.length === 0) return null
  return parts.flatMap((part, i) => (i === 0 ? [part] : [<span key={`dot${i}`} className="meta__dot">·</span>, part]))
}

interface RowProps {
  item: Item
  /** Connector id to display name, so a row says "Tuesday" rather than "tuesday". */
  names: Map<string, string>
  /** New since the last view: it arrives rather than appears. */
  entering: boolean
  open: boolean
  active: boolean
  onToggle: () => void
  onStart: () => void
  onComplete: () => void
  onReopen: () => void
  onBackToTodo: () => void
  onSetActive: () => void
  onRemove: () => void
}

function Row(props: RowProps) {
  const { item, open, active, onToggle } = props
  const isTask = item.kind === 'task'
  const isDone = item.status === 'done'
  const sortable = isTask && !isDone
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.id, disabled: !sortable })
  const [leaving, setLeaving] = useState(false)

  const swipe = useSwipe(isTask && !isDone && !isDragging, () => props.onComplete(), onToggle)

  // Ticking the circle: the row slides off and the gap closes, then the change is made.
  const completeFromCheck = () => {
    if (document.documentElement.dataset.motion !== 'on') return props.onComplete()
    setLeaving(true)
    window.setTimeout(props.onComplete, 220)
  }

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
  if (open) classes.push('card--open')
  if (isDragging) classes.push('card--dragging')
  if (Math.abs(swipe.dx) > 60 || swipe.leaving) classes.push('card--will-complete')
  if (props.entering) classes.push('card--entering')
  if (leaving || swipe.leaving) classes.push('card--leaving')

  const meta = metaLine(item, props.names)

  return (
    <div
      ref={setNodeRef}
      style={style}
      className={classes.join(' ')}
      data-testid={`card-${item.id}`}
      {...(sortable ? attributes : {})}
      {...(sortable ? listeners : {})}
    >
      <div className="card__body" style={bodyStyle} {...swipe.handlers}>
        {isTask ? (
          <button
            className={`check${isDone ? ' check--on' : ''}`}
            aria-label={isDone ? `Reopen ${item.title}` : `Complete ${item.title}`}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation()
              if (isDone) props.onReopen()
              else completeFromCheck()
            }}
          >
            {isDone && <TickIcon />}
          </button>
        ) : (
          <span className="check check--event" aria-hidden="true">
            <CalendarIcon />
          </span>
        )}
        <div className="card__text">
          <span className="card__title">{item.title}</span>
          {meta && <span className="card__meta">{meta}</span>}
          {active && <span className="card__focus-bar" aria-hidden="true" />}
        </div>
      </div>
      {open && <RowDetails {...props} />}
    </div>
  )
}

function RowDetails(props: RowProps) {
  const { item, active } = props
  const source = item.origin ? (props.names.get(item.origin.connector) ?? item.origin.connector) : 'Local'
  return (
    <div className="card__details" onPointerDown={(e) => e.stopPropagation()}>
      <dl className="card__facts">
        <div>
          <dt>Source</dt>
          <dd>{item.origin?.url ? <a href={item.origin.url} target="_blank" rel="noreferrer">{source}</a> : source}</dd>
        </div>
        {item.trail && item.trail.length > 0 && (
          <div>
            <dt>Where</dt>
            <dd className="card__facts-trail">
              {item.trail.map((step, i) => (
                <span key={i}>
                  {i > 0 && <span className="meta__dot"> › </span>}
                  {step}
                </span>
              ))}
            </dd>
          </div>
        )}
        {item.due && (
          <div>
            <dt>Due</dt>
            <dd>{dueWord(item.due).text.replace(/^Due /, '')}</dd>
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
              <button className="button button--primary" onClick={props.onStart}>
                Start
              </button>
              <button className="button" onClick={props.onComplete}>
                Complete
              </button>
            </>
          )}
          {item.status === 'in_progress' && (
            <>
              <button onClick={props.onSetActive} className={`button${active ? ' button--primary' : ''}`}>
                {active ? 'Clear active' : 'Make active'}
              </button>
              <button className="button" onClick={props.onComplete}>
                Complete
              </button>
              <button className="button" onClick={props.onBackToTodo}>
                Back to To Do
              </button>
            </>
          )}
          {item.status === 'done' && (
            <button className="button" onClick={props.onReopen}>
              Reopen
            </button>
          )}
          <button onClick={props.onRemove} className="button button--danger">
            Delete
          </button>
        </div>
      )}
    </div>
  )
}

/* Icons: one stroke weight, drawn on a 20px grid so they sit level with the text. */

function GearIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
    </svg>
  )
}

/** A pushpin: filled and upright when the rail is pinned, an outline at a tilt when it is free to hide. */
function PinIcon({ pinned }: { pinned: boolean }) {
  return (
    <svg
      width="18"
      height="18"
      viewBox="0 0 24 24"
      fill={pinned ? 'currentColor' : 'none'}
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={`pin${pinned ? '' : ' pin--free'}`}
    >
      <path d="M12 17v5" />
      <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z" />
    </svg>
  )
}

function PlusIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" aria-hidden="true">
      <path d="M12 5v14M5 12h14" />
    </svg>
  )
}

function SearchIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" aria-hidden="true">
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.6-3.6" />
    </svg>
  )
}

function PlayIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M7 4.5v15l12-7.5z" />
    </svg>
  )
}

function PauseIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
      <path d="M6 4h4v16H6zM14 4h4v16h-4z" />
    </svg>
  )
}

function TickIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="3.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="m5 12.5 4.5 4.5L19 7.5" />
    </svg>
  )
}

function CalendarIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" aria-hidden="true">
      <rect x="4" y="5" width="16" height="15" rx="2" />
      <path d="M4 10h16M8 3v4M16 3v4" />
    </svg>
  )
}

function ChevronIcon({ open }: { open: boolean }) {
  return (
    <svg className={`chevron${open ? ' chevron--open' : ''}`} width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="m9 6 6 6-6 6" />
    </svg>
  )
}
