//! Hardware ports the scan loop depends on, behind traits so the pure control
//! logic stays host-testable. The real implementations are the edge modules,
//! each tracked by a requirement / open issue:
//!
//! - [`BusIo`] → `EtherCatIo`, the taktora ethercat-wago connector (REQ_0015).
//! - [`NetworkController`] → `NetworkMode`, the NIC EtherCAT↔static-IP switch
//!   (REQ_0009).
//! - [`Watchdog`] → `WatchdogPetter` over `/dev/watchdog` (REQ_0010, ISSUE_0001).
//!
//! The `Sim*` implementations below let the daemon build and run on the host.

use std::net::Ipv4Addr;

pub struct Inputs {
    pub bale_full: bool,
    pub knife_in: bool,
    /// EtherCAT connector health for this cycle.
    pub healthy: bool,
}

// Fields are read by the real `BusIo` (EtherCatIo); the `SimBus` drops them.
#[allow(dead_code)]
pub struct Outputs {
    pub wrap: bool,
    pub knife: bool,
}

/// The host/sim bus port. The EtherCAT build talks to the WAGO connector through
/// the concrete `ethercat_io::WagoBus` (executor-driven), not this trait, so the
/// trait and its `SimBus` impl exist only off the `ethercat` feature (ISSUE_0009).
#[cfg(not(feature = "ethercat"))]
pub trait BusIo {
    /// Read DI1/DI2 and the connector health for this scan cycle.
    fn poll(&mut self) -> Inputs;
    /// Write DO1/DO2 for this scan cycle.
    fn write(&mut self, out: Outputs);
}

/// Switches the single shared NIC between EtherCAT (raw, no IP) and Ethernet
/// (static IPv4 up) modes. The EtherCAT master is stopped/recreated by the
/// daemon around these calls; this trait is purely IP-level.
///
/// `Send` because the EtherCAT build drives the control cycle (and thus the
/// network controller) from a taktora executor item, which runs on a worker
/// thread (ISSUE_0009).
pub trait NetworkController: Send {
    /// Bring up the configured static IP and return it. Errors if the link/addr
    /// commands fail (interface missing, no `CAP_NET_ADMIN`, …).
    fn enter_ethernet(&mut self) -> Result<Ipv4Addr, NetworkError>;
    /// Tear the IP interface back down so ethercrab can reclaim the raw socket.
    fn enter_ethercat(&mut self) -> Result<(), NetworkError>;
}

/// Network-switch failure. Defined unconditionally so the trait signature is
/// identical on host and target builds; variants are only constructed by the
/// hardware-gated `network_mode` impl.
#[derive(Debug)]
#[cfg_attr(not(feature = "hardware"), allow(dead_code))]
pub enum NetworkError {
    /// An `ip` subcommand exited non-zero. Carries the argv, exit code, stderr.
    Command {
        argv: String,
        code: Option<i32>,
        stderr: String,
    },
    /// The `ip` binary could not be spawned (missing / not executable).
    Spawn {
        argv: String,
        source: std::io::Error,
    },
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::Command { argv, code, stderr } => write!(
                f,
                "`{argv}` failed (exit {}): {}",
                match code {
                    Some(c) => c.to_string(),
                    None => "signal".to_string(),
                },
                stderr.trim()
            ),
            NetworkError::Spawn { argv, source } => {
                write!(f, "could not spawn `{argv}`: {source}")
            }
        }
    }
}

impl std::error::Error for NetworkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NetworkError::Spawn { source, .. } => Some(source),
            NetworkError::Command { .. } => None,
        }
    }
}

/// Stops and re-creates the EtherCAT master around a maintenance-mode switch
/// (ISSUE_0010). The L3 NIC switch lives in [`NetworkController`]; this port is
/// the master's lifecycle, deliberately kept separate.
///
/// ethercrab enumerates the bus once per process, so there is no in-process
/// "resume": [`suspend`](Self::suspend) stops the connector's dispatcher and
/// releases the raw socket; [`restart`](Self::restart) asks for a fresh process
/// (the EtherCAT run path exits non-zero so systemd respawns and re-enumerates).
///
/// `Send` for the same reason as [`NetworkController`]: the EtherCAT build drives
/// it from the executor-driven control item, which runs on a worker thread.
pub trait BusController: Send {
    /// Stop pumping the EtherCAT bus and release the raw socket so it stops
    /// flapping recovery on the maintenance link.
    fn suspend(&mut self);
    /// Request the EtherCAT master be brought back up. On the device this means a
    /// process restart (fresh enumeration); the host/sim build no-ops.
    fn restart(&mut self);
}

/// `Send` for the same reason as [`NetworkController`]: the EtherCAT build pets
/// the watchdog from the executor-driven control item (ISSUE_0009).
pub trait Watchdog: Send {
    /// Pet the hardware watchdog. Called only from a completed, healthy scan
    /// cycle so a hung loop lets the device reboot.
    fn pet(&mut self);
}

/// Simulated bus: healthy, knives out, and a periodic "bale full" so the
/// demo (no coupler) exercises the full→wrap→count cycle. Replaces the WAGO
/// connector off the `ethercat` feature.
#[cfg(not(feature = "ethercat"))]
pub struct SimBus {
    start: std::time::Instant,
}

#[cfg(not(feature = "ethercat"))]
impl Default for SimBus {
    fn default() -> Self {
        Self {
            start: std::time::Instant::now(),
        }
    }
}

#[cfg(not(feature = "ethercat"))]
impl BusIo for SimBus {
    fn poll(&mut self) -> Inputs {
        // Full for an 8 s window every 24 s.
        let phase = self.start.elapsed().as_secs() % 24;
        Inputs {
            bale_full: phase >= 16,
            knife_in: false,
            healthy: true,
        }
    }
    fn write(&mut self, _out: Outputs) {}
}

/// No-op bus controller for the host/sim run path: there is no EtherCAT master
/// to stop or recreate. The EtherCAT build uses `WagoBus` instead (ISSUE_0010).
#[cfg(not(feature = "ethercat"))]
pub struct NoopBus;

#[cfg(not(feature = "ethercat"))]
impl BusController for NoopBus {
    fn suspend(&mut self) {}
    fn restart(&mut self) {}
}

/// Returns the demo static IP. Replaces `NetworkMode` off the `netmode-hw` feature.
#[cfg(not(feature = "netmode-hw"))]
#[derive(Default)]
pub struct SimNetwork;

#[cfg(not(feature = "netmode-hw"))]
impl NetworkController for SimNetwork {
    fn enter_ethernet(&mut self) -> Result<Ipv4Addr, NetworkError> {
        Ok(Ipv4Addr::new(192, 168, 1, 102))
    }
    fn enter_ethercat(&mut self) -> Result<(), NetworkError> {
        Ok(())
    }
}

/// Does nothing. Replaces `WatchdogPetter` off the `watchdog-hw` feature.
#[cfg(not(feature = "watchdog-hw"))]
pub struct NoopWatchdog;

#[cfg(not(feature = "watchdog-hw"))]
impl Watchdog for NoopWatchdog {
    fn pet(&mut self) {}
}
