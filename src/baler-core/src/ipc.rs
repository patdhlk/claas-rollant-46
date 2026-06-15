//! IPC contract shared between `baler-daemon` and `baler-ui` (ISSUE_0011).
//!
//! These are pure data types — the daemon publishes [`StateSnapshot`], the UI
//! publishes [`Command`]. They cross the process boundary over taktora's
//! `transport-iox` channels, serialised with the project's [`PostcardCodec`]
//! ([`crate::codec`]). They were previously owned by the now-deleted `baler-ipc`
//! crate and carried `iceoryx2::ZeroCopySend` + `#[repr(C)]`; the taktora
//! transport frames a serialised payload inside its own envelope, so a plain
//! `serde` derive is all that is needed here and the host build stays free of
//! iceoryx2.

use serde::{Deserialize, Serialize};

/// High-level machine mode (the master state machine, REQ_0011/REQ_0012).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Booting; commands rejected until EtherCAT is healthy (REQ_0011).
    Initializing,
    /// Bus healthy; operator commands accepted.
    Operational,
    /// EtherCAT lost; outputs dropped by the coupler watchdog (REQ_0012).
    Fault,
    /// Maintenance mode: EtherCAT down, static IP up (REQ_0009).
    Ethernet,
}

/// A command the UI can issue to the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Fire the 5 s wrap pulse (operator-confirmed, REQ_0003).
    Wrap,
    /// Fire the 5 s knife-toggle pulse (REQ_0005).
    ToggleKnife,
    /// Reset the session counter (operator action, REQ_0007).
    ResetSession,
    /// Reset the total counter (PIN-gated by the UI before sending, REQ_0007).
    ResetTotal,
    /// Switch to Ethernet maintenance mode (idle-only, REQ_0008).
    EnterEthernet,
    /// Return from Ethernet mode and re-init EtherCAT (REQ_0009).
    ReturnToEthercat,
}

/// Knife position as reported by DI2, or unknown while the bus is down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KnifePos {
    Unknown,
    In,
    Out,
}

/// Everything the UI needs to render one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub mode: Mode,
    pub bale_full: bool,
    pub knife: KnifePos,
    /// True when a wrap is permitted and the bale is full (arms the F1 softkey).
    pub wrap_armed: bool,
    pub wrap_active: bool,
    pub knife_active: bool,
    pub session: u64,
    pub total: u64,
    /// Static IP shown in Ethernet mode (REQ_0009); valid only when `ip_valid`.
    pub ip: [u8; 4],
    pub ip_valid: bool,
}
