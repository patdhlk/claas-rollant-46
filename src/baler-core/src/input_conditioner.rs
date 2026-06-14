//! Debounce + edge detection for digital inputs (REQ_0006).
//!
//! One [`Debouncer`] per input bit. Each scan cycle feeds one raw sample; the
//! debounced level only changes after `threshold` consecutive stable samples,
//! and the change is reported as a rising/falling [`Edge`] (the PLC `R_TRIG` /
//! `F_TRIG` idiom). At a 10 ms scan, a threshold of 3 is ~30 ms.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    None,
    Rising,
    Falling,
}

#[derive(Debug, Clone)]
pub struct Debouncer {
    threshold: u8,
    level: bool,
    candidate: bool,
    count: u8,
}

impl Debouncer {
    /// `threshold` is the number of consecutive opposite samples required to
    /// accept a change (clamped to at least 1). `initial` is the starting level.
    pub fn new(threshold: u8, initial: bool) -> Self {
        Self {
            threshold: threshold.max(1),
            level: initial,
            candidate: initial,
            count: 0,
        }
    }

    /// Feed one raw sample. Returns the edge produced if the debounced level
    /// changed on this sample, else [`Edge::None`].
    pub fn update(&mut self, raw: bool) -> Edge {
        if raw == self.level {
            // Back to the stable level — reset any pending change.
            self.candidate = raw;
            self.count = 0;
            return Edge::None;
        }
        if raw == self.candidate {
            self.count = self.count.saturating_add(1);
        } else {
            self.candidate = raw;
            self.count = 1;
        }
        if self.count >= self.threshold {
            self.level = raw;
            self.count = 0;
            return if raw { Edge::Rising } else { Edge::Falling };
        }
        Edge::None
    }

    /// The current debounced level.
    pub fn level(&self) -> bool {
        self.level
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_change_produces_one_edge() {
        let mut d = Debouncer::new(3, false);
        assert_eq!(d.update(true), Edge::None); // 1
        assert_eq!(d.update(true), Edge::None); // 2
        assert_eq!(d.update(true), Edge::Rising); // 3 -> accepted
        assert!(d.level());
        // Already high: further highs are no-ops.
        assert_eq!(d.update(true), Edge::None);
        // Falling needs the threshold too.
        assert_eq!(d.update(false), Edge::None);
        assert_eq!(d.update(false), Edge::None);
        assert_eq!(d.update(false), Edge::Falling);
        assert!(!d.level());
    }

    #[test]
    fn brief_glitch_is_rejected() {
        let mut d = Debouncer::new(3, false);
        assert_eq!(d.update(true), Edge::None); // glitch sample 1
        assert_eq!(d.update(false), Edge::None); // settles back, no edge
        assert!(!d.level());
        // The aborted attempt must not shorten the next real transition.
        assert_eq!(d.update(true), Edge::None);
        assert_eq!(d.update(true), Edge::None);
        assert_eq!(d.update(true), Edge::Rising);
    }

    #[test]
    fn threshold_one_is_immediate() {
        let mut d = Debouncer::new(0, false); // clamped to 1
        assert_eq!(d.update(true), Edge::Rising);
        assert_eq!(d.update(false), Edge::Falling);
    }
}
