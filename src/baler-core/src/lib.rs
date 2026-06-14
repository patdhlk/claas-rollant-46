//! Pure, host-testable control logic for the baler controller.
//!
//! These four modules carry the safety-relevant behaviour and depend on no
//! hardware — they are driven by synthetic scan ticks and a fake clock, so they
//! are unit-tested in full on the host (the testing decision in FEAT_0001):
//!
//! - [`input_conditioner`] — debounce + rising/falling edge detection (R_TRIG).
//! - [`pulse`] — the 5 s output pulse generator with completed/aborted events.
//! - [`state`] — the master mode state machine and command gating.
//! - [`counter`] — session/total counters with atomic persistence.

pub mod counter;
pub mod input_conditioner;
pub mod pulse;
pub mod state;
