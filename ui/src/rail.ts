// The shapes the shell sends us, mirroring taskrail-core. If you change a
// type in crates/core/src/domain.rs, change it here too.

export type ItemKind = 'task' | 'event'
export type Status = 'todo' | 'in_progress' | 'done'
export type Priority = 'lowest' | 'low' | 'normal' | 'high' | 'highest'

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

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export async function loadRailView(): Promise<RailView> {
  if (inTauri) {
    const { invoke } = await import('@tauri-apps/api/core')
    return invoke<RailView>('rail_view')
  }
  return sampleView()
}

export async function setActive(id: string | null): Promise<void> {
  if (inTauri) {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('set_active', { id })
  }
}

// What you see when running the UI in a plain browser with `npm run ui:dev`.
function sampleView(): RailView {
  const now = Date.now()
  const minutes = (n: number) => new Date(now + n * 60_000).toISOString()
  const task = (id: string, title: string, status: Status, priority: Priority = 'normal'): Item => ({
    id,
    kind: 'task',
    title,
    status,
    priority,
    rank: 0,
    origin: null,
    window: null,
    due: null,
    completed_at: status === 'done' ? minutes(-120) : null,
    updated_at: minutes(0),
  })
  const meeting: Item = {
    ...task('q1-board-meeting', 'Q1 board meeting', 'todo'),
    kind: 'event',
    window: { start: minutes(40), end: minutes(100) },
  }
  return {
    up_next: meeting,
    top: [
      meeting,
      task('prepare-board-deck', 'Prepare board deck', 'todo', 'highest'),
      task('reply-to-supplier', 'Reply to supplier', 'todo', 'high'),
      task('book-venue', 'Book venue', 'todo'),
      task('update-docs', 'Update docs', 'todo', 'low'),
    ],
    top_hidden: 3,
    in_progress: [
      task('design-dashboard', 'Design dashboard', 'in_progress'),
      task('review-pull-request', 'Review pull request', 'in_progress'),
    ],
    active: 'design-dashboard',
    done_today: [task('triage-inbox', 'Triage inbox', 'done'), task('update-project-plan', 'Update project plan', 'done')],
  }
}
