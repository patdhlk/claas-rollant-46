//! Real EtherCAT IO (REQ_0015): the taktora ethercat-wago connector for a
//! WAGO 750-354 coupler (+750-430 8 DI, +750-530 8 DO).
//!
//! Behind the `ethercat` cargo feature (gated at the `mod` site in main.rs).
//!
//! # Architecture: the connector IS the main loop (ISSUE_0009)
//!
//! taktora is cyclic: the ethercat connector's PDI exchange is pumped by
//! executor items registered via `Connector::register_with`, which only run
//! while `Executor::run()` is live. Mirroring the upstream
//! `examples/ethercat-wago-coupler`, the daemon builds the connector here,
//! registers it (and a health pump) into the caller's [`Executor`] via
//! [`register`], and the caller runs that executor as its **main loop** —
//! adding the 10 ms control cycle as another executor item that reads/writes
//! the WAGO process image through the returned [`WagoBus`]. The earlier
//! background-thread `exec.run()` never scheduled its interval items, so
//! bring-up never advanced past the construction-time frame; running the
//! executor on the main thread is the fix.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use taktora_connector_core::{ChannelDescriptor, ConnectorHealthKind, PayloadCodec};
use taktora_connector_ethercat::{
    connector::EthercatState, declare_pdu_storage, EthercatConnector, EthercatConnectorOptions,
    EthercatRouting, EthercrabBusDriver, PdoDirection, SmWatchdog, SubDeviceMap,
};
use taktora_connector_host::Connector;
use taktora_connector_transport_iox::{ChannelReader, ChannelWriter};
use taktora_executor::{item_with_triggers, ControlFlow, ExecuteResult, Executor, ExecutorError};

use crate::ports::{BusController, Inputs, Outputs};

// --- Hardware constants — WAGO 750-354 + 750-430 (DI) + 750-530 (DO). --------

/// WAGO 750-354 configured station address (the only SubDevice on the bus).
const SUBDEV: u16 = 0x1000;
/// LRW working counter for the single read+write coupler (+2 write, +1 read).
const EXPECTED_WKC: u16 = 3;
/// SM watchdog window: ≤ FTTI/2 = 50 ms (the coupler carries outputs).
const SM_WATCHDOG_US: u32 = 50_000;
/// Bus PDU cycle time.
const CYCLE_TIME: Duration = Duration::from_millis(2);
/// Logical scan interval for the background pump items (10 ms).
const SCAN_INTERVAL: Duration = Duration::from_millis(10);

/// Input byte offset in the Tx image: 4-byte coupler header → bit 32 (byte 4).
const DI_BIT_OFFSET: u32 = 32;
/// Output byte offset in the Rx image: mirrors the input side.
const DO_BIT_OFFSET: u32 = 32;
const DI_BITS: u16 = 8;
const DO_BITS: u16 = 8;

// IO map within the single data byte. bit0 = channel1, bit1 = channel2.
const BIT_BALE_FULL: u8 = 0; // DI1
const BIT_KNIFE_IN: u8 = 1; // DI2 (true = knives in)
const BIT_WRAP: u8 = 0; // DO1
const BIT_KNIVES_IN: u8 = 1; // DO2 (fired when ch2/DI2 is true)
const BIT_KNIVES_OUT: u8 = 2; // DO3 (fired when ch2/DI2 is false)

/// iceoryx2 service buffer slots.
const N: usize = 256;
/// Topology bounds for `EthercrabBusDriver`.
const MAX_SUBDEVICES: usize = 16;
const MAX_PDI: usize = 256;

const IN_CHANNEL: &str = "ethercat.wago.750-430.inputs";
const OUT_CHANNEL: &str = "ethercat.wago.750-530.outputs";

/// Static per-SubDevice map. PDO-entry lists empty: the 750-354 PDO assignment
/// is fixed, so no SDO re-assignment is possible.
static PDO_MAP: &[SubDeviceMap] = &[SubDeviceMap::new(SUBDEV, &[], &[], EXPECTED_WKC)
    .with_sm_watchdog(SmWatchdog::from_timeout_us(SM_WATCHDOG_US))];

declare_pdu_storage!(WAGO_PDU_STORAGE);

// --- RawByteCodec — round-trips a single `u8` to/from one wire byte. ---------

#[derive(Debug, Clone, Copy, Default)]
struct RawByteCodec;

impl PayloadCodec for RawByteCodec {
    fn format_name(&self) -> &'static str {
        "raw-byte"
    }

    fn encode<T>(&self, value: &T, buf: &mut [u8]) -> Result<usize, taktora_connector_core::ConnectorError>
    where
        T: serde::Serialize,
    {
        use taktora_connector_core::ConnectorError;
        let v = serde_json::to_value(value).map_err(|e| ConnectorError::codec("raw-byte", e))?;
        let byte: u8 = v
            .as_u64()
            .ok_or_else(|| {
                ConnectorError::codec("raw-byte", std::io::Error::other("expected u8-like integer"))
            })?
            .try_into()
            .map_err(|_| {
                ConnectorError::codec("raw-byte", std::io::Error::other("value does not fit in u8"))
            })?;
        if buf.is_empty() {
            return Err(ConnectorError::PayloadOverflow { actual: 1, max: 0 });
        }
        buf[0] = byte;
        Ok(1)
    }

    fn decode<T>(&self, buf: &[u8]) -> Result<T, taktora_connector_core::ConnectorError>
    where
        T: serde::de::DeserializeOwned,
    {
        use taktora_connector_core::ConnectorError;
        if buf.is_empty() {
            return Err(ConnectorError::codec(
                "raw-byte",
                std::io::Error::other("empty buffer; expected exactly 1 byte"),
            ));
        }
        let v = serde_json::Value::Number(serde_json::Number::from(buf[0]));
        serde_json::from_value(v).map_err(|e| ConnectorError::codec("raw-byte", e))
    }
}

// --- Error type. ------------------------------------------------------------

/// Errors raised while constructing [`EtherCatIo`]. The steady-state path never
/// returns these — it degrades to a safe, unhealthy reading instead of panicking.
#[derive(Debug)]
pub enum EtherCatIoError {
    Connector(taktora_connector_core::ConnectorError),
    Executor(ExecutorError),
}

impl std::fmt::Display for EtherCatIoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connector(e) => write!(f, "ethercat connector setup failed: {e}"),
            Self::Executor(e) => write!(f, "executor setup failed: {e}"),
        }
    }
}

impl std::error::Error for EtherCatIoError {}

impl From<taktora_connector_core::ConnectorError> for EtherCatIoError {
    fn from(e: taktora_connector_core::ConnectorError) -> Self {
        Self::Connector(e)
    }
}

impl From<ExecutorError> for EtherCatIoError {
    fn from(e: ExecutorError) -> Self {
        Self::Executor(e)
    }
}

// --- WagoBus. ----------------------------------------------------------------

fn kind_to_u8(k: ConnectorHealthKind) -> u8 {
    match k {
        ConnectorHealthKind::Up => 0,
        ConnectorHealthKind::Connecting => 1,
        ConnectorHealthKind::Degraded => 2,
        ConnectorHealthKind::Down => 3,
    }
}

/// Concrete connector type for the single WAGO coupler on this bus.
type WagoConnector = EthercatConnector<EthercrabBusDriver<MAX_SUBDEVICES, MAX_PDI>, RawByteCodec>;

/// The WAGO process-image IO handles, driven from the caller's executor.
///
/// Owns the connector so it stays alive for as long as the executor runs (the
/// registered PDI driving holds no separate handle to it). The daemon moves the
/// whole `WagoBus` into its 10 ms control item and calls [`poll`](Self::poll) /
/// [`write`](Self::write) each cycle — the executor-driven analogue of the old
/// synchronous `BusIo`.
pub struct WagoBus {
    /// Kept alive for the executor's lifetime; **dropped** on
    /// [`suspend`](BusController::suspend) to tear the bus down (ISSUE_0010).
    /// `None` once suspended — the bus is then quiet and its ports are gone.
    connector: Option<WagoConnector>,
    reader: ChannelReader<u8, RawByteCodec, N>,
    writer: ChannelWriter<u8, RawByteCodec, N>,
    health: Arc<AtomicU8>,
    last_input: u8,
    last_output: Option<u8>,
    /// Set by [`restart`](BusController::restart); the run loop drains it via
    /// [`take_restart_request`](Self::take_restart_request) and exits for a fresh
    /// process (ISSUE_0010).
    restart_requested: bool,
}

/// Build the WAGO connector on NIC `nic`, program the 50 ms SM watchdog and
/// fixed PDO map, register the connector **and a health pump** into `exec`, and
/// return the IO handles. The caller adds its control item and runs `exec` as
/// the main loop (ISSUE_0009).
///
/// Requires `CAP_NET_RAW + CAP_NET_ADMIN`.
pub fn register(nic: &str, exec: &mut Executor) -> Result<WagoBus, EtherCatIoError> {
    let opts = EthercatConnectorOptions::builder()
        .pdo_map(PDO_MAP)
        .network_interface(nic)
        .cycle_time(CYCLE_TIME)
        .build();

    let driver =
        EthercrabBusDriver::<MAX_SUBDEVICES, MAX_PDI>::new(&WAGO_PDU_STORAGE, opts.clone())?;

    let state = Arc::new(EthercatState::new(opts.clone()));
    let mut connector = EthercatConnector::new(state, driver, RawByteCodec)?;

    let in_routing = EthercatRouting::new(SUBDEV, PdoDirection::Tx, DI_BIT_OFFSET, DI_BITS);
    let in_desc = ChannelDescriptor::<EthercatRouting, N>::new(IN_CHANNEL, in_routing)?;
    let reader = connector.create_reader::<u8, N>(&in_desc)?;

    let out_routing = EthercatRouting::new(SUBDEV, PdoDirection::Rx, DO_BIT_OFFSET, DO_BITS);
    let out_desc = ChannelDescriptor::<EthercatRouting, N>::new(OUT_CHANNEL, out_routing)?;
    let writer = connector.create_writer::<u8, N>(&out_desc)?;

    // Register the connector's cyclic PDI driving into the caller's executor.
    // The example reaches `Up` with this exact shape on the main thread.
    connector.register_with(exec)?;

    // Health pump (competing consumer: exactly one item drains the events). It
    // owns the shared health snapshot the control item reads each cycle.
    let health = Arc::new(AtomicU8::new(kind_to_u8(connector.health().kind())));
    let health_pump = Arc::clone(&health);
    let health_sub = connector.subscribe_health();
    exec.add(item_with_triggers(
        |d| -> Result<(), ExecutorError> {
            d.interval(SCAN_INTERVAL);
            Ok(())
        },
        move |_ctx| -> ExecuteResult {
            while let Ok(Some(event)) = health_sub.try_next() {
                // Log the full target state (carries the Down/Degraded reason,
                // e.g. "bring-up failed: … Timeout(Pdu)") — the key bring-up
                // diagnostic, matching the upstream example.
                eprintln!("[diag] connector health -> {:?}", event.to);
                health_pump.store(kind_to_u8(event.to.kind()), Ordering::Release);
            }
            Ok(ControlFlow::Continue)
        },
    ))?;

    Ok(WagoBus {
        connector: Some(connector),
        reader,
        writer,
        health,
        last_input: 0,
        last_output: None,
        restart_requested: false,
    })
}

#[inline]
fn bit(byte: u8, idx: u8) -> bool {
    (byte >> idx) & 1 == 1
}

#[inline]
fn set_bit(byte: u8, idx: u8, on: bool) -> u8 {
    if on {
        byte | (1 << idx)
    } else {
        byte & !(1 << idx)
    }
}

impl WagoBus {
    fn is_healthy(&self) -> bool {
        self.connector.is_some()
            && self.health.load(Ordering::Acquire) == kind_to_u8(ConnectorHealthKind::Up)
    }

    /// Drain the input channel and decode the latest DI1/DI2 + connector health
    /// for this scan cycle.
    pub fn poll(&mut self) -> Inputs {
        // Suspended (Ethernet maintenance mode): the connector is torn down, so the
        // bus is down and its PDI ports are gone — report not-healthy without
        // touching them (ISSUE_0010).
        if self.connector.is_none() {
            return Inputs {
                bale_full: false,
                knife_in: false,
                healthy: false,
            };
        }
        loop {
            match self.reader.try_recv() {
                Ok(Some(env)) => self.last_input = env.value,
                Ok(None) => break,
                Err(_) => break,
            }
        }
        let byte = self.last_input;
        Inputs {
            bale_full: bit(byte, BIT_BALE_FULL),
            knife_in: bit(byte, BIT_KNIFE_IN),
            healthy: self.is_healthy(),
        }
    }

    /// Encode and write DO1/DO2/DO3 for this scan cycle (only on change).
    pub fn write(&mut self, out: Outputs) {
        if self.connector.is_none() {
            return; // suspended: the bus is torn down, nothing to drive
        }
        let mut byte = 0u8;
        byte = set_bit(byte, BIT_WRAP, out.wrap);
        byte = set_bit(byte, BIT_KNIVES_IN, out.knives_in);
        byte = set_bit(byte, BIT_KNIVES_OUT, out.knives_out);
        if self.last_output != Some(byte) && self.writer.send(&byte).is_ok() {
            self.last_output = Some(byte);
        }
    }

    /// Drain a pending restart request (ISSUE_0010). The run loop calls this each
    /// cycle; when `true`, it exits non-zero so systemd respawns a fresh process
    /// that re-enumerates the bus (ethercrab enumerates once per process).
    pub fn take_restart_request(&mut self) -> bool {
        std::mem::take(&mut self.restart_requested)
    }
}

impl BusController for WagoBus {
    /// Tear the EtherCAT master down so it stops driving (and flapping recovery
    /// on) the bus and releases the raw socket on the maintenance link.
    ///
    /// `stop_dispatcher()` alone is **not** enough (ISSUE_0010, found on-device
    /// 2026-06-15): once the coupler drops, the taktora runner parks inside
    /// `recover_per_policy` — an infinite reconnect-backoff loop that never
    /// re-checks the stop flag — so the flapping continues regardless. Dropping
    /// the connector drops its `EthercatGateway`, whose `Drop` shuts down the
    /// tokio runtime and aborts the dispatcher + ethercrab tx/rx wherever they
    /// are parked, which closes the raw socket. The drop is moved onto a detached
    /// thread so the gateway's blocking `shutdown_timeout` can never stall the
    /// 10 ms control cycle (and never starves the watchdog). Idempotent.
    fn suspend(&mut self) {
        if let Some(connector) = self.connector.take() {
            eprintln!("[diag] EtherCAT master suspended (Ethernet maintenance mode)");
            connector.stop_dispatcher();
            std::thread::spawn(move || drop(connector));
        }
    }

    /// Flag a return to EtherCAT. ethercrab cannot re-enumerate in-process, so the
    /// run loop drains this and exits for a fresh process (ISSUE_0010).
    fn restart(&mut self) {
        eprintln!("[diag] EtherCAT return requested; exiting for a fresh bus scan");
        self.restart_requested = true;
    }
}
