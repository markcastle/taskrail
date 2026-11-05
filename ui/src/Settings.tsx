import { useCallback, useEffect, useState, type ReactNode } from 'react'
import {
  createBackend,
  type ConnectorSummary,
  type DiscoveryResult,
  type ItemKind,
  type ModelSettings,
  type Provider,
  type RailBackend,
  type RailSettings,
  type ServerSpec,
} from './backend'

const backend: RailBackend = createBackend()

type Page = 'rail' | 'model' | 'connectors'

const PAGES: { id: Page; label: string; blurb: string }[] = [
  { id: 'rail', label: 'Rail', blurb: 'Where it sits and how much it shows.' },
  { id: 'connectors', label: 'Connectors', blurb: 'The systems your cards come from.' },
  { id: 'model', label: 'Model', blurb: 'Used once per connector, to work out what a server is.' },
]

export default function Settings() {
  const [page, setPage] = useState<Page>('rail')
  const [notice, setNotice] = useState<string | null>(null)

  useEffect(() => {
    if (!notice) return
    const t = window.setTimeout(() => setNotice(null), 8000)
    return () => window.clearTimeout(t)
  }, [notice])

  const current = PAGES.find((p) => p.id === page)!
  return (
    <div className="settings-window">
      <nav className="settings-nav">
        <div className="settings-nav__brand">
          <span className="rail__mark" aria-hidden="true" />
          <span>Task Rail</span>
        </div>
        {PAGES.map((p) => (
          <button key={p.id} className={`settings-nav__item${p.id === page ? ' settings-nav__item--current' : ''}`} onClick={() => setPage(p.id)} aria-current={p.id === page}>
            {p.label}
          </button>
        ))}
        <div className="settings-nav__spacer" />
        <button className="settings-nav__item settings-nav__item--danger" onClick={() => backend.quit().catch((e) => setNotice(String(e)))}>
          Quit Task Rail
        </button>
      </nav>
      <main className="settings-main">
        <header className="settings-main__header">
          <h1>{current.label}</h1>
          <p>{current.blurb}</p>
        </header>
        {notice && (
          <div className="notice" role="status">
            <span>{notice}</span>
            <button className="notice__close" aria-label="Dismiss" onClick={() => setNotice(null)}>
              ×
            </button>
          </div>
        )}
        {page === 'rail' && <RailPage onNotice={setNotice} />}
        {page === 'model' && <ModelPage onNotice={setNotice} />}
        {page === 'connectors' && <ConnectorsPage onNotice={setNotice} />}
      </main>
    </div>
  )
}

function Field({ label, help, children }: { label: string; help?: string; children: ReactNode }) {
  return (
    <label className="field">
      <span className="field__label">{label}</span>
      {children}
      {help && <span className="field__help">{help}</span>}
    </label>
  )
}

function RailPage({ onNotice }: { onNotice: (m: string) => void }) {
  const [draft, setDraft] = useState<RailSettings | null>(null)
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    backend.getSettings().then(setDraft).catch((e) => onNotice(String(e)))
  }, [onNotice])
  if (!draft) return <p className="settings-muted">Loading…</p>
  const set = <K extends keyof RailSettings>(key: K, value: RailSettings[K]) => setDraft((d) => (d ? { ...d, [key]: value } : d))
  const save = () => {
    setBusy(true)
    backend
      .updateSettings(draft)
      .then((saved) => {
        setDraft(saved)
        onNotice('Rail settings saved')
      })
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }
  return (
    <form
      className="settings-form"
      onSubmit={(e) => {
        e.preventDefault()
        save()
      }}
    >
      <fieldset className="field">
        <legend className="field__label">Dock to</legend>
        <div className="field__choices">
          <label>
            <input type="radio" name="edge" checked={draft.edge === 'left'} onChange={() => set('edge', 'left')} /> Left edge
          </label>
          <label>
            <input type="radio" name="edge" checked={draft.edge === 'right'} onChange={() => set('edge', 'right')} /> Right edge
          </label>
        </div>
      </fieldset>
      <Field label="Width" help="Pixels. The desktop work area shrinks by this much on Windows.">
        <input type="number" min={240} max={600} step={10} value={draft.width_px} onChange={(e) => set('width_px', Number(e.target.value))} />
      </Field>
      <Field label="Top priorities shown" help="The rest sit behind the fold. Ten at most.">
        <input type="number" min={1} max={10} value={draft.top_limit} onChange={(e) => set('top_limit', Number(e.target.value))} />
      </Field>
      <Field label="Pull in events starting within" help="Minutes. A meeting this close joins Top Priorities.">
        <input type="number" min={0} max={720} step={5} value={draft.event_lead_minutes} onChange={(e) => set('event_lead_minutes', Number(e.target.value))} />
      </Field>
      <div className="settings-actions">
        <button type="submit" className="button--primary" disabled={busy}>
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

function ModelPage({ onNotice }: { onNotice: (m: string) => void }) {
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
    <form
      className="settings-form"
      onSubmit={(e) => {
        e.preventDefault()
        save()
      }}
    >
      <p className="settings-muted">
        The model is only used when you add a server: it reads the server's tool list and writes the connector profile. Syncing never involves it. Bring your own
        key; it is kept in your operating system's credential store.
      </p>
      <Field label="Provider">
        <select aria-label="Provider" value={provider} onChange={(e) => setProvider(e.target.value as Provider)}>
          {PROVIDERS.map((p) => (
            <option key={p.value} value={p.value}>
              {p.label}
            </option>
          ))}
        </select>
      </Field>
      <Field label="Model" help={provider === 'anthropic' ? 'Leave blank for the default.' : 'Required for this provider.'}>
        <input type="text" aria-label="Model name" placeholder={hint} value={model} onChange={(e) => setModel(e.target.value)} />
      </Field>
      <Field label="API key" help={settings?.has_key ? 'A key is saved. Paste a new one to replace it.' : 'No key saved yet.'}>
        <input type="password" aria-label="API key" placeholder={settings?.has_key ? '••••••••' : 'paste a key'} value={apiKey} onChange={(e) => setApiKey(e.target.value)} autoComplete="off" />
      </Field>
      <div className="settings-actions">
        <button type="submit" className="button--primary" disabled={busy}>
          Save model
        </button>
        <button type="button" disabled={busy || !settings?.has_key} onClick={test}>
          Test
        </button>
      </div>
    </form>
  )
}

function ConnectorsPage({ onNotice }: { onNotice: (m: string) => void }) {
  const [connectors, setConnectors] = useState<ConnectorSummary[]>([])
  const [mode, setMode] = useState<'list' | 'server' | 'profile'>('list')
  const [profileJson, setProfileJson] = useState('')
  const [busy, setBusy] = useState(false)

  const reload = useCallback(() => {
    backend.listConnectors().then(setConnectors).catch((e) => onNotice(String(e)))
  }, [onNotice])
  useEffect(() => {
    reload()
  }, [reload])
  useEffect(() => backend.onChanged(reload), [reload])

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
    <div className="settings-form">
      <div className="connector-list">
        {connectors.length === 0 && <p className="settings-muted">No connectors yet. Your cards are local only.</p>}
        {connectors.map((c) => (
          <div className="connector" key={c.id} data-testid={`connector-${c.id}`}>
            <div className="connector__main">
              <span className="connector__name">{c.name}</span>
              <span className="connector__meta">
                {c.server} · {c.item_count} items · {c.capabilities.write_status ? 'two-way' : 'read-only'} · every {c.poll_seconds}s
              </span>
              {c.last_error && <span className="connector__error">{c.last_error}</span>}
            </div>
            <button type="button" disabled={busy} onClick={() => run(() => backend.syncConnector(c.id))} title="Pull now">
              Sync
            </button>
            <button type="button" disabled={busy} className="button--danger" onClick={() => run(() => backend.removeConnector(c.id))}>
              Remove
            </button>
          </div>
        ))}
      </div>
      <div className="settings-actions">
        <button type="button" className="button--primary" disabled={busy} onClick={() => setMode(mode === 'server' ? 'list' : 'server')} aria-expanded={mode === 'server'}>
          Add server
        </button>
        <button type="button" disabled={busy} onClick={() => run(() => backend.addDemoConnector())}>
          Add demo tracker
        </button>
        <button type="button" disabled={busy} onClick={() => setMode(mode === 'profile' ? 'list' : 'profile')} aria-expanded={mode === 'profile'}>
          Paste profile
        </button>
      </div>
      {mode === 'server' && (
        <AddServer
          onNotice={onNotice}
          onAdded={() => {
            setMode('list')
            reload()
          }}
        />
      )}
      {mode === 'profile' && (
        <div className="panel">
          <textarea aria-label="Connector profile JSON" placeholder="Paste a Connector Profile (JSON)" rows={10} value={profileJson} onChange={(e) => setProfileJson(e.target.value)} />
          <div className="settings-actions">
            <button
              type="button"
              className="button--primary"
              disabled={busy || !profileJson.trim()}
              onClick={() =>
                run(
                  () => backend.addConnectorProfile(profileJson),
                  () => {
                    setProfileJson('')
                    setMode('list')
                  },
                )
              }
            >
              Add connector
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

/** Describe a server, let the model write a profile, check the sample it pulled, then save. */
function AddServer({ onNotice, onAdded }: { onNotice: (message: string) => void; onAdded: () => void }) {
  const [name, setName] = useState('')
  const [transport, setTransport] = useState<'stdio' | 'http'>('http')
  const [url, setUrl] = useState('')
  const [command, setCommand] = useState('')
  const [kind, setKind] = useState<ItemKind>('task')
  const [hint, setHint] = useState('')
  const [bearer, setBearer] = useState('')
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
      .discoverConnector({ id, name: name.trim(), server, kind, hint, allow_unannotated_list: allow, bearer: bearer.trim() || null })
      .then((r) => {
        setResult(r)
        if (r.status === 'ok') setProfileJson(JSON.stringify(r.profile, null, 2))
      })
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }
  const signIn = () => {
    if (!server || server.transport !== 'http') return
    setBusy(true)
    setResult(null)
    onNotice('Finish signing in in your browser; this window will carry on by itself.')
    backend
      .signIn(id, server.url)
      .then(() => {
        onNotice('Signed in. Discovering…')
        discover()
      })
      .catch((e) => {
        onNotice(String(e))
        setBusy(false)
      })
  }
  const add = () => {
    setBusy(true)
    backend
      .addConnectorProfile(profileJson, bearer.trim() || undefined)
      .then(() => {
        onNotice(`Connected ${name.trim()}`)
        onAdded()
      })
      .catch((e) => onNotice(String(e)))
      .finally(() => setBusy(false))
  }

  return (
    <div className="panel" data-testid="add-server">
      <div className="panel__grid">
        <Field label="Name">
          <input type="text" aria-label="Server name" placeholder="Tuesday" value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
        <Field label="It holds">
          <select aria-label="Item kind" value={kind} onChange={(e) => setKind(e.target.value as ItemKind)}>
            <option value="task">Tasks</option>
            <option value="event">Calendar events</option>
          </select>
        </Field>
      </div>
      <fieldset className="field">
        <legend className="field__label">Reach it by</legend>
        <div className="field__choices">
          <label>
            <input type="radio" name="transport" checked={transport === 'http'} onChange={() => setTransport('http')} /> URL
          </label>
          <label>
            <input type="radio" name="transport" checked={transport === 'stdio'} onChange={() => setTransport('stdio')} /> Command
          </label>
        </div>
      </fieldset>
      {transport === 'http' ? (
        <>
          <Field label="Server URL">
            <input type="url" aria-label="Server URL" placeholder="https://example.com/mcp" value={url} onChange={(e) => setUrl(e.target.value)} />
          </Field>
          <Field label="Bearer token" help="Only if the server wants a static token. Servers with a sign-in flow leave this blank.">
            <input type="password" aria-label="Bearer token" placeholder="optional" value={bearer} onChange={(e) => setBearer(e.target.value)} autoComplete="off" />
          </Field>
        </>
      ) : (
        <Field label="Command" help="Run on this machine; speaks MCP over its standard input and output.">
          <input type="text" aria-label="Server command" placeholder="npx some-mcp-server --flag" value={command} onChange={(e) => setCommand(e.target.value)} />
        </Field>
      )}
      <Field label="Hint for the model" help="Optional. Anything that helps: which project, whose items, what the product is called.">
        <input type="text" aria-label="Hint for the model" placeholder="e.g. only tickets assigned to me in the Platform project" value={hint} onChange={(e) => setHint(e.target.value)} />
      </Field>
      <div className="settings-actions">
        <button type="button" className="button--primary" disabled={busy || !ready} onClick={() => discover()}>
          {busy ? 'Discovering…' : 'Discover'}
        </button>
      </div>
      {result?.status === 'sign_in_required' && (
        <div className="callout callout--info" data-testid="sign-in-required">
          This server wants you to sign in before it will talk. Your browser will open; when you come back, discovery continues on its own.
          <div className="settings-actions">
            <button type="button" className="button--primary" disabled={busy} onClick={signIn}>
              Sign in
            </button>
          </div>
        </div>
      )}
      {result?.status === 'needs_confirmation' && (
        <div className="callout">
          The model wants to list items with <code>{result.tool}</code>, which the server does not mark as read-only. Discovery will call it once.
          <div className="settings-actions">
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
          <p className="discovery__summary">
            Found {result.sample.length} items in {result.rounds} {result.rounds === 1 ? 'round' : 'rounds'}, from {result.tools.length} tools.
            {result.notes.length > 0 && <span className="connector__meta"> Earlier tries: {result.notes.join('; ')}</span>}
          </p>
          <ul className="discovery__sample">
            {result.sample.slice(0, 6).map((item) => (
              <li key={item.external_id}>
                <span>{item.title}</span>
                {item.status && <span className="connector__meta"> · {item.status}</span>}
              </li>
            ))}
            {result.sample.length > 6 && <li className="connector__meta">and {result.sample.length - 6} more</li>}
          </ul>
          <Field label="Profile" help="What the model decided. Edit it if something looks wrong; it is saved as shown.">
            <textarea aria-label="Discovered profile JSON" rows={12} value={profileJson} onChange={(e) => setProfileJson(e.target.value)} />
          </Field>
          <div className="settings-actions">
            <button type="button" className="button--primary" disabled={busy || !profileJson.trim()} onClick={add}>
              Add connector
            </button>
          </div>
        </div>
      )}
    </div>
  )
}
