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
//!
//! Alongside them live the host-testable IPC pieces that used to be the
//! `baler-ipc` crate (ISSUE_0011): the contract types ([`ipc`]), the wire
//! [`codec`], and the transport bring-up [`bringup`] policy.

pub mod bringup;
pub mod channel;
pub mod codec;
pub mod counter;
pub mod input_conditioner;
pub mod ipc;
pub mod pulse;
pub mod state;

// The IPC contract types are re-exported at the crate root so callers can write
// `baler_core::Mode` / `baler_core::Command` (ISSUE_0011 — these moved out of the
// deleted `baler-ipc` crate).
pub use ipc::{Command, KnifePos, Mode, StateSnapshot};
