//! Auto-hide: watching the pointer and moving the rail in and out.
//!
//! The rules live in the core (`taskrail_core::autohide`), with no clock and
//! no operating system in them. This is the part that has both: one thread
//! asks the system where the pointer is a few times a second, hands that to
//! the rules, and makes the window match whatever phase they are in.
//!
//! The pointer is read from the system rather than from the page on purpose.
//! A tucked-away rail is a few pixels of window, and whether the webview
//! hears the pointer arrive on those pixels is exactly the kind of thing
//! that differs between machines. Where the pointer is on the desktop does
//! not.
//!
//! Everything that changes the window happens from this one thread, hopping
//! to the main thread for the call itself. The tray, the shortcut and the
//! page only tell the rules what they want; the thread notices within a
//! tick. That keeps a wipe from ever being started on the thread that would
//! have to draw it.

use std::time::{Duration, Instant};

use taskrail_core::autohide::{Layout, Phase, Pointer, Step};
use taskrail_core::settings::Edge;
use tauri::{AppHandle, Emitter, Manager};

use crate::rail::STRIP_WIDTH_PX;
use crate::state::{lock, AppState};

/// Tells the page which phase the rail is in: `"out"`, `"leaving"` or
/// `"tucked"`.
pub const PHASE_EVENT: &str = "rail:phase";

/// Where the dock can show part of the window, the rail is wiped out of its
/// strip and back by the shell, and the page stays still. Elsewhere the
/// window snaps and the page plays the motion.
pub const WIPES: bool = cfg!(windows);

const IDLE: Duration = Duration::from_millis(50);
const FRAME: Duration = Duration::from_millis(12);

/// How long a wipe takes.
const WIPE_MS: u32 = 140;
/// How long the page's own leaving animation runs, with a frame to spare.
const PAGE_LEAVE_MS: u32 = 180;

pub fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Out => "out",
        Phase::Leaving => "leaving",
        Phase::Tucked => "tucked",
    }
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut watcher = Watcher {
            last: Instant::now(),
            phase: None,
            wipe: None,
        };
        loop {
            std::thread::sleep(if watcher.wipe.is_some() { FRAME } else { IDLE });
            watcher.turn(&app);
        }
    });
}

/// The tray, the shortcut or a click on the strip asked for the rail.
pub fn bring_out(app: &AppHandle, why: &str) {
    let state = app.state::<AppState>();
    if !lock(&state.rail).settings.auto_hide {
        return;
    }
    if lock(&state.autohide).bring_out() != Step::Nothing {
        log::info!("auto-hide: rail asked out ({why})");
    }
}

/// Escape: put the rail away now.
pub fn put_away(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !lock(&state.rail).settings.auto_hide {
        return;
    }
    if lock(&state.autohide).put_away() != Step::Nothing {
        log::info!("auto-hide: rail asked away");
    }
}

struct Wipe {
    from: f64,
    to: f64,
    started: Instant,
}

struct Watcher {
    last: Instant,
    /// The phase the window was last made to match. `None` after a spell
    /// with nothing to watch, so the next turn starts from scratch.
    phase: Option<Phase>,
    wipe: Option<Wipe>,
}

impl Watcher {
    fn turn(&mut self, app: &AppHandle) {
        let now = Instant::now();
        let dt_ms = now.duration_since(self.last).as_millis().min(1_000) as u32;
        self.last = now;

        let state = app.state::<AppState>();
        let (edge, rail_px, held, animate) = {
            let mut rail = lock(&state.rail);
            // Settings open, hidden from the tray, or auto-hide off: nothing
            // to watch, and the rail is wherever those put it.
            if !rail.settings.auto_hide || rail.expanded || rail.tray_hidden {
                rail.wipe_px = None;
                self.wipe = None;
                self.phase = None;
                return;
            }
            (
                rail.settings.edge,
                rail.window_width_px(),
                rail.held || rail.preview_width_px.is_some(),
                rail.settings.animations,
            )
        };
        let leave_ms = match (animate, WIPES) {
            (false, _) => 0,
            (true, true) => WIPE_MS,
            (true, false) => PAGE_LEAVE_MS,
        };

        if let Some((pointer, cursor, layout)) = pointer(app, edge, rail_px) {
            let step = lock(&state.autohide).tick(dt_ms, pointer, held, leave_ms);
            if step != Step::Nothing {
                log::info!(
                    "auto-hide: {step:?} with the pointer at ({:.0}, {:.0}); rail edge x={:.0}, y={:.0}..{:.0}, held={held}",
                    cursor.0,
                    cursor.1,
                    layout.outer,
                    layout.top,
                    layout.bottom
                );
            }
        }

        let phase = lock(&state.autohide).phase();
        if self.phase != Some(phase) {
            self.enter(app, phase, f64::from(rail_px), animate);
        }
        self.advance(app);
    }

    /// Make the window and the page match a phase the rules have just
    /// entered.
    fn enter(&mut self, app: &AppHandle, phase: Phase, full: f64, animate: bool) {
        let state = app.state::<AppState>();
        let strip = f64::from(STRIP_WIDTH_PX);
        // The first turn after a spell away has nothing to move from.
        let moving = self.phase.is_some() && animate && WIPES;
        let showing = {
            let rail = lock(&state.rail);
            rail.wipe_px
                .map(f64::from)
                .unwrap_or(if rail.revealed { full } else { strip })
        };
        match phase {
            Phase::Out => {
                let mut rail = lock(&state.rail);
                rail.revealed = true;
                if moving && showing < full {
                    rail.wipe_px = Some(showing.round() as u32);
                    self.wipe = Some(Wipe {
                        from: showing,
                        to: full,
                        started: Instant::now(),
                    });
                } else {
                    rail.wipe_px = None;
                    self.wipe = None;
                }
            }
            Phase::Leaving => {
                if moving {
                    lock(&state.rail).wipe_px = Some(showing.round() as u32);
                    self.wipe = Some(Wipe {
                        from: showing,
                        to: strip,
                        started: Instant::now(),
                    });
                }
            }
            Phase::Tucked => {
                let mut rail = lock(&state.rail);
                rail.revealed = false;
                rail.wipe_px = None;
                self.wipe = None;
            }
        }
        log::info!("auto-hide: rail {}", phase_name(phase));
        self.phase = Some(phase);
        resize(app);
        let _ = app.emit(PHASE_EVENT, phase_name(phase));
    }

    /// One frame of a wipe.
    fn advance(&mut self, app: &AppHandle) {
        let Some(wipe) = &self.wipe else {
            return;
        };
        let state = app.state::<AppState>();
        let t = (wipe.started.elapsed().as_secs_f64() * 1_000.0 / f64::from(WIPE_MS)).min(1.0);
        // Fast at first, settling at the end.
        let eased = 1.0 - (1.0 - t).powi(3);
        let px = wipe.from + (wipe.to - wipe.from) * eased;
        let growing = wipe.to > wipe.from;
        {
            let mut rail = lock(&state.rail);
            // A wipe out ends with nothing held back; a wipe in rests on the
            // strip until the rules say tucked.
            rail.wipe_px = if t >= 1.0 && growing {
                None
            } else {
                Some(px.round() as u32)
            };
        }
        if t >= 1.0 {
            self.wipe = None;
        }
        resize(app);
    }
}

/// Where the pointer is against the rail, or `None` when the system will
/// not say or the rail has not been placed yet.
fn pointer(app: &AppHandle, edge: Edge, rail_px: u32) -> Option<(Pointer, (f64, f64), Layout)> {
    let state = app.state::<AppState>();
    let (rect, scale) = (*lock(&state.window_rect))?;
    let cursor = app.cursor_position().ok()?;
    let layout = Layout {
        edge,
        outer: match edge {
            Edge::Right => f64::from(rect.x + rect.width),
            Edge::Left => f64::from(rect.x),
        },
        top: f64::from(rect.y),
        bottom: f64::from(rect.y + rect.height),
        strip: f64::from(STRIP_WIDTH_PX) * scale,
        rail: f64::from(rail_px) * scale,
    };
    Some((
        layout.pointer(cursor.x, cursor.y),
        (cursor.x, cursor.y),
        layout,
    ))
}

/// The window work happens on the main thread.
fn resize(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Err(e) = crate::dock::resize(&handle) {
            log::warn!("auto-hide: could not change the window: {e}");
        }
    });
}
