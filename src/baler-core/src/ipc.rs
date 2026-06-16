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
    /// Fire the 5 s directional knife pulse. The daemon picks the direction from
    /// the live ch2 (DI2) value at trigger time — ch2 true → knives-in (DO2),
    /// ch2 false → knives-out (DO3) (REQ_0005).
    ToggleKnife,
    /// Reset the session counter (operator action, REQ_0007).
    ResetSession,
    /// Reset the total counter (PIN-gated by the UI before sending, REQ_0007).
    ResetTotal,
    /// Switch to Ethernet maintenance mode (idle-only, REQ_0008).
    EnterEthernet,
    /// Return from Ethernet mode and re-init EtherCAT (REQ_0009).
    ReturnToEthercat,
    /// IO test screen: drive the outputs directly from the operator's held keys
    /// (momentary). Sent every frame while the page is open; its absence is what
    /// the daemon's watchdog uses to fail safe (REQ_0018). `knives_in`/`knives_out`
    /// drive DO2/DO3 respectively; the daemon interlocks them so both can never be
    /// energized together.
    ManualIo {
        wrap: bool,
        knives_in: bool,
        knives_out: bool,
    },
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
    /// Advisory hint: the bale is full and a wrap is permitted. No longer gates the
    /// F1 softkey — firing a wrap is the operator's responsibility (REQ_0013).
    pub wrap_armed: bool,
    /// Bale-full attention latch: held true for at least 20 s after a true DI1 so
    /// the operator notices, cleared early by a wrap (REQ_0017).
    pub full_latched: bool,
    pub wrap_active: bool,
    pub knife_active: bool,
    pub session: u64,
    pub total: u64,
    /// Raw (un-debounced) discrete inputs for the IO test screen (REQ_0018):
    /// `di1` = bale-full sensor, `di2` = knife-position sensor.
    pub di1: bool,
    pub di2: bool,
    /// Static IP shown in Ethernet mode (REQ_0009); valid only when `ip_valid`.
    pub ip: [u8; 4],
    pub ip_valid: bool,
}
