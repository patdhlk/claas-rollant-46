//! Fixed-duration output pulse generator (REQ_0003, REQ_0004, REQ_0005).
//!
//! One [`PulseEngine`] per output (wrap, knife). A pulse is started on a button
//! rising edge via [`PulseEngine::trigger`] and held high for `duration` of
//! healthy scan cycles. It emits [`PulseEvent::Completed`] when it finishes
//! cleanly (the moment the daemon increments the counter for a wrap) and
//! [`PulseEvent::Aborted`] if the bus goes unhealthy mid-pulse — an aborted
//! pulse must not be counted. Re-triggering while active is locked out.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PulseEvent {
    None,
    Completed,
    Aborted,
}

#[derive(Debug, Clone)]
pub struct PulseEngine {
    duration: Duration,
    elapsed: Duration,
    active: bool,
}

impl PulseEngine {
    pub fn new(duration: Duration) -> Self {
        Self {
            duration,
            elapsed: Duration::ZERO,
            active: false,
        }
    }

    /// Start a pulse. Returns `false` (and does nothing) if one is already
    /// active — the same output cannot be re-triggered until its pulse ends.
    pub fn trigger(&mut self) -> bool {
        if self.active {
            return false;
        }
        self.active = true;
        self.elapsed = Duration::ZERO;
        true
    }

    /// Advance one scan cycle by `dt`. The pulse only progresses while
    /// `bus_healthy`; a drop aborts it immediately with outputs going low.
    pub fn tick(&mut self, dt: Duration, bus_healthy: bool) -> PulseEvent {
        if !self.active {
            return PulseEvent::None;
        }
        if !bus_healthy {
            self.reset();
            return PulseEvent::Aborted;
        }
        self.elapsed = self.elapsed.saturating_add(dt);
        if self.elapsed >= self.duration {
            self.reset();
            return PulseEvent::Completed;
        }
        PulseEvent::None
    }

    /// The commanded output level for this pulse (high while active).
    pub fn output(&self) -> bool {
        self.active
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    fn reset(&mut self) {
        self.active = false;
        self.elapsed = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCAN: Duration = Duration::from_millis(10);
    const PULSE: Duration = Duration::from_secs(5);

    #[test]
    fn completes_after_exactly_the_duration() {
        let mut p = PulseEngine::new(PULSE);
        assert!(!p.output());
        assert!(p.trigger());
        assert!(p.output());

        // 500 ticks of 10 ms == 5 s. The first 499 produce nothing.
        for _ in 0..499 {
            assert_eq!(p.tick(SCAN, true), PulseEvent::None);
            assert!(p.output());
        }
        assert_eq!(p.tick(SCAN, true), PulseEvent::Completed);
        assert!(!p.output());
        assert!(!p.is_active());
    }

    #[test]
    fn bus_loss_aborts_without_completion() {
        let mut p = PulseEngine::new(PULSE);
        p.trigger();
        for _ in 0..10 {
            assert_eq!(p.tick(SCAN, true), PulseEvent::None);
        }
        assert_eq!(p.tick(SCAN, false), PulseEvent::Aborted);
        assert!(!p.output());
        assert!(!p.is_active());
    }

    #[test]
    fn retrigger_while_active_is_ignored() {
        let mut p = PulseEngine::new(PULSE);
        assert!(p.trigger());
        p.tick(SCAN, true);
        assert!(!p.trigger()); // locked out
    }

    #[test]
    fn tick_while_idle_is_inert() {
        let mut p = PulseEngine::new(PULSE);
        assert_eq!(p.tick(SCAN, true), PulseEvent::None);
        assert_eq!(p.tick(SCAN, false), PulseEvent::None);
        assert!(!p.output());
    }

    #[test]
    fn can_retrigger_after_completion() {
        let mut p = PulseEngine::new(Duration::from_millis(20));
        p.trigger();
        assert_eq!(p.tick(SCAN, true), PulseEvent::None);
        assert_eq!(p.tick(SCAN, true), PulseEvent::Completed);
        assert!(p.trigger()); // free again
    }
}
