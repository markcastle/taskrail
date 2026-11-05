import { useEffect, useState, type ReactNode } from 'react'

/** A row of mutually exclusive choices. Saves on tap; no Save button. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T
  options: { value: T; label: string }[]
  onChange: (value: T) => void
  label: string
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={o.value === value}
          className={`segmented__option${o.value === value ? ' segmented__option--on' : ''}`}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  )
}

/** A slider with its value spelled out beside it. */
export function Slider({
  value,
  min,
  max,
  step = 1,
  unit,
  onChange,
  label,
}: {
  value: number
  min: number
  max: number
  step?: number
  unit: string
  onChange: (value: number) => void
  label: string
}) {
  return (
    <div className="slider">
      <input type="range" aria-label={label} min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
      <output className="slider__value">
        {value} {unit}
      </output>
    </div>
  )
}

/** An on/off switch. */
export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (checked: boolean) => void; label: string }) {
  return (
    <button type="button" role="switch" aria-checked={checked} aria-label={label} className={`toggle${checked ? ' toggle--on' : ''}`} onClick={() => onChange(!checked)}>
      <span className="toggle__knob" />
    </button>
  )
}

export function Spinner({ label }: { label?: string }) {
  return (
    <span className="spinner" role="status" aria-live="polite">
      <span className="spinner__ring" aria-hidden="true" />
      {label && <span>{label}</span>}
    </span>
  )
}

export type Tone = 'info' | 'success' | 'error'

/** A result shown where the action happened, coloured by what happened. */
export function Result({ tone, children }: { tone: Tone; children: ReactNode }) {
  const mark = tone === 'success' ? '✓' : tone === 'error' ? '!' : 'i'
  return (
    <div className={`result result--${tone}`} role={tone === 'error' ? 'alert' : 'status'} data-tone={tone}>
      <span className="result__mark" aria-hidden="true">
        {mark}
      </span>
      <div className="result__body">{children}</div>
    </div>
  )
}

/** A small "Saved" that appears for a moment after a change lands. */
export function SavedMark({ at }: { at: number | null }) {
  const [hiddenAt, setHiddenAt] = useState<number | null>(null)
  useEffect(() => {
    if (!at) return
    const t = window.setTimeout(() => setHiddenAt(at), 1800)
    return () => window.clearTimeout(t)
  }, [at])
  const visible = at !== null && at !== hiddenAt
  return (
    <span className={`saved${visible ? ' saved--visible' : ''}`} aria-live="polite">
      ✓ Saved
    </span>
  )
}
