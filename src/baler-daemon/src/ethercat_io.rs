//! Real `BusIo` implementation (REQ_0015): the taktora ethercat-wago connector
//! for a WAGO 750-354 coupler (+750-430 8 DI, +750-530 8 DO).
//!
//! Behind the `hardware` cargo feature (gated at the `mod` site in main.rs).
//!
//! # Architecture: bridging a cyclic framework to a synchronous scan loop
//!
//! taktora is cyclic: the ethercat connector's PDI exchange is pumped by
//! executor items registered via `Connector::register_with`, which only run
//! while `Executor::run()` is live. There is no public per-cycle `step()` on the
//! `Connector` trait to drive a single cycle by hand. So we run the taktora
//! `Executor` on a dedicated background thread; the daemon's synchronous scan
//! loop talks to the running connector through the iceoryx2
//! `ChannelReader`/`ChannelWriter` handles plus a shared health snapshot kept
//! current by a health-pump item — mirroring the upstream
//! `examples/ethercat-wago-coupler`.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use taktora_connector_core::{ChannelDescriptor, ConnectorHealthKind, PayloadCodec};
use taktora_connector_ethercat::{
    connector::EthercatState, declare_pdu_storage, EthercatConnector, EthercatConnectorOptions,
    EthercatRouting, EthercrabBusDriver, PdoDirection, SmWatchdog, SubDeviceMap,
};
use taktora_connector_host::Connector;
use taktora_connector_transport_iox::{ChannelReader, ChannelWriter};
use taktora_executor::{item_with_triggers, ControlFlow, ExecuteResult, Executor, ExecutorError};

use crate::ports::{BusIo, Inputs, Outputs};

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
const BIT_KNIFE: u8 = 1; // DO2

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

// --- EtherCatIo. ------------------------------------------------------------

fn kind_to_u8(k: ConnectorHealthKind) -> u8 {
    match k {
        ConnectorHealthKind::Up => 0,
        ConnectorHealthKind::Connecting => 1,
        ConnectorHealthKind::Degraded => 2,
        ConnectorHealthKind::Down => 3,
    }
}

/// EtherCAT IO layer for the WAGO coupler. Implements [`BusIo`].
pub struct EtherCatIo {
    reader: ChannelReader<u8, RawByteCodec, N>,
    writer: ChannelWriter<u8, RawByteCodec, N>,
    health: Arc<AtomicU8>,
    last_input: u8,
    last_output: Option<u8>,
    _executor: ExecutorHandle,
}

/// RAII guard that stops the background executor and joins its thread on drop.
struct ExecutorHandle {
    stop: Arc<std::sync::atomic::AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for ExecutorHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl EtherCatIo {
    /// Build the driver/connector for the WAGO coupler on NIC `nic`, program the
    /// 50 ms SM watchdog and fixed PDO map, register the connector with a
    /// single-worker executor, and spawn that executor on a background thread.
    ///
    /// Requires `CAP_NET_RAW + CAP_NET_ADMIN`.
    pub fn new(nic: &str) -> Result<Self, EtherCatIoError> {
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

        let mut exec = Executor::builder().worker_threads(1).build()?;
        connector.register_with(&mut exec)?;

        // Health pump (competing consumer: exactly one item drains the events).
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
                    health_pump.store(kind_to_u8(event.to.kind()), Ordering::Release);
                }
                Ok(ControlFlow::Continue)
            },
        ))?;

        // Cooperative stop item so dropping EtherCatIo tears the bus down.
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_item = Arc::clone(&stop);
        exec.add(item_with_triggers(
            |d| -> Result<(), ExecutorError> {
                d.interval(SCAN_INTERVAL);
                Ok(())
            },
            move |ctx| -> ExecuteResult {
                if stop_item.load(Ordering::Acquire) {
                    ctx.stop_executor();
                }
                Ok(ControlFlow::Continue)
            },
        ))?;

        let join = std::thread::Builder::new()
            .name("ethercat-wago".into())
            .spawn(move || {
                let _ = exec.run();
            })
            .expect("spawn ethercat executor thread");

        Ok(Self {
            reader,
            writer,
            health,
            last_input: 0,
            last_output: None,
            _executor: ExecutorHandle {
                stop,
                join: Some(join),
            },
        })
    }

    fn is_healthy(&self) -> bool {
        self.health.load(Ordering::Acquire) == kind_to_u8(ConnectorHealthKind::Up)
    }
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

impl BusIo for EtherCatIo {
    fn poll(&mut self) -> Inputs {
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

    fn write(&mut self, out: Outputs) {
        let mut byte = 0u8;
        byte = set_bit(byte, BIT_WRAP, out.wrap);
        byte = set_bit(byte, BIT_KNIFE, out.knife);
        if self.last_output != Some(byte) && self.writer.send(&byte).is_ok() {
            self.last_output = Some(byte);
        }
    }
}
