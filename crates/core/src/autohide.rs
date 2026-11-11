//! When an auto-hidden rail comes out and when it goes back.
//!
//! The rules are the ones people already know from an auto-hidden taskbar or
//! Dock: push the pointer against the edge and it comes out, move away and
//! it goes back. Written here as a small machine with no clock and no
//! operating system in it, so every rule can be tested. The shell feeds it
//! where the pointer is a few times a second and does what it says.

use crate::settings::Edge;

/// How long the pointer has to rest on the edge. Long enough that crossing
/// to a monitor on that side does not bring the rail out, short enough to
/// feel immediate.
pub const DWELL_MS: u32 = 150;

/// How long after the pointer leaves before the rail starts to go back.
pub const AWAY_MS: u32 = 500;

/// A rail brought out by the tray or the shortcut waits this long for the
/// pointer to come to it before it gives up and goes back.
pub const UNVISITED_MS: u32 = 6_000;

/// Where the pointer is, as far as the rail cares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pointer {
    /// Pressed against the rail's edge of the screen.
    pub at_edge: bool,
    /// Over where the rail is when it is out.
    pub over_rail: bool,
}

/// A little slack around the rail, so a pointer that drifts a few pixels
/// off its inner edge has not left.
const SLACK_PX: f64 = 12.0;

/// Where the rail lives on the desktop, in the same pixels the pointer is
/// reported in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub edge: Edge,
    /// The x of the screen edge the rail sits against.
    pub outer: f64,
    pub top: f64,
    pub bottom: f64,
    /// How wide the strip is when the rail is tucked away.
    pub strip: f64,
    /// How wide the rail is when it is out.
    pub rail: f64,
}

impl Layout {
    pub fn pointer(&self, x: f64, y: f64) -> Pointer {
        if y < self.top || y >= self.bottom {
            return Pointer::default();
        }
        // How far in from the screen edge the pointer is. Negative is past
        // the edge, which only happens with another display on that side.
        let inward = match self.edge {
            Edge::Right => self.outer - x,
            Edge::Left => x - self.outer,
        };
        // On the right the last pixel of the screen is one short of the edge.
        let on_screen = match self.edge {
            Edge::Right => inward > 0.0,
            Edge::Left => inward >= 0.0,
        };
        Pointer {
            at_edge: on_screen && inward <= self.strip,
            over_rail: on_screen && inward <= self.rail + SLACK_PX,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Out over the desktop.
    Out,
    /// On its way back in: the page is playing the animation.
    Leaving,
    /// Tucked into its strip at the edge.
    Tucked,
}

/// What the shell should do after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// Show the whole rail, now.
    BringOut,
    /// Tell the page to play the rail out; the strip comes after.
    StartLeaving,
    /// Shrink to the strip.
    Tuck,
}

#[derive(Debug, Clone)]
pub struct AutoHide {
    phase: Phase,
    /// The pointer must be seen off the edge before the edge counts, so a
    /// rail put away with the pointer still on the edge stays away.
    armed: bool,
    /// The pointer has been over the rail since it came out.
    visited: bool,
    /// Put away on purpose: the pointer being over the rail does not stop it.
    forced: bool,
    dwell_ms: u32,
    away_ms: u32,
    out_ms: u32,
    leaving_ms: u32,
}

impl AutoHide {
    pub fn new(out: bool) -> Self {
        Self {
            phase: if out { Phase::Out } else { Phase::Tucked },
            armed: false,
            visited: false,
            forced: false,
            dwell_ms: 0,
            away_ms: 0,
            out_ms: 0,
            leaving_ms: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// One look at the pointer, `dt_ms` after the last. `held` is the page
    /// saying the rail is in use (someone is typing in it, or dragging a
    /// row) and must stay. `leave_ms` is how long the page's animation runs.
    pub fn tick(&mut self, dt_ms: u32, pointer: Pointer, held: bool, leave_ms: u32) -> Step {
        match self.phase {
            Phase::Tucked => {
                if !pointer.at_edge {
                    self.armed = true;
                    self.dwell_ms = 0;
                    return Step::Nothing;
                }
                if !self.armed {
                    return Step::Nothing;
                }
                self.dwell_ms += dt_ms;
                if self.dwell_ms < DWELL_MS {
                    return Step::Nothing;
                }
                // The pointer is on the edge, which is on the rail.
                self.come_out(true);
                Step::BringOut
            }
            Phase::Out => {
                self.out_ms = self.out_ms.saturating_add(dt_ms);
                let over = pointer.over_rail || pointer.at_edge;
                if over {
                    self.visited = true;
                }
                if over || held {
                    self.away_ms = 0;
                    return Step::Nothing;
                }
                if !self.visited && self.out_ms < UNVISITED_MS {
                    return Step::Nothing;
                }
                self.away_ms += dt_ms;
                if self.away_ms < AWAY_MS {
                    return Step::Nothing;
                }
                self.start_leaving(false);
                Step::StartLeaving
            }
            Phase::Leaving => {
                let wanted_back = pointer.over_rail || pointer.at_edge || held;
                if wanted_back && !self.forced {
                    self.come_out(true);
                    return Step::BringOut;
                }
                self.leaving_ms += dt_ms;
                if self.leaving_ms < leave_ms {
                    return Step::Nothing;
                }
                self.phase = Phase::Tucked;
                self.armed = !pointer.at_edge;
                self.dwell_ms = 0;
                Step::Tuck
            }
        }
    }

    /// The tray, the shortcut or a click on the strip asked for the rail.
    pub fn bring_out(&mut self) -> Step {
        let was = self.phase;
        self.come_out(false);
        if was == Phase::Out {
            Step::Nothing
        } else {
            Step::BringOut
        }
    }

    /// Escape: put it away now, whatever the pointer is doing.
    pub fn put_away(&mut self) -> Step {
        if self.phase != Phase::Out {
            return Step::Nothing;
        }
        self.start_leaving(true);
        Step::StartLeaving
    }

    fn come_out(&mut self, visited: bool) {
        self.phase = Phase::Out;
        self.visited = visited;
        self.forced = false;
        self.away_ms = 0;
        self.out_ms = 0;
        self.dwell_ms = 0;
    }

    fn start_leaving(&mut self, forced: bool) {
        self.phase = Phase::Leaving;
        self.forced = forced;
        self.leaving_ms = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: u32 = 50;
    const LEAVE: u32 = 150;
    const ELSEWHERE: Pointer = Pointer {
        at_edge: false,
        over_rail: false,
    };
    const EDGE: Pointer = Pointer {
        at_edge: true,
        over_rail: false,
    };
    const ON_RAIL: Pointer = Pointer {
        at_edge: false,
        over_rail: true,
    };

    /// Runs ticks with the pointer in one place and returns every step that
    /// asked for something.
    fn run(machine: &mut AutoHide, ms: u32, pointer: Pointer, held: bool) -> Vec<Step> {
        (0..ms / TICK)
            .map(|_| machine.tick(TICK, pointer, held, LEAVE))
            .filter(|step| *step != Step::Nothing)
            .collect()
    }

    fn tucked() -> AutoHide {
        let mut machine = AutoHide::new(false);
        run(&mut machine, 100, ELSEWHERE, false);
        machine
    }

    #[test]
    fn resting_the_pointer_on_the_edge_brings_the_rail_out() {
        let mut machine = tucked();
        assert_eq!(run(&mut machine, 300, EDGE, false), vec![Step::BringOut]);
        assert_eq!(machine.phase(), Phase::Out);
    }

    #[test]
    fn brushing_past_the_edge_does_not() {
        let mut machine = tucked();
        assert!(run(&mut machine, 100, EDGE, false).is_empty());
        assert!(run(&mut machine, 100, ELSEWHERE, false).is_empty());
        assert!(run(&mut machine, 100, EDGE, false).is_empty());
        assert_eq!(machine.phase(), Phase::Tucked);
    }

    #[test]
    fn moving_away_puts_it_back_after_a_moment() {
        let mut machine = tucked();
        run(&mut machine, 300, EDGE, false);
        assert!(run(&mut machine, 2_000, ON_RAIL, false).is_empty());
        assert_eq!(
            run(&mut machine, 1_000, ELSEWHERE, false),
            vec![Step::StartLeaving, Step::Tuck]
        );
        assert_eq!(machine.phase(), Phase::Tucked);
    }

    #[test]
    fn a_quick_trip_off_the_rail_and_back_changes_nothing() {
        let mut machine = tucked();
        run(&mut machine, 300, EDGE, false);
        assert!(run(&mut machine, 300, ELSEWHERE, false).is_empty());
        assert!(run(&mut machine, 300, ON_RAIL, false).is_empty());
        assert_eq!(machine.phase(), Phase::Out);
    }

    #[test]
    fn coming_back_while_it_is_leaving_brings_it_straight_out() {
        let mut machine = tucked();
        run(&mut machine, 300, EDGE, false);
        assert_eq!(
            run(&mut machine, AWAY_MS, ELSEWHERE, false),
            vec![Step::StartLeaving]
        );
        assert_eq!(machine.tick(TICK, ON_RAIL, false, LEAVE), Step::BringOut);
        assert_eq!(machine.phase(), Phase::Out);
    }

    #[test]
    fn a_rail_in_use_stays_out_however_far_the_pointer_goes() {
        let mut machine = tucked();
        run(&mut machine, 300, EDGE, false);
        assert!(run(&mut machine, 10_000, ELSEWHERE, true).is_empty());
        assert_eq!(
            run(&mut machine, 1_000, ELSEWHERE, false),
            vec![Step::StartLeaving, Step::Tuck]
        );
    }

    #[test]
    fn escape_puts_it_away_even_with_the_pointer_on_it() {
        let mut machine = tucked();
        run(&mut machine, 300, EDGE, false);
        assert_eq!(machine.put_away(), Step::StartLeaving);
        assert_eq!(run(&mut machine, 300, ON_RAIL, false), vec![Step::Tuck]);
    }

    #[test]
    fn put_away_with_the_pointer_on_the_edge_it_waits_for_the_pointer_to_leave_and_return() {
        let mut machine = tucked();
        run(&mut machine, 300, EDGE, false);
        machine.put_away();
        assert_eq!(run(&mut machine, 300, EDGE, false), vec![Step::Tuck]);
        assert!(run(&mut machine, 2_000, EDGE, false).is_empty());
        run(&mut machine, 100, ELSEWHERE, false);
        assert_eq!(run(&mut machine, 300, EDGE, false), vec![Step::BringOut]);
    }

    #[test]
    fn brought_out_by_the_shortcut_it_waits_for_the_pointer_before_it_counts_it_gone() {
        let mut machine = tucked();
        assert_eq!(machine.bring_out(), Step::BringOut);
        assert!(run(&mut machine, 3_000, ELSEWHERE, false).is_empty());
        assert!(run(&mut machine, 500, ON_RAIL, false).is_empty());
        assert_eq!(
            run(&mut machine, 1_000, ELSEWHERE, false),
            vec![Step::StartLeaving, Step::Tuck]
        );
    }

    #[test]
    fn brought_out_and_never_visited_it_goes_back_in_the_end() {
        let mut machine = tucked();
        machine.bring_out();
        let steps = run(&mut machine, UNVISITED_MS + 2_000, ELSEWHERE, false);
        assert_eq!(steps, vec![Step::StartLeaving, Step::Tuck]);
    }

    fn right_edge_of_a_1920_wide_screen() -> Layout {
        Layout {
            edge: Edge::Right,
            outer: 1920.0,
            top: 0.0,
            bottom: 1040.0,
            strip: 8.0,
            rail: 320.0,
        }
    }

    #[test]
    fn the_last_pixels_of_the_screen_are_the_edge() {
        let layout = right_edge_of_a_1920_wide_screen();
        assert!(layout.pointer(1919.0, 500.0).at_edge);
        assert!(layout.pointer(1912.0, 500.0).at_edge);
        assert!(!layout.pointer(1900.0, 500.0).at_edge);
        assert!(layout.pointer(1900.0, 500.0).over_rail);
        assert!(!layout.pointer(1500.0, 500.0).over_rail);
    }

    #[test]
    fn a_pointer_on_the_display_beyond_the_edge_is_not_on_the_rail() {
        let layout = right_edge_of_a_1920_wide_screen();
        assert_eq!(layout.pointer(1920.0, 500.0), Pointer::default());
        assert_eq!(layout.pointer(2400.0, 500.0), Pointer::default());
    }

    #[test]
    fn the_taskbar_below_the_rail_is_not_the_edge() {
        let layout = right_edge_of_a_1920_wide_screen();
        assert_eq!(layout.pointer(1919.0, 1060.0), Pointer::default());
    }

    #[test]
    fn a_rail_on_the_left_mirrors_all_of_it() {
        let layout = Layout {
            edge: Edge::Left,
            outer: 0.0,
            ..right_edge_of_a_1920_wide_screen()
        };
        assert!(layout.pointer(0.0, 500.0).at_edge);
        assert!(layout.pointer(7.0, 500.0).at_edge);
        assert!(!layout.pointer(40.0, 500.0).at_edge);
        assert!(layout.pointer(40.0, 500.0).over_rail);
        assert_eq!(layout.pointer(-10.0, 500.0), Pointer::default());
    }

    #[test]
    fn asking_for_a_rail_that_is_already_out_does_nothing() {
        let mut machine = AutoHide::new(true);
        assert_eq!(machine.bring_out(), Step::Nothing);
    }
}
