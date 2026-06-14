//! IPC contract shared between `baler-daemon` and `baler-ui`.
//!
//! Transport is iceoryx2 0.8 — decentralized, no central daemon required
//! (see ISSUE_0002). These types are the payloads only; the zero-copy
//! transport edge lives in the [`transport`] module, gated behind the
//! `hardware` cargo feature. The daemon publishes [`StateSnapshot`]; the UI
//! publishes [`Command`].
//!
//! # Zero-copy layout
//!
//! Every payload that crosses the iceoryx2 shared-memory boundary derives
//! [`iceoryx2::prelude::ZeroCopySend`] and is `#[repr(C)]`. iceoryx2 0.8's
//! `#[derive(ZeroCopySend)]` *requires* a stable C layout: structs and C-like
//! enums must both carry an explicit `repr`. We use `#[repr(C)]` on the structs
//! and `#[repr(C)]` on the C-like enums (a fixed integer repr is the stable,
//! cross-process-safe choice for fieldless enums). The derive is only pulled in
//! under the `hardware` feature so the host build stays dependency-free.

// Re-export the ZeroCopySend derive only when the transport is compiled, so the
// `#[cfg_attr(...)]` attributes below resolve. Without the feature the derive is
// simply absent and the attributes are no-ops.
#[cfg(feature = "hardware")]
use iceoryx2::prelude::ZeroCopySend;

#[cfg(feature = "hardware")]
pub mod transport;

/// High-level machine mode (the master state machine, REQ_0011/REQ_0012).
#[cfg_attr(feature = "hardware", derive(ZeroCopySend))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
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
#[cfg_attr(feature = "hardware", derive(ZeroCopySend))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
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
#[cfg_attr(feature = "hardware", derive(ZeroCopySend))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum KnifePos {
    Unknown,
    In,
    Out,
}

/// Everything the UI needs to render one frame.
#[cfg_attr(feature = "hardware", derive(ZeroCopySend))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
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
