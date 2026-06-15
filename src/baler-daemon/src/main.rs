//! baler-daemon — the always-on safety authority (REQ_0001).
//!
//! Owns the EtherCAT master, the control state machine, the counters, and the
//! network mode. The 10 ms control cycle lives in [`Control::step`]; two run
//! paths drive that same step (ISSUE_0009):
//!
//! * the host/sim path ([`run`]) — a plain `loop { poll; step; write; sleep }`.
//! * the EtherCAT path ([`run_ethercat`]) — the taktora `Executor` run on the
//!   main thread, with the control cycle added as a 10 ms executor item.
//!
//! Hardware ports are selected at build time:
//!
//! * default            → `Sim*` ports (host-buildable, no deps).
//! * `transport`        → publish/receive over iceoryx2 to baler-ui.
//! * `ethercat`         → real WAGO connector (taktora; needs the coupler + NIC).
//! * `watchdog-hw`      → real `/dev/watchdog`.
//! * `netmode-hw`       → real `ip`-based NIC switch.
//! * `hardware`         → all of the above (full device build).
//!
//! A safe on-device demo is `--features transport,watchdog-hw`: Sim bus/net so
//! eth0 is never touched, real watchdog, live state to the UI.

mod control;
mod ports;

#[cfg(feature = "ethercat")]
mod ethercat_io;
#[cfg(feature = "netmode-hw")]
mod network_mode;
#[cfg(feature = "watchdog-hw")]
mod watchdog;

use std::time::Duration;

use baler_ipc::Command;
use control::Control;
#[cfg(not(feature = "ethercat"))]
use ports::BusIo;
use ports::{NetworkController, Watchdog};

const SCAN: Duration = Duration::from_millis(10);
const COUNTER_PATH: &str = "/var/lib/baler/counters";

#[cfg(any(feature = "ethercat", feature = "netmode-hw"))]
const NIC: &str = "eth0";

type DynError = Box<dyn std::error::Error>;

fn main() -> Result<(), DynError> {
    // Capture ethercrab/taktora `log` output (set RUST_LOG, e.g. `info` or
    // `ethercrab=debug`). Defaults to `info` so bring-up is legible out of the box.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    eprintln!("[diag] baler-daemon starting; opening ports…");
    let (net, wd) = build_net_wd()?;

    #[cfg(feature = "ethercat")]
    {
        eprintln!("[diag] ports open; running the EtherCAT executor as the main loop");
        run_ethercat(net, wd)
    }
    #[cfg(not(feature = "ethercat"))]
    {
        eprintln!("[diag] ports open; entering the 10 ms sim scan loop");
        run(net, wd)
    }
}

/// Select the network + watchdog port implementations at build time. Boxed so
/// any feature combination yields one return type. The bus is selected by the
/// run path: `SimBus` for the host loop, the EtherCAT connector for [`run_ethercat`].
fn build_net_wd() -> Result<(Box<dyn NetworkController>, Box<dyn Watchdog>), DynError> {
    #[cfg(feature = "netmode-hw")]
    let net: Box<dyn NetworkController> = Box::new(network_mode::NetworkMode::new(
        NIC,
        std::net::Ipv4Addr::new(192, 168, 1, 102),
        24,
    ));
    #[cfg(not(feature = "netmode-hw"))]
    let net: Box<dyn NetworkController> = Box::new(ports::SimNetwork);

    #[cfg(feature = "watchdog-hw")]
    let wd: Box<dyn Watchdog> =
        Box::new(watchdog::WatchdogPetter::open("/dev/watchdog", Some(15), false)?);
    #[cfg(not(feature = "watchdog-hw"))]
    let wd: Box<dyn Watchdog> = Box::new(ports::NoopWatchdog);

    Ok((net, wd))
}

/// The host/sim 10 ms scan loop: poll the bus, run one [`Control::step`], write
/// the outputs, pet the watchdog, publish the snapshot, sleep. The EtherCAT
/// build drives the *same* [`Control::step`] from an executor item instead — see
/// [`run_ethercat`] — so the control logic is identical on both paths (ISSUE_0009).
#[cfg(not(feature = "ethercat"))]
fn run(mut net: Box<dyn NetworkController>, mut wd: Box<dyn Watchdog>) -> Result<(), DynError> {
    let mut bus = ports::SimBus::default();
    let mut control = Control::new(COUNTER_PATH)?;

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

        // Commands from baler-ui. Real over iceoryx2; none otherwise.
        #[cfg(feature = "transport")]
        let commands: Vec<Command> = cmd_rx.drain().unwrap_or_default();
        #[cfg(not(feature = "transport"))]
        let commands: Vec<Command> = Vec::new();

        let (outputs, snapshot) = control.step(inputs, commands, &mut *net);

        bus.write(outputs);

        // Pet the hardware watchdog from the tail of the loop (REQ_0010).
        wd.pet();

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

/// The EtherCAT 10 ms control loop (ISSUE_0009). Builds the taktora `Executor`,
/// registers the WAGO connector + health pump into it, adds the control cycle
/// as a 10 ms executor item (the same [`Control::step`] the sim loop runs —
/// reading/writing the WAGO process image, pumping commands, publishing the
/// snapshot, petting the watchdog), and runs the executor on the **main thread**.
/// Mirrors the proven `taktora examples/ethercat-wago-coupler`: the earlier
/// background-thread `exec.run()` never scheduled its interval items, so bring-up
/// stalled before the coupler ever reached OP.
#[cfg(feature = "ethercat")]
fn run_ethercat(
    mut net: Box<dyn NetworkController>,
    mut wd: Box<dyn Watchdog>,
) -> Result<(), DynError> {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use taktora_executor::{item_with_triggers, ControlFlow, ExecuteResult, Executor, ExecutorError};

    let mut control = Control::new(COUNTER_PATH)?;

    // ethercrab enumerates the bus ONCE at connector construction and never
    // re-scans. If eth0 has no carrier yet (boot race: the daemon starts a couple
    // of seconds after boot, before the PHY/coupler link is up) that one scan
    // fails with ENETDOWN and the connector is wedged `Down` forever. So wait for
    // the link to come up before building the connector (ISSUE_0009).
    wait_for_link(NIC, Duration::from_secs(20));

    // Transport bridge (ISSUE_0009). taktora's `ExecutableItem` is `Send`, but the
    // iceoryx2 ports baler-ipc wraps hold an `Rc` internally — they are `!Send` and
    // cannot live inside the executor's control item. So the iceoryx2 publisher /
    // subscriber are owned by a dedicated relay thread (created there, never moved
    // across threads) and bridged to the control item with `Send` mpsc channels:
    // outbound `StateSnapshot`s to publish, inbound `Command`s from the UI. The
    // relay is not real-time-critical; the hard 10 ms control cycle and the
    // EtherCAT connector run on the main-thread executor below.
    #[cfg(feature = "transport")]
    let (snap_tx, snap_rx) = std::sync::mpsc::channel::<baler_ipc::StateSnapshot>();
    #[cfg(feature = "transport")]
    let (cmd_tx, cmd_rx) = std::sync::mpsc::channel::<Command>();
    #[cfg(feature = "transport")]
    let _relay = spawn_transport_relay(snap_rx, cmd_tx)?;

    // One worker, exactly like the upstream example that reaches `Up`. The
    // connector's own tokio runtime (rt-multi-thread) drives ethercrab's
    // continuous tx/rx independently of this worker.
    let mut exec = Executor::builder().worker_threads(1).build()?;
    let mut bus = ethercat_io::register(NIC, &mut exec)?;

    // Restart-until-first-Up backstop. Even after the link is up the very first
    // scan can still race a slow coupler; and ethercrab won't re-enumerate. If the
    // bus has not reached `Up` within this deadline, exit non-zero so systemd
    // (`Restart=always`) respawns a fresh scan. Once `Up` is seen, never self-exit
    // — the connector handles Degraded/Up recovery from there (ISSUE_0009).
    const BRINGUP_DEADLINE: Duration = Duration::from_secs(12);
    let bringup_failed = Arc::new(AtomicBool::new(false));
    let failed_flag = Arc::clone(&bringup_failed);

    exec.add(item_with_triggers(
        |d| -> Result<(), ExecutorError> {
            d.interval(SCAN);
            Ok(())
        },
        {
            let started = std::time::Instant::now();
            let mut ever_up = false;
            move |ctx| -> ExecuteResult {
                let inputs = bus.poll();

                if inputs.healthy {
                    ever_up = true;
                } else if !ever_up && started.elapsed() >= BRINGUP_DEADLINE {
                    eprintln!(
                        "[diag] EtherCAT not Up within {:?} — exiting so systemd restarts a \
                         fresh bus scan (ethercrab enumerates once at init)",
                        BRINGUP_DEADLINE
                    );
                    failed_flag.store(true, Ordering::Release);
                    ctx.stop_executor();
                    return Ok(ControlFlow::Continue);
                }

                // Commands from baler-ui via the relay. None without transport.
                #[cfg(feature = "transport")]
                let commands: Vec<Command> = cmd_rx.try_iter().collect();
                #[cfg(not(feature = "transport"))]
                let commands: Vec<Command> = Vec::new();

                let (outputs, snapshot) = control.step(inputs, commands, &mut *net);

                bus.write(outputs);

                // Pet the hardware watchdog from the tail of the cycle (REQ_0010).
                wd.pet();

                #[cfg(feature = "transport")]
                {
                    let _ = snap_tx.send(snapshot); // relay publishes; err only if relay gone
                }
                #[cfg(not(feature = "transport"))]
                {
                    let _ = &snapshot; // built for parity; published only with transport
                }

                Ok(ControlFlow::Continue)
            }
        },
    ))?;

    eprintln!("[diag] EtherCAT executor running on the main thread");
    exec.run()?;
    if bringup_failed.load(Ordering::Acquire) {
        return Err("EtherCAT bring-up timed out; restarting for a fresh bus scan".into());
    }
    Ok(())
}

/// Block until `nic` reports a usable link (operstate `up` or carrier `1`), or
/// `timeout` elapses. Best-effort `ip link set <nic> up` first, since raw EtherCAT
/// frames need the interface administratively up. Returns either way — the
/// restart-until-Up backstop covers the timeout case (ISSUE_0009).
#[cfg(feature = "ethercat")]
fn wait_for_link(nic: &str, timeout: Duration) {
    let _ = std::process::Command::new("ip")
        .args(["link", "set", nic, "up"])
        .status();

    let started = std::time::Instant::now();
    let operstate = format!("/sys/class/net/{nic}/operstate");
    let carrier = format!("/sys/class/net/{nic}/carrier");
    loop {
        let up = std::fs::read_to_string(&operstate).map(|s| s.trim() == "up").unwrap_or(false);
        let has_carrier = std::fs::read_to_string(&carrier).map(|s| s.trim() == "1").unwrap_or(false);
        if up || has_carrier {
            eprintln!("[diag] {nic} link ready (operstate up={up}, carrier={has_carrier}) after {:?}", started.elapsed());
            return;
        }
        if started.elapsed() >= timeout {
            eprintln!("[diag] {nic} still no carrier after {:?}; proceeding (restart-until-up will retry)", started.elapsed());
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Relay thread bridging the executor-driven control cycle to iceoryx2. Owns the
/// `!Send` iceoryx2 ports (built here, never moved off this thread): forwards UI
/// `Command`s inbound and publishes the newest `StateSnapshot` outbound, ~every
/// scan period (ISSUE_0009).
#[cfg(all(feature = "ethercat", feature = "transport"))]
fn spawn_transport_relay(
    snap_rx: std::sync::mpsc::Receiver<baler_ipc::StateSnapshot>,
    cmd_tx: std::sync::mpsc::Sender<Command>,
) -> Result<std::thread::JoinHandle<()>, DynError> {
    use std::sync::mpsc::TryRecvError;

    let handle = std::thread::Builder::new()
        .name("baler-transport".into())
        .spawn(move || {
            let node = match baler_ipc::transport::build_node() {
                Ok(n) => n,
                Err(e) => return eprintln!("[transport] build_node failed: {e}"),
            };
            let state_pub = match baler_ipc::transport::StatePublisher::new(&node) {
                Ok(p) => p,
                Err(e) => return eprintln!("[transport] state publisher failed: {e}"),
            };
            let cmd_rx_iox = match baler_ipc::transport::CommandReceiver::new(&node) {
                Ok(r) => r,
                Err(e) => return eprintln!("[transport] command receiver failed: {e}"),
            };

            loop {
                // Inbound: UI commands -> control item.
                for cmd in cmd_rx_iox.drain().unwrap_or_default() {
                    if cmd_tx.send(cmd).is_err() {
                        return; // control loop gone; tear down the relay
                    }
                }
                // Outbound: publish only the newest snapshot (coalesce backlog).
                let mut latest = None;
                loop {
                    match snap_rx.try_recv() {
                        Ok(s) => latest = Some(s),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                if let Some(snapshot) = latest {
                    let _ = state_pub.publish(&snapshot);
                }
                std::thread::sleep(SCAN);
            }
        })?;
    Ok(handle)
}
