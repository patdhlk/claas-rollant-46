//! baler-daemon — the always-on safety authority (REQ_0001).
//!
//! Owns the EtherCAT master, the control state machine, the counters, and the
//! network mode. Runs a fixed 10 ms scan loop wiring the pure `baler-core`
//! modules to hardware ports selected at build time:
//!
//! * default            → `Sim*` ports (host-buildable, no deps).
//! * `transport`        → publish/receive over iceoryx2 to baler-ui.
//! * `ethercat`         → real `EtherCatIo` (taktora; needs the coupler + NIC).
//! * `watchdog-hw`      → real `/dev/watchdog`.
//! * `netmode-hw`       → real `ip`-based NIC switch.
//! * `hardware`         → all of the above (full device build).
//!
//! A safe on-device demo is `--features transport,watchdog-hw`: Sim bus/net so
//! eth0 is never touched, real watchdog, live state to the UI.

mod ports;

#[cfg(feature = "ethercat")]
mod ethercat_io;
#[cfg(feature = "netmode-hw")]
mod network_mode;
#[cfg(feature = "watchdog-hw")]
mod watchdog;

use std::net::Ipv4Addr;
use std::time::Duration;

use baler_core::counter::CounterStore;
use baler_core::input_conditioner::Debouncer;
use baler_core::pulse::{PulseEngine, PulseEvent};
use baler_core::state::{Action, BalerState};
use baler_ipc::{Command, KnifePos, StateSnapshot};
use ports::{BusIo, NetworkController, Outputs, Watchdog};

const SCAN: Duration = Duration::from_millis(10);
const DEBOUNCE: u8 = 3; // ~30 ms at the 10 ms scan
const PULSE: Duration = Duration::from_secs(5);
const COUNTER_PATH: &str = "/var/lib/baler/counters";

#[cfg(any(feature = "ethercat", feature = "netmode-hw"))]
const NIC: &str = "eth0";

type DynError = Box<dyn std::error::Error>;

fn main() -> Result<(), DynError> {
    let (bus, net, wd) = build_ports()?;
    run(bus, net, wd)
}

/// Select the port implementations at build time. Boxed so any feature
/// combination yields one return type.
fn build_ports() -> Result<
    (
        Box<dyn BusIo>,
        Box<dyn NetworkController>,
        Box<dyn Watchdog>,
    ),
    DynError,
> {
    #[cfg(feature = "ethercat")]
    let bus: Box<dyn BusIo> = Box::new(ethercat_io::EtherCatIo::new(NIC)?);
    #[cfg(not(feature = "ethercat"))]
    let bus: Box<dyn BusIo> = Box::new(ports::SimBus::default());

    #[cfg(feature = "netmode-hw")]
    let net: Box<dyn NetworkController> =
        Box::new(network_mode::NetworkMode::new(NIC, Ipv4Addr::new(192, 168, 1, 102), 24));
    #[cfg(not(feature = "netmode-hw"))]
    let net: Box<dyn NetworkController> = Box::new(ports::SimNetwork);

    #[cfg(feature = "watchdog-hw")]
    let wd: Box<dyn Watchdog> =
        Box::new(watchdog::WatchdogPetter::open("/dev/watchdog", Some(15), false)?);
    #[cfg(not(feature = "watchdog-hw"))]
    let wd: Box<dyn Watchdog> = Box::new(ports::NoopWatchdog);

    Ok((bus, net, wd))
}

/// The 10 ms scan loop. Boxed trait objects so the body is identical for every
/// feature combination.
fn run(
    mut bus: Box<dyn BusIo>,
    mut net: Box<dyn NetworkController>,
    mut wd: Box<dyn Watchdog>,
) -> Result<(), DynError> {
    let mut counters = CounterStore::load(COUNTER_PATH)?;
    let mut state = BalerState::new();
    let mut wrap = PulseEngine::new(PULSE);
    let mut knife = PulseEngine::new(PULSE);
    let mut full_db = Debouncer::new(DEBOUNCE, false);
    let mut knife_db = Debouncer::new(DEBOUNCE, false);
    let mut last_ip: Option<Ipv4Addr> = None;

    // iceoryx2 transport to baler-ui (decentralized; ISSUE_0002). The node must
    // outlive the publisher/subscriber, so it is bound for the whole loop.
    #[cfg(feature = "transport")]
    let _node = baler_ipc::transport::build_node()?;
    #[cfg(feature = "transport")]
    let state_pub = baler_ipc::transport::StatePublisher::new(&_node)?;
    #[cfg(feature = "transport")]
    let cmd_rx = baler_ipc::transport::CommandReceiver::new(&_node)?;

    loop {
        let inputs = bus.poll();
        state.on_bus_health(inputs.healthy);
        full_db.update(inputs.bale_full);
        knife_db.update(inputs.knife_in);
        if inputs.healthy {
            state.on_inputs(full_db.level(), knife_db.level());
        }

        // Commands from baler-ui. Real over iceoryx2; none otherwise.
        #[cfg(feature = "transport")]
        let commands: Vec<Command> = cmd_rx.drain().unwrap_or_default();
        #[cfg(not(feature = "transport"))]
        let commands: Vec<Command> = Vec::new();

        for cmd in commands {
            let any_active = wrap.is_active() || knife.is_active();
            match state.handle(cmd, any_active) {
                Ok(Action::FireWrap) => {
                    wrap.trigger();
                }
                Ok(Action::FireKnife) => {
                    knife.trigger();
                }
                Ok(Action::ResetSession) => counters.reset_session()?,
                Ok(Action::ResetTotal) => counters.reset_total()?,
                Ok(Action::SwitchToEthernet) => last_ip = net.enter_ethernet().ok(),
                Ok(Action::SwitchToEthercat) => {
                    let _ = net.enter_ethercat();
                    last_ip = None;
                }
                Err(_reject) => { /* surface "busy / not ready" on the UI */ }
            }
        }

        // Independent pulses; a wrap counts only on clean completion (REQ_0004).
        if let PulseEvent::Completed = wrap.tick(SCAN, inputs.healthy) {
            counters.increment_wrap()?;
        }
        let _ = knife.tick(SCAN, inputs.healthy);

        bus.write(Outputs {
            wrap: wrap.output(),
            knife: knife.output(),
        });

        // Pet the hardware watchdog from the healthy tail of the loop (REQ_0010).
        wd.pet();

        let counts = counters.snapshot();
        let snapshot = StateSnapshot {
            mode: state.mode(),
            bale_full: state.bale_full(),
            knife: match state.knife_in() {
                None => KnifePos::Unknown,
                Some(true) => KnifePos::In,
                Some(false) => KnifePos::Out,
            },
            wrap_armed: state.wrap_armed(),
            wrap_active: wrap.is_active(),
            knife_active: knife.is_active(),
            session: counts.session,
            total: counts.total,
            ip: last_ip.map(|i| i.octets()).unwrap_or([0, 0, 0, 0]),
            ip_valid: last_ip.is_some(),
        };

        #[cfg(feature = "transport")]
        {
            let _ = state_pub.publish(&snapshot);
        }
        #[cfg(not(feature = "transport"))]
        {
            let _ = &snapshot; // built for parity; published only with transport
        }

        std::thread::sleep(SCAN);
    }
}
