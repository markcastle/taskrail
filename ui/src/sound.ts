// Two small sounds, made on the spot rather than shipped as files: a tick
// when something is completed, a softer one when something is started.
// Nothing plays unless the person turned sounds on, and the audio context
// is only created on the first play, which always follows a click.

export type Cue = 'complete' | 'start'

let context: AudioContext | null = null
let enabled = false

/** Sounds on or off. Off is the default; the setting decides. */
export function setSoundsEnabled(on: boolean) {
  enabled = on
}

function tone(ctx: AudioContext, at: number, frequency: number, length: number, peak: number) {
  const osc = ctx.createOscillator()
  const gain = ctx.createGain()
  osc.type = 'sine'
  osc.frequency.setValueAtTime(frequency, at)
  gain.gain.setValueAtTime(0.0001, at)
  gain.gain.exponentialRampToValueAtTime(peak, at + 0.008)
  gain.gain.exponentialRampToValueAtTime(0.0001, at + length)
  osc.connect(gain).connect(ctx.destination)
  osc.start(at)
  osc.stop(at + length + 0.02)
}

/** Play a cue, if sounds are on. Never throws: a browser without audio just stays quiet. */
export function play(cue: Cue) {
  if (!enabled) return
  try {
    context ??= new AudioContext()
    const ctx = context
    if (ctx.state === 'suspended') void ctx.resume()
    const now = ctx.currentTime
    if (cue === 'complete') {
      // A short two-note tick, rising: done.
      tone(ctx, now, 880, 0.07, 0.06)
      tone(ctx, now + 0.07, 1320, 0.1, 0.05)
    } else {
      // One soft note: picked up.
      tone(ctx, now, 660, 0.09, 0.035)
    }
  } catch {
    /* no audio here; the rail stays quiet */
  }
}
