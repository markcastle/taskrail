import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import {
  createBackend,
  type ConnectorSummary,
  type DiscoveryResult,
  type ItemKind,
  type ModelSettings,
  type MonitorInfo,
  type Provider,
  type RailBackend,
  type RailSettings,
  type ServerSpec,
  type Theme,
  type UpdateStatus,
} from './backend'
import { Result, SavedMark, Segmented, Slider, Spinner, Toggle, type Tone } from './controls'
import { useDebouncedSave } from './useDebouncedSave'
import { followTheme } from './theme'

const backend: RailBackend = createBackend()

type Page = 'rail' | 'model' | 'connectors' | 'logs'

const PAGES: { id: Page; label: string; blurb: string }[] = [
  { id: 'rail', label: 'Rail', blurb: 'Where it sits and how much it shows. Changes apply as you make them.' },
  { id: 'connectors', label: 'Connectors', blurb: 'The systems your cards come from.' },
  { id: 'model', label: 'Model', blurb: 'Used once per connector, to work out what a server is.' },
  { id: 'logs', label: 'Logs', blurb: 'What the app has been doing, for when something is wrong.' },
]

export interface Toast {
  tone: Tone
  text: string
}

export default function Settings({ onClose }: { onClose: () => void }) {
  const [page, setPage] = useState<Page>('rail')
  const [toast, setToast] = useState<Toast | null>(null)
  useEffect(() => followTheme(backend), [])
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onClose])

  useEffect(() => {
    if (!toast) return
    const t = window.setTimeout(() => setToast(null), toast.tone === 'error' ? 8000 : 3500)
    return () => window.clearTimeout(t)
  }, [toast])

  const notify = useCallback((tone: Tone, text: string) => setToast({ tone, text }), [])
  const fail = useCallback((e: unknown) => setToast({ tone: 'error', text: String(e) }), [])

  const current = PAGES.find((p) => p.id === page)!
  return (
    <div className="settings-window">
      <nav className="settings-nav">
        <div className="settings-nav__brand">
          <img className="rail__mark" src="/logo.svg" alt="" width={24} height={24} />
          <span>Task Rail</span>
        </div>
        {PAGES.map((p) => (
          <button key={p.id} className={`settings-nav__item${p.id === page ? ' settings-nav__item--current' : ''}`} onClick={() => setPage(p.id)} aria-current={p.id === page}>
            {p.label}
          </button>
        ))}
        <div className="settings-nav__spacer" />
        <button className="settings-nav__item settings-nav__item--close" onClick={onClose}>
          ← Back to the rail
        </button>
        <button className="settings-nav__item settings-nav__item--danger" onClick={() => backend.quit().catch(fail)}>
          Quit Task Rail
        </button>
      </nav>
      <main className="settings-main">
        <header className="settings-main__header">
          <h1>{current.label}</h1>
          <p>{current.blurb}</p>
        </header>
        {page === 'rail' && <RailPage fail={fail} />}
        {page === 'model' && <ModelPage notify={notify} fail={fail} />}
        {page === 'connectors' && <ConnectorsPage notify={notify} fail={fail} />}
        {page === 'logs' && <LogsPage notify={notify} fail={fail} />}
        {toast && (
          <div className={`toast toast--${toast.tone}`} role={toast.tone === 'error' ? 'alert' : 'status'} data-testid="toast">
            <span>{toast.text}</span>
            <button className="toast__close" aria-label="Dismiss" onClick={() => setToast(null)}>
              ×
            </button>
          </div>
        )}
      </main>
    </div>
  )
}

function Row({
  label,
  help,
  children,
  trailing,
  stack = false,
}: {
  label: string
  help?: string
  children: ReactNode
  trailing?: ReactNode
  /** Put the control on its own line under the label, for controls that need the width. */
  stack?: boolean
}) {
  return (
    <div className={stack ? 'row row--stack' : 'row'}>
      <div className="row__text">
        <span className="row__label">{label}</span>
        {help && <span className="row__help">{help}</span>}
      </div>
      <div className="row__control">
        {children}
        {trailing}
      </div>
    </div>
  )
}

/** A connector's profile as a blueprint, to copy into the registry or an issue. */
const SUBMIT_URL = 'https://github.com/markcastle/taskrail/issues/new'

/** The submission form, prefilled. Browsers and GitHub cap the address length, so a big blueprint goes by clipboard instead. */
function submissionLink(json: string): { url: string; prefilled: boolean } {
  let title = 'Blueprint: '
  try {
    title += String(JSON.parse(json).title ?? '')
  } catch {
    /* the title field is just a nicety */
  }
  const params = new URLSearchParams({ template: 'blueprint.yml', title, blueprint: json })
  const url = `${SUBMIT_URL}?${params}`
  if (url.length <= 7000) return { url, prefilled: true }
  return { url: `${SUBMIT_URL}?${new URLSearchParams({ template: 'blueprint.yml', title })}`, prefilled: false }
}

function BlueprintPanel({ id, json, onClose, notify }: { id: string; json: string; onClose: () => void; notify: (tone: Tone, text: string) => void }) {
  const area = useRef<HTMLTextAreaElement>(null)
  const submit = () => {
    const link = submissionLink(json)
    const go = () =>
      backend
        .openProjectPage(link.url)
        .then(() => notify('info', link.prefilled ? 'The submission form is open in your browser' : 'The form is open in your browser. Paste the blueprint into it.'))
        .catch((e) => notify('error', String(e)))
    if (link.prefilled || !navigator.clipboard?.writeText) go()
    else navigator.clipboard.writeText(json).then(go, go)
  }
  const copy = () => {
    const done = () => notify('success', 'Blueprint copied')
    const fallback = () => {
      area.current?.focus()
      area.current?.select()
      notify('info', 'Press Ctrl+C to copy the selected text')
    }
    if (navigator.clipboard?.writeText) navigator.clipboard.writeText(json).then(done, fallback)
    else fallback()
  }
  return (
    <div className="panel panel--form" data-testid={`blueprint-${id}`}>
      <p className="settings-muted">
        This is the connector without your server or secrets, with the tools it calls as the fingerprint. Submit it for review and, once a maintainer merges it, the next person to connect this
        system will not need a model. If it names you, swap your id for <code>{'{me}'}</code> and add an <code>identity</code> block; the registry's README shows how.
      </p>
      <textarea ref={area} aria-label="Blueprint JSON" readOnly rows={14} value={json} />
      <div className="settings-actions settings-actions--end">
        <button type="button" className="button button--quiet" onClick={onClose}>
          Close
        </button>
        <button type="button" className="button" onClick={copy} data-testid="copy-blueprint">
          Copy
        </button>
        <button type="button" className="button button--primary" onClick={submit} data-testid="submit-blueprint">
          Submit for review
        </button>
      </div>
    </div>
  )
}

/* ------------------------------------------------------------------ rail */

function monitorName(m: MonitorInfo, index: number) {
  return m.primary ? 'Main display' : `Display ${index + 1}`
}

/** The displays drawn as little screens, the way the Displays pane does it. The chosen one shows where the rail sits. */
function Displays({ monitors, chosen, edge, onChoose }: { monitors: MonitorInfo[]; chosen: string; edge: 'left' | 'right'; onChoose: (name: string) => void }) {
  return (
    <div className="displays" role="radiogroup" aria-label="Monitor">
      {monitors.map((m, i) => {
        const on = m.name === chosen
        return (
          <button
            key={m.name}
            type="button"
            role="radio"
            aria-checked={on}
            aria-label={`${monitorName(m, i)} · ${m.width}×${m.height}`}
            className={on ? 'display display--on' : 'display'}
            onClick={() => onChoose(m.name)}
          >
            <span className="display__frame" aria-hidden="true">
              <span className="display__screen" style={{ aspectRatio: `${m.width} / ${m.height}` }}>
                {on && <span className={`display__rail display__rail--${edge}`} />}
              </span>
            </span>
            <span className="display__name">{monitorName(m, i)}</span>
            <span className="display__size">
              {m.width}×{m.height}
            </span>
          </button>
        )
      })}
    </div>
  )
}

function RailPage({ fail }: { fail: (e: unknown) => void }) {
  const [draft, setDraft] = useState<RailSettings | null>(null)
  const [monitors, setMonitors] = useState<MonitorInfo[]>([])
  const [savedAt, setSavedAt] = useState<number | null>(null)
  useEffect(() => {
    backend.getSettings().then(setDraft).catch(fail)
    backend.listMonitors().then(setMonitors).catch(() => setMonitors([]))
  }, [fail])
  useDebouncedSave(draft, (value) => {
    backend
      .updateSettings(value)
      .then(() => setSavedAt(Date.now()))
      .catch(fail)
  })
  if (!draft) return <Spinner label="Loading…" />
  const set = <K extends keyof RailSettings>(key: K, value: RailSettings[K]) => setDraft((d) => (d ? { ...d, [key]: value } : d))

  return (
    <section className="group">
      <div className="group__head">
        <SavedMark at={savedAt} />
      </div>
      <Row label="Appearance">
        <Segmented<Theme>
          label="Appearance"
          value={draft.theme}
          onChange={(v) => set('theme', v)}
          options={[
            { value: 'system', label: 'System' },
            { value: 'light', label: 'Light' },
            { value: 'dark', label: 'Dark' },
          ]}
        />
      </Row>
      {monitors.length > 1 && (
        <Row label="Monitor" help="Which display the rail lives on. Unplug it and the rail moves to the main display until it is back." stack>
          <Displays
            monitors={monitors}
            chosen={draft.monitor ?? monitors.find((m) => m.primary)?.name ?? monitors[0].name}
            edge={draft.edge}
            onChoose={(name) => set('monitor', monitors.find((m) => m.name === name)?.primary ? null : name)}
          />
        </Row>
      )}
      <Row label="Dock to" help="Windows shrinks the desktop work area to make room.">
        <Segmented
          label="Dock to"
          value={draft.edge}
          onChange={(v) => set('edge', v)}
          options={[
            { value: 'left', label: 'Left edge' },
            { value: 'right', label: 'Right edge' },
          ]}
        />
      </Row>
      <Row label="Width">
        <Slider label="Width" value={draft.width_px} min={240} max={480} step={10} unit="px" onChange={(v) => set('width_px', v)} />
      </Row>
      <Row label="Top priorities shown" help="The rest wait behind the fold.">
        <Slider label="Top priorities shown" value={draft.top_limit} min={1} max={10} unit={draft.top_limit === 1 ? 'card' : 'cards'} onChange={(v) => set('top_limit', v)} />
      </Row>
      <Row label="Pull in events starting within" help="A meeting this close joins Priorities.">
        <Slider label="Pull in events starting within" value={draft.event_lead_minutes} min={0} max={240} step={5} unit="min" onChange={(v) => set('event_lead_minutes', v)} />
      </Row>
      <Updates autoUpdate={draft.auto_update} onAutoUpdate={(v) => set('auto_update', v)} fail={fail} />
    </section>
  )
}

/** Checking for and installing new releases. Automatic is a switch; otherwise the rail asks. */
function Updates({ autoUpdate, onAutoUpdate, fail }: { autoUpdate: boolean; onAutoUpdate: (on: boolean) => void; fail: (e: unknown) => void }) {
  const [version, setVersion] = useState('')
  const [checking, setChecking] = useState(false)
  const [status, setStatus] = useState<UpdateStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [installing, setInstalling] = useState(false)
  useEffect(() => {
    backend.appVersion().then(setVersion).catch(() => setVersion(''))
  }, [])
  const check = () => {
    setChecking(true)
    setError(null)
    setStatus(null)
    backend
      .checkForUpdate()
      .then(setStatus)
      .catch((e) => setError(String(e)))
      .finally(() => setChecking(false))
  }
  const install = () => {
    setInstalling(true)
    backend.installUpdate().catch((e) => {
      setInstalling(false)
      fail(e)
    })
  }
  return (
    <>
      <div className="settings-subhead">Updates</div>
      <Row label="Update automatically" help="New releases install at a quiet moment. Off, and the rail tells you one is ready instead.">
        <Toggle checked={autoUpdate} onChange={onAutoUpdate} label="Update automatically" />
      </Row>
      <Row label="Version" help={version ? `You have ${version}.` : undefined}>
        <button type="button" className="button" disabled={checking} onClick={check} data-testid="check-update">
          {checking ? <Spinner label="Checking…" /> : 'Check for updates'}
        </button>
      </Row>
      {status && !status.available && (
        <Result tone="success">
          <strong>You're up to date.</strong>
        </Result>
      )}
      {status?.available && (
        <Result tone="info">
          <strong>Task Rail {status.available.version} is available.</strong>{' '}
          <button
            type="button"
            className="link"
            onClick={() => backend.openProjectPage(`https://github.com/markcastle/taskrail/releases/tag/v${status.available?.version}`).catch(fail)}
          >
            Release notes
          </button>
          <div className="settings-actions">
            <button type="button" className="button button--primary" disabled={installing} onClick={install} data-testid="install-update">
              {installing ? 'Installing…' : 'Install and restart'}
            </button>
          </div>
        </Result>
      )}
      {error && (
        <Result tone="error">
          <strong>Couldn't check.</strong> {error}
        </Result>
      )}
    </>
  )
}

/* ----------------------------------------------------------------- model */

const PROVIDERS: { value: Provider; label: string; modelHint: string }[] = [
  { value: 'anthropic', label: 'Anthropic', modelHint: 'claude-opus-5-5' },
  { value: 'open_router', label: 'OpenRouter', modelHint: 'anthropic/claude-opus-5.5' },
  { value: 'open_ai', label: 'OpenAI', modelHint: 'gpt-5' },
]

function ModelPage({ notify, fail }: { notify: (tone: Tone, text: string) => void; fail: (e: unknown) => void }) {
  const [settings, setSettings] = useState<ModelSettings | null>(null)
  const [model, setModel] = useState('')
  const [apiKey, setApiKey] = useState('')
  const [savedAt, setSavedAt] = useState<number | null>(null)
  const [testing, setTesting] = useState(false)
  const [testResult, setTestResult] = useState<{ tone: Tone; text: string } | null>(null)
  const modelRef = useRef('')

  useEffect(() => {
    backend
      .getModelSettings()
      .then((s) => {
        setSettings(s)
        setModel(s.model ?? '')
        modelRef.current = s.model ?? ''
      })
      .catch(fail)
  }, [fail])

  if (!settings) return <Spinner label="Loading…" />
  const hint = PROVIDERS.find((p) => p.value === settings.provider)?.modelHint ?? ''

  const persist = (next: Partial<ModelSettings>, key?: string) => {
    const merged = { provider: settings.provider, model: settings.model, base_url: settings.base_url, ...next }
    return backend
      .saveModelSettings({ provider: merged.provider, model: merged.model?.trim() || null, base_url: merged.base_url }, key)
      .then((s) => {
        setSettings(s)
        setSavedAt(Date.now())
        setTestResult(null)
        return s
      })
      .catch(fail)
  }
  const commitModel = () => {
    if (model.trim() === modelRef.current) return
    modelRef.current = model.trim()
    persist({ model: model.trim() || null })
  }
  const commitKey = () => {
    const key = apiKey.trim()
    if (!key) return
    persist({}, key).then((s) => {
      if (s) {
        setApiKey('')
        notify('success', 'Key saved to your keychain')
      }
    })
  }
  const removeKey = () => persist({}, '').then((s) => s && notify('info', 'Key removed'))
  const test = () => {
    setTesting(true)
    setTestResult(null)
    backend
      .testModel()
      .then((line) => setTestResult({ tone: 'success', text: line }))
      .catch((e) => setTestResult({ tone: 'error', text: String(e) }))
      .finally(() => setTesting(false))
  }

  return (
    <section className="group">
      <div className="group__head">
        <p className="settings-muted">
          The model reads a server's tool list and writes its connector profile. That's all it does; syncing never involves it. Your key stays in your operating
          system's credential store.
        </p>
        <SavedMark at={savedAt} />
      </div>
      <Row label="Provider">
        <Segmented<Provider> label="Provider" value={settings.provider} onChange={(v) => persist({ provider: v })} options={PROVIDERS.map((p) => ({ value: p.value, label: p.label }))} />
      </Row>
      <Row label="Model" help={settings.provider === 'anthropic' ? 'Leave blank to use the default.' : 'Required for this provider.'}>
        <input
          type="text"
          aria-label="Model name"
          placeholder={hint}
          value={model}
          onChange={(e) => setModel(e.target.value)}
          onBlur={commitModel}
          onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
        />
      </Row>
      <Row
        label="API key"
        help={settings.has_key ? 'A key is saved. Paste a new one to replace it.' : 'Paste a key and press Enter.'}
        trailing={
          settings.has_key && (
            <button type="button" className="link" onClick={removeKey}>
              Remove key
            </button>
          )
        }
      >
        <input
          type="password"
          aria-label="API key"
          placeholder={settings.has_key ? '••••••••••••' : 'sk-…'}
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          onBlur={commitKey}
          onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
          autoComplete="off"
        />
      </Row>
      <Row label="Connection" help="Sends one short message and shows what comes back.">
        <button type="button" className="button" disabled={testing || !settings.has_key} onClick={test} data-testid="test-model">
          {testing ? <Spinner label="Testing…" /> : 'Test connection'}
        </button>
      </Row>
      {testResult && (
        <Result tone={testResult.tone}>
          {testResult.tone === 'success' ? (
            <>
              <strong>Connected.</strong> {testResult.text}
            </>
          ) : (
            <>
              <strong>Not connected.</strong> {testResult.text}
            </>
          )}
        </Result>
      )}
    </section>
  )
}

/* ------------------------------------------------------------ connectors */

function ConnectorsPage({ notify, fail }: { notify: (tone: Tone, text: string) => void; fail: (e: unknown) => void }) {
  const [connectors, setConnectors] = useState<ConnectorSummary[]>([])
  const [mode, setMode] = useState<'none' | 'server' | 'profile'>('none')
  const [blueprint, setBlueprint] = useState<{ id: string; json: string } | null>(null)
  const showBlueprint = (id: string) => {
    if (blueprint?.id === id) return setBlueprint(null)
    backend
      .exportBlueprint(id)
      .then((json) => setBlueprint({ id, json }))
      .catch(fail)
  }
  const [profileJson, setProfileJson] = useState('')
  const [busy, setBusy] = useState<string | null>(null)

  const reload = useCallback(() => {
    backend.listConnectors().then(setConnectors).catch(fail)
  }, [fail])
  useEffect(() => {
    reload()
  }, [reload])
  useEffect(() => backend.onChanged(reload), [reload])

  const run = (what: string, work: () => Promise<unknown>, done?: (result: unknown) => void) => {
    setBusy(what)
    work()
      .then((r) => {
        done?.(r)
        reload()
      })
      .catch(fail)
      .finally(() => setBusy(null))
  }

  return (
    <>
      <section className="group">
        {connectors.length === 0 && (
          <div className="empty">
            <strong>Nothing connected yet.</strong>
            <span>Your cards are local to this machine. Connect a system below and its tasks join the rail.</span>
          </div>
        )}
        {connectors.map((c) => (
          <div className="connector" key={c.id} data-testid={`connector-${c.id}`}>
            <span className={`connector__dot${c.last_error ? ' connector__dot--error' : c.paused ? ' connector__dot--paused' : ''}`} aria-hidden="true" />
            <div className="connector__main">
              <span className="connector__name">
                {c.name}
                {c.paused && <span className="connector__tag">Paused</span>}
              </span>
              <span className="connector__meta">
                {c.item_count} {c.item_count === 1 ? 'item' : 'items'} · {c.capabilities.write_status ? 'changes sync back' : 'read-only'} ·{' '}
                {c.paused ? 'not refreshing' : `refreshes every ${c.poll_seconds}s`}
              </span>
              <span className="connector__meta connector__meta--quiet">{c.server}</span>
              {c.last_error && <span className="connector__error">{c.last_error}</span>}
            </div>
            {c.paused ? (
              <button type="button" className="button" disabled={busy !== null} onClick={() => run(`pause-${c.id}`, () => backend.setConnectorPaused(c.id, false), () => notify('success', `${c.name} resumed`))}>
                {busy === `pause-${c.id}` ? <Spinner label="Resuming…" /> : 'Resume'}
              </button>
            ) : (
              <>
                <button type="button" className="button" disabled={busy !== null} onClick={() => run(`sync-${c.id}`, () => backend.syncConnector(c.id), (n) => notify('success', `Pulled ${n} items from ${c.name}`))}>
                  {busy === `sync-${c.id}` ? <Spinner label="Syncing…" /> : 'Sync now'}
                </button>
                <button type="button" className="button button--quiet" disabled={busy !== null} onClick={() => run(`pause-${c.id}`, () => backend.setConnectorPaused(c.id, true), () => notify('info', `${c.name} paused`))}>
                  Pause
                </button>
              </>
            )}
            <button type="button" className="button button--quiet" disabled={busy !== null} onClick={() => run(`remove-${c.id}`, () => backend.removeConnector(c.id), () => notify('info', `Removed ${c.name}`))}>
              Remove
            </button>
            <button type="button" className="button button--quiet" disabled={busy !== null} onClick={() => showBlueprint(c.id)} aria-expanded={blueprint?.id === c.id}>
              Blueprint
            </button>
          </div>
        ))}
        {blueprint && (
          <BlueprintPanel id={blueprint.id} json={blueprint.json} onClose={() => setBlueprint(null)} notify={notify} />
        )}
      </section>

      <h2 className="settings-subhead">Add a connector</h2>
      <div className="choices">
        <button type="button" className={`choice${mode === 'server' ? ' choice--on' : ''}`} disabled={busy !== null} onClick={() => setMode(mode === 'server' ? 'none' : 'server')} aria-expanded={mode === 'server'}>
          <span className="choice__title">Add server</span>
          <span className="choice__text">Any MCP server. A model works out how to read it.</span>
        </button>
        <button type="button" className="choice" disabled={busy !== null} onClick={() => run('demo', () => backend.addDemoConnector(), () => notify('success', 'Demo tracker connected'))}>
          <span className="choice__title">{busy === 'demo' ? <Spinner label="Connecting…" /> : 'Add demo tracker'}</span>
          <span className="choice__text">A small fake issue tracker, to see two-way sync working.</span>
        </button>
        <button type="button" className={`choice${mode === 'profile' ? ' choice--on' : ''}`} disabled={busy !== null} onClick={() => setMode(mode === 'profile' ? 'none' : 'profile')} aria-expanded={mode === 'profile'}>
          <span className="choice__title">Paste profile</span>
          <span className="choice__text">You already have a Connector Profile as JSON.</span>
        </button>
      </div>

      {mode === 'server' && (
        <section className="group">
          <AddServer
            notify={notify}
            fail={fail}
            onAdded={() => {
              setMode('none')
              reload()
            }}
          />
        </section>
      )}
      {mode === 'profile' && (
        <section className="group">
          <div className="panel">
            <textarea aria-label="Connector profile JSON" placeholder="Paste a Connector Profile (JSON)" rows={10} value={profileJson} onChange={(e) => setProfileJson(e.target.value)} />
            <div className="settings-actions settings-actions--end">
              <button
                type="button"
                className="button button--primary"
                disabled={busy !== null || !profileJson.trim()}
                onClick={() => {
                  if (looksLikeBlueprint(profileJson)) {
                    notify('info', 'That is a blueprint, not a profile. Use Add server: enter the address, paste the blueprint in its Blueprint box, and press Discover.')
                    return
                  }
                  run('profile', () => backend.addConnectorProfile(profileJson), () => {
                    setProfileJson('')
                    setMode('none')
                    notify('success', 'Connector added')
                  })
                }}
              >
                {busy === 'profile' ? <Spinner label="Adding…" /> : 'Add connector'}
              </button>
            </div>
          </div>
        </section>
      )}
    </>
  )
}

/** A blueprint has a `profile` inside and a `match`; a profile has its `server` at the top. */
function looksLikeBlueprint(json: string): boolean {
  try {
    const value = JSON.parse(json)
    return !!value && typeof value === 'object' && 'profile' in value && 'match' in value && !('server' in value)
  } catch {
    return false
  }
}

/** Describe a server, let the model write a profile, check the sample it pulled, then save. */
function AddServer({ notify, fail, onAdded }: { notify: (tone: Tone, text: string) => void; fail: (e: unknown) => void; onAdded: () => void }) {
  const [name, setName] = useState('')
  const [transport, setTransport] = useState<'http' | 'stdio'>('http')
  const [url, setUrl] = useState('')
  const [command, setCommand] = useState('')
  const [kind, setKind] = useState<ItemKind>('task')
  const [hint, setHint] = useState('')
  const [bearer, setBearer] = useState('')
  const [allowUnannotated, setAllowUnannotated] = useState(false)
  const [blueprint, setBlueprint] = useState('')
  const [phase, setPhase] = useState<'idle' | 'discovering' | 'signing-in' | 'adding'>('idle')
  const [result, setResult] = useState<DiscoveryResult | null>(null)
  const [problem, setProblem] = useState<string | null>(null)
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
  const ready = server !== null
  const busy = phase !== 'idle'

  const discover = (allow = allowUnannotated) => {
    if (!server) return
    setPhase('discovering')
    setResult(null)
    setProblem(null)
    backend
      .discoverConnector({ id, name: name.trim(), server, kind, hint, allow_unannotated_list: allow, bearer: bearer.trim() || null, blueprint: blueprint.trim() || null })
      .then((r) => {
        setResult(r)
        if (r.status === 'ok') setProfileJson(JSON.stringify(r.profile, null, 2))
      })
      .catch((e) => setProblem(String(e)))
      .finally(() => setPhase('idle'))
  }
  const signIn = () => {
    if (!server || server.transport !== 'http') return
    setPhase('signing-in')
    setResult(null)
    setProblem(null)
    backend
      .signIn(server.url)
      .then(() => {
        notify('success', 'Signed in')
        discover()
      })
      .catch((e) => {
        setProblem(String(e))
        setPhase('idle')
      })
  }
  const add = () => {
    setPhase('adding')
    backend
      .addConnectorProfile(profileJson, bearer.trim() || undefined)
      .then(() => {
        notify('success', `${(result?.status === 'ok' && String(result.profile.name)) || name.trim() || 'Server'} connected`)
        onAdded()
      })
      .catch(fail)
      .finally(() => setPhase('idle'))
  }

  return (
    <div className="panel panel--form" data-testid="add-server">
      <div className="panel__grid">
        <Row label="Name" help="Optional. Left blank, the server says what it is called.">
          <input type="text" aria-label="Server name" placeholder="Whatever the server calls itself" value={name} onChange={(e) => setName(e.target.value)} />
        </Row>
        <Row label="It holds">
          <Segmented<ItemKind>
            label="Item kind"
            value={kind}
            onChange={setKind}
            options={[
              { value: 'task', label: 'Tasks' },
              { value: 'event', label: 'Calendar' },
            ]}
          />
        </Row>
      </div>
      <Row label="Reach it by">
        <Segmented<'http' | 'stdio'>
          label="Transport"
          value={transport}
          onChange={setTransport}
          options={[
            { value: 'http', label: 'URL' },
            { value: 'stdio', label: 'Command' },
          ]}
        />
      </Row>
      {transport === 'http' ? (
        <>
          <Row label="Server URL">
            <input type="url" aria-label="Server URL" placeholder="https://example.com/mcp" value={url} onChange={(e) => setUrl(e.target.value)} />
          </Row>
          <Row label="Bearer token" help="Only for servers that hand out a static token. Servers with a sign-in leave this blank.">
            <input type="password" aria-label="Bearer token" placeholder="optional" value={bearer} onChange={(e) => setBearer(e.target.value)} autoComplete="off" />
          </Row>
        </>
      ) : (
        <Row label="Command" help="Runs on this machine and speaks MCP over its standard input and output.">
          <input type="text" aria-label="Server command" placeholder="npx some-mcp-server --flag" value={command} onChange={(e) => setCommand(e.target.value)} />
        </Row>
      )}
      <Row label="Hint for the model" help="Optional. Which project, whose items, what the product is called.">
        <input type="text" aria-label="Hint for the model" placeholder="only tickets assigned to me in the Platform project" value={hint} onChange={(e) => setHint(e.target.value)} />
      </Row>
      <Row label="Blueprint" help="Optional. One someone sent you, or one you are trying before it is in the registry. Used instead of the registry and the model." stack>
        <textarea aria-label="Blueprint to use" placeholder='{ "id": "...", "match": { "tools": [...] }, "profile": { ... } }' rows={4} value={blueprint} onChange={(e) => setBlueprint(e.target.value)} />
      </Row>
      <div className="settings-actions settings-actions--end">
        <button type="button" className="button button--primary" disabled={busy || !ready} onClick={() => discover()}>
          {phase === 'discovering' ? <Spinner label="Asking the model and checking the server…" /> : phase === 'signing-in' ? <Spinner label="Waiting for your browser…" /> : 'Discover'}
        </button>
      </div>
      {problem && (
        <Result tone="error">
          <strong>Couldn't connect.</strong> {problem}
        </Result>
      )}
      {result?.status === 'sign_in_required' && (
        <Result tone="info">
          <div data-testid="sign-in-required">
            <strong>This server wants you to sign in first.</strong> Your browser will open. When you come back, discovery continues on its own.
            <div className="settings-actions">
              <button type="button" className="button button--primary" disabled={busy} onClick={signIn}>
                Sign in
              </button>
            </div>
          </div>
        </Result>
      )}
      {result?.status === 'needs_confirmation' && (
        <Result tone="info">
          <strong>One check before going on.</strong> The model wants to list items with <code>{result.tool}</code>, which the server does not mark as read-only. Discovery would call it once.
          <div className="settings-actions">
            <button
              type="button"
              className="button"
              disabled={busy}
              onClick={() => {
                setAllowUnannotated(true)
                discover(true)
              }}
            >
              Allow and continue
            </button>
          </div>
        </Result>
      )}
      {result?.status === 'ok' && (
        <div className="discovery" data-testid="discovery-result">
          <Result tone="success">
            {result.blueprint ? (
              <>
                <strong>Found {result.sample.length} items</strong> on {String(result.profile.name)} using the {result.blueprint} blueprint. No model was needed.
              </>
            ) : (
              <>
                <strong>Found {result.sample.length} items</strong> on {String(result.profile.name)} in {result.rounds} {result.rounds === 1 ? 'round' : 'rounds'}, from {result.tools.length} tools.
              </>
            )}
            {result.notes.length > 0 && <span className="connector__meta"> Earlier tries: {result.notes.join('; ')}</span>}
          </Result>
          <ul className="discovery__sample">
            {result.sample.slice(0, 6).map((item) => (
              <li key={item.external_id}>
                <span>{item.title}</span>
                {item.status && <span className="connector__meta"> · {item.status}</span>}
              </li>
            ))}
            {result.sample.length > 6 && <li className="connector__meta">and {result.sample.length - 6} more</li>}
          </ul>
          <details className="disclosure">
            <summary>Profile the model wrote</summary>
            <textarea aria-label="Discovered profile JSON" rows={12} value={profileJson} onChange={(e) => setProfileJson(e.target.value)} />
          </details>
          <div className="settings-actions">
            <button type="button" className="button button--primary" disabled={busy || !profileJson.trim()} onClick={add}>
              {phase === 'adding' ? <Spinner label="Adding…" /> : 'Add connector'}
            </button>
          </div>
        </div>
      )}
    </div>
  )
}

/* ------------------------------------------------------------------ logs */

function LogsPage({ notify, fail }: { notify: (tone: Tone, text: string) => void; fail: (e: unknown) => void }) {
  const [text, setText] = useState('')
  const [follow, setFollow] = useState(true)
  const [version, setVersion] = useState('')
  useEffect(() => {
    backend.appVersion().then(setVersion).catch(() => setVersion(''))
  }, [])
  const refresh = useCallback(() => {
    backend.readLog(400).then(setText).catch(fail)
  }, [fail])
  useEffect(() => {
    refresh()
    if (!follow) return
    const t = window.setInterval(refresh, 3000)
    return () => window.clearInterval(t)
  }, [refresh, follow])
  return (
    <section className="group group--wide">
      <div className="settings-actions settings-actions--spread">
        <label className="inline-toggle">
          <Toggle checked={follow} onChange={setFollow} label="Follow the log" />
          <span>Follow</span>
          {version && <span className="connector__meta" data-testid="build">Build {version}</span>}
        </label>
        <span className="settings-actions__group">
          <button type="button" className="button button--quiet" onClick={() => backend.openLogFolder().catch(fail)}>
            Open folder
          </button>
          <button type="button" className="button button--quiet" onClick={() => navigator.clipboard?.writeText(text).then(() => notify('success', 'Log copied')).catch(() => notify('error', 'Could not copy'))}>
            Copy
          </button>
        </span>
      </div>
      <pre className="log" data-testid="log">
        {text || 'Nothing logged yet.'}
      </pre>
    </section>
  )
}
