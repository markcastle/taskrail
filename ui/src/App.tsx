import { useEffect, useState, type ReactNode } from 'react'
import { loadRailView, setActive, type Item, type RailView } from './rail'

export default function App() {
  const [view, setView] = useState<RailView | null>(null)
  const [error, setError] = useState<string | null>(null)

  const refresh = () => loadRailView().then(setView).catch((e) => setError(String(e)))
  useEffect(() => {
    refresh()
  }, [])

  if (error) return <div className="rail rail--empty">Could not load the rail: {error}</div>
  if (!view) return <div className="rail rail--empty">Loading…</div>

  const activate = async (item: Item) => {
    await setActive(item.id)
    refresh()
  }

  return (
    <div className="rail">
      <header className="rail__header">
        <span className="rail__mark" aria-hidden="true" />
        <h1>Task Rail</h1>
        <button className="rail__add" title="Add a task" aria-label="Add a task">
          +
        </button>
      </header>

      {view.up_next && <UpNext item={view.up_next} />}

      <Section title="Top Priorities" tone="priority" count={view.top.length + view.top_hidden}>
        {view.top.map((item, i) => (
          <Card key={item.id} item={item} faded={view.top_hidden > 0 && i === view.top.length - 1} />
        ))}
        {view.top_hidden > 0 && <div className="section__more">{view.top_hidden} more</div>}
      </Section>

      <Section title="In Progress" tone="progress" count={view.in_progress.length}>
        {view.in_progress.map((item) => (
          <Card key={item.id} item={item} active={view.active === item.id} onClick={() => activate(item)} />
        ))}
      </Section>

      <Section title="Completed" tone="done" count={view.done_today.length} collapsed>
        {view.done_today.map((item) => (
          <Card key={item.id} item={item} />
        ))}
      </Section>
    </div>
  )
}

function UpNext({ item }: { item: Item }) {
  const start = item.window ? new Date(item.window.start) : null
  return (
    <div className="upnext">
      <span className="upnext__label">Up next</span>
      <span className="upnext__title">{item.title}</span>
      {start && <time className="upnext__time">{formatTime(start)}</time>}
    </div>
  )
}

function Section({
  title,
  tone,
  count,
  collapsed = false,
  children,
}: {
  title: string
  tone: 'priority' | 'progress' | 'done'
  count: number
  collapsed?: boolean
  children: ReactNode
}) {
  const [open, setOpen] = useState(!collapsed)
  return (
    <section className={`section section--${tone}`}>
      <button className="section__header" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <span className="section__bar" aria-hidden="true" />
        <span className="section__title">{title}</span>
        <span className="section__count">{count}</span>
      </button>
      {open && <div className="section__cards">{children}</div>}
    </section>
  )
}

function Card({
  item,
  active = false,
  faded = false,
  onClick,
}: {
  item: Item
  active?: boolean
  faded?: boolean
  onClick?: () => void
}) {
  const classes = ['card', `card--${item.status}`, `card--${item.kind}`]
  if (active) classes.push('card--active')
  if (faded) classes.push('card--faded')
  const start = item.window ? new Date(item.window.start) : null
  return (
    <div className={classes.join(' ')} onClick={onClick} role={onClick ? 'button' : undefined}>
      <span className="card__mark" aria-hidden="true" />
      <span className="card__title">{item.title}</span>
      {start ? (
        <time className="card__meta">{formatTime(start)}</time>
      ) : (
        item.priority !== 'normal' && <span className={`card__badge card__badge--${item.priority}`}>{item.priority}</span>
      )}
    </div>
  )
}

function formatTime(date: Date): string {
  return date.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })
}
