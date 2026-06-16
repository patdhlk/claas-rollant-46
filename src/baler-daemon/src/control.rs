//! The shared 10 ms control cycle (ISSUE_0009).
//!
//! [`Control`] owns every stateful piece of the daemon's control logic — the
//! mode state machine, both output pulses, both input debouncers, the persisted
//! counters, and the last static IP — behind a single [`Control::step`] method.
//! Extracting it lets the two run paths (the host/sim manual loop and the
//! EtherCAT executor item) drive identical logic: only the IO transport around
//! `step` differs.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;

use baler_core::counter::CounterStore;
use baler_core::input_conditioner::Debouncer;
use baler_core::pulse::{PulseEngine, PulseEvent};
use baler_core::state::{Action, BalerState};
use baler_core::{Command, KnifePos, StateSnapshot};

use crate::ports::{BusController, Inputs, NetworkController, Outputs};

const SCAN: Duration = Duration::from_millis(10);
const DEBOUNCE: u8 = 3; // ~30 ms at the 10 ms scan
const PULSE: Duration = Duration::from_secs(5);
/// Bale-full attention latch: hold "FULL" for >= 20 s after a true DI1 so the
/// operator notices even when looking away (REQ_0017). 20 s / 10 ms = 2000 cycles.
const FULL_LATCH_CYCLES: u32 = 2000;
/// Manual-IO command watchdog: outputs de-energize and the mode exits if no
/// `ManualIo` arrives within this many cycles — 30 × 10 ms = 300 ms (REQ_0018).
const MANUAL_IO_WATCHDOG: u32 = 30;

/// Owns the control-cycle state. One `step` per 10 ms scan.
pub struct Control {
    counters: CounterStore,
    state: BalerState,
    wrap: PulseEngine,
    knife: PulseEngine,
    full_db: Debouncer,
    knife_db: Debouncer,
    last_ip: Option<Ipv4Addr>,
    /// Bale-full attention latch: cycles remaining that "FULL" stays shown after a
    /// rising debounced DI1, cleared early by a wrap (REQ_0017).
    full_latch_remaining: u32,
    /// Raw (un-debounced) discrete inputs, surfaced for the IO test screen (REQ_0018).
    last_di1: bool,
    last_di2: bool,
    /// Manual-IO mode (REQ_0018): cycles remaining before the command watchdog
    /// fails safe. >0 means the IO test screen is driving the outputs directly;
    /// `manual_wrap`/`manual_knife` are the latest operator-held bits.
    manual_io_remaining: u32,
    manual_wrap: bool,
    manual_knife: bool,
}

impl Control {
    /// Build the control cycle, loading persisted counters from `counter_path`.
    pub fn new(counter_path: impl Into<PathBuf>) -> std::io::Result<Self> {
        Ok(Self {
            counters: CounterStore::load(counter_path.into())?,
            state: BalerState::new(),
            wrap: PulseEngine::new(PULSE),
            knife: PulseEngine::new(PULSE),
            full_db: Debouncer::new(DEBOUNCE, false),
            knife_db: Debouncer::new(DEBOUNCE, false),
            last_ip: None,
            full_latch_remaining: 0,
            last_di1: false,
            last_di2: false,
            manual_io_remaining: 0,
            manual_wrap: false,
            manual_knife: false,
        })
    }

    /// Advance one 10 ms scan cycle: fold in bus health and conditioned inputs,
    /// apply operator `commands` (driving the network controller *and* the
    /// EtherCAT-master `bus` controller for mode switches — ISSUE_0010), tick the
    /// pulses, and return the commanded [`Outputs`] plus the [`StateSnapshot`] to
    /// publish.
    pub fn step(
        &mut self,
        inputs: Inputs,
        commands: Vec<Command>,
        net: &mut dyn NetworkController,
        bus: &mut dyn BusController,
    ) -> (Outputs, StateSnapshot) {
        self.state.on_bus_health(inputs.healthy);
        // Raw inputs for the IO test screen (un-debounced; REQ_0018).
        self.last_di1 = inputs.bale_full;
        self.last_di2 = inputs.knife_in;
        self.full_db.update(inputs.bale_full);
        self.knife_db.update(inputs.knife_in);
        if inputs.healthy {
            self.state.on_inputs(self.full_db.level(), self.knife_db.level());
        }

        // Bale-full attention latch (REQ_0017): (re)arm while debounced DI1 is
        // true, otherwise count down. `full_latched` (in the snapshot) stays true
        // for >= 20 s after the last true sample so the operator notices.
        if inputs.healthy && self.full_db.level() {
            self.full_latch_remaining = FULL_LATCH_CYCLES;
        } else if self.full_latch_remaining > 0 {
            self.full_latch_remaining -= 1;
        }

        let mut got_manual = false;
        for cmd in commands {
            // IO test screen (REQ_0018): intercept before the state machine. Its
            // presence (re)arms the watchdog; while in manual-IO mode the normal
            // control state machine is suspended.
            if let Command::ManualIo { wrap, knife } = cmd {
                got_manual = true;
                self.manual_wrap = wrap;
                self.manual_knife = knife;
                continue;
            }
            if self.manual_io_remaining > 0 {
                continue; // manual-IO mode: ignore machine/mode commands
            }
            let any_active = self.wrap.is_active() || self.knife.is_active();
            match self.state.handle(cmd, any_active) {
                Ok(Action::FireWrap) => {
                    self.wrap.trigger();
                    // The operator handled the full bale — clear the attention
                    // latch so it stops nagging (REQ_0017).
                    self.full_latch_remaining = 0;
                }
                Ok(Action::FireKnife) => {
                    self.knife.trigger();
                }
                Ok(Action::ResetSession) => {
                    if let Err(e) = self.counters.reset_session() {
                        eprintln!("[control] session reset persist failed: {e}");
                    }
                }
                Ok(Action::ResetTotal) => {
                    if let Err(e) = self.counters.reset_total() {
                        eprintln!("[control] total reset persist failed: {e}");
                    }
                }
                Ok(Action::SwitchToEthernet) => {
                    // Stop the EtherCAT master first so it stops flapping recovery
                    // and releases the raw socket, then bring up the L3 NIC (ISSUE_0010).
                    bus.suspend();
                    self.last_ip = net.enter_ethernet().ok();
                }
                Ok(Action::SwitchToEthercat) => {
                    let _ = net.enter_ethercat();
                    // ethercrab enumerates once per process, so the master can only
                    // come back via a fresh process — ask for the restart (ISSUE_0010).
                    bus.restart();
                    self.last_ip = None;
                }
                Err(_reject) => { /* surface "busy / not ready" on the UI */ }
            }
        }

        // Manual-IO watchdog (REQ_0018): a fresh `ManualIo` (re)arms it; otherwise
        // it counts down and fails safe at zero. A released key, the operator
        // leaving the page, a UI crash, or a lost link all stop the stream and
        // de-energize the outputs within the window.
        if got_manual {
            self.manual_io_remaining = MANUAL_IO_WATCHDOG;
        } else if self.manual_io_remaining > 0 {
            self.manual_io_remaining -= 1;
        }

        let outputs = if self.manual_io_remaining > 0 {
            // Manual-IO mode: drive outputs straight from the operator's held keys,
            // but only while the bus is up — never energize into a faulted bus.
            Outputs {
                wrap: inputs.healthy && self.manual_wrap,
                knife: inputs.healthy && self.manual_knife,
            }
        } else {
            // Normal control: independent pulses; a wrap counts only on clean
            // completion (REQ_0004).
            if let PulseEvent::Completed = self.wrap.tick(SCAN, inputs.healthy) {
                if let Err(e) = self.counters.increment_wrap() {
                    // A failed persist must not crash the safety loop; log and carry on.
                    eprintln!("[control] wrap counter persist failed: {e}");
                }
            }
            let _ = self.knife.tick(SCAN, inputs.healthy);
            Outputs {
                wrap: self.wrap.output(),
                knife: self.knife.output(),
            }
        };
        (outputs, self.snapshot())
    }

    fn snapshot(&self) -> StateSnapshot {
        let counts = self.counters.snapshot();
        StateSnapshot {
            mode: self.state.mode(),
            bale_full: self.state.bale_full(),
            knife: match self.state.knife_in() {
                None => KnifePos::Unknown,
                Some(true) => KnifePos::In,
                Some(false) => KnifePos::Out,
            },
            wrap_armed: self.state.wrap_armed(),
            full_latched: self.full_latch_remaining > 0,
            wrap_active: self.wrap.is_active(),
            knife_active: self.knife.is_active(),
            session: counts.session,
            total: counts.total,
            di1: self.last_di1,
            di2: self.last_di2,
            ip: self.last_ip.map(|i| i.octets()).unwrap_or([0, 0, 0, 0]),
            ip_valid: self.last_ip.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::NetworkError;
    use baler_core::Mode;

    /// A unique scratch counter path per test (no time/randomness dependency).
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("baler-control-test-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("counters")
    }

    /// Records network-mode switches and returns the demo static IP.
    #[derive(Default)]
    struct FakeNet {
        to_ethernet: u32,
        to_ethercat: u32,
    }

    impl NetworkController for FakeNet {
        fn enter_ethernet(&mut self) -> Result<Ipv4Addr, NetworkError> {
            self.to_ethernet += 1;
            Ok(Ipv4Addr::new(192, 168, 1, 102))
        }
        fn enter_ethercat(&mut self) -> Result<(), NetworkError> {
            self.to_ethercat += 1;
            Ok(())
        }
    }

    /// Records EtherCAT-master suspend/restart requests (ISSUE_0010).
    #[derive(Default)]
    struct FakeBus {
        suspends: u32,
        restarts: u32,
    }

    impl BusController for FakeBus {
        fn suspend(&mut self) {
            self.suspends += 1;
        }
        fn restart(&mut self) {
            self.restarts += 1;
        }
    }

    fn healthy() -> Inputs {
        Inputs {
            bale_full: false,
            knife_in: false,
            healthy: true,
        }
    }

    fn full() -> Inputs {
        Inputs {
            bale_full: true,
            knife_in: false,
            healthy: true,
        }
    }

    #[test]
    fn healthy_cycle_is_operational_with_low_outputs() {
        let mut c = Control::new(scratch("tracer")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();

        let (out, snap) = c.step(healthy(), vec![], &mut net, &mut bus);

        assert_eq!(snap.mode, Mode::Operational);
        assert!(!out.wrap);
        assert!(!out.knife);
        assert!(!snap.wrap_active);
        assert!(!snap.knife_active);
    }

    #[test]
    fn debounced_bale_full_reaches_state_machine_after_window() {
        let mut c = Control::new(scratch("debounce")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();

        // Sustained `bale_full` only crosses the debounce after 3 cycles.
        let (_, s1) = c.step(full(), vec![], &mut net, &mut bus);
        assert!(!s1.bale_full, "first sample must not pass the debounce");
        let (_, _s2) = c.step(full(), vec![], &mut net, &mut bus);
        let (_, s3) = c.step(full(), vec![], &mut net, &mut bus);
        assert!(s3.bale_full, "third sustained sample crosses the debounce");
        assert!(s3.wrap_armed, "full + operational arms the wrap softkey");
    }

    #[test]
    fn bale_full_latches_for_at_least_20s_after_di1_clears() {
        let mut c = Control::new(scratch("fulllatch")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();

        // Sustained full crosses the debounce and latches the FULL indication.
        c.step(full(), vec![], &mut net, &mut bus);
        c.step(full(), vec![], &mut net, &mut bus);
        let (_, s) = c.step(full(), vec![], &mut net, &mut bus);
        assert!(s.full_latched, "a debounced bale-full latches FULL");

        // DI1 clears immediately — the latch must hold.
        let (_, s) = c.step(healthy(), vec![], &mut net, &mut bus);
        assert!(s.full_latched, "latch holds right after DI1 drops");

        // Still latched ~10 s in (1000 cycles of DI1 low).
        let mut snap = s;
        for _ in 0..1000 {
            let (_, x) = c.step(healthy(), vec![], &mut net, &mut bus);
            snap = x;
        }
        assert!(snap.full_latched, "still latched ~10 s after DI1 cleared");

        // After >= 20 s total it releases.
        for _ in 0..1100 {
            let (_, x) = c.step(healthy(), vec![], &mut net, &mut bus);
            snap = x;
        }
        assert!(!snap.full_latched, "latch releases after >= 20 s");
    }

    #[test]
    fn firing_a_wrap_clears_the_full_latch() {
        let mut c = Control::new(scratch("wrapclears")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(full(), vec![], &mut net, &mut bus);
        c.step(full(), vec![], &mut net, &mut bus);
        let (_, s) = c.step(full(), vec![], &mut net, &mut bus);
        assert!(s.full_latched, "latched after a debounced full");

        let (_, s) = c.step(healthy(), vec![Command::Wrap], &mut net, &mut bus);
        assert!(
            !s.full_latched,
            "a wrap clears the FULL latch — the operator handled it"
        );
    }

    #[test]
    fn manual_io_drives_outputs_directly() {
        let mut c = Control::new(scratch("manualon")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        let (out, _) = c.step(
            healthy(),
            vec![Command::ManualIo { wrap: true, knife: false }],
            &mut net,
            &mut bus,
        );
        assert!(out.wrap, "manual-IO energizes DO1 directly");
        assert!(!out.knife);

        let (out, _) = c.step(
            healthy(),
            vec![Command::ManualIo { wrap: false, knife: true }],
            &mut net,
            &mut bus,
        );
        assert!(!out.wrap);
        assert!(out.knife, "manual-IO energizes DO2 directly");
    }

    #[test]
    fn manual_io_watchdog_de_energizes_and_restores_control_when_commands_stop() {
        let mut c = Control::new(scratch("manualwd")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        let (out, _) = c.step(
            healthy(),
            vec![Command::ManualIo { wrap: true, knife: true }],
            &mut net,
            &mut bus,
        );
        assert!(out.wrap && out.knife, "manual-IO energizes both outputs");

        // Stop sending ManualIo — within the watchdog window the outputs fall.
        let mut out = out;
        for _ in 0..MANUAL_IO_WATCHDOG {
            let (o, _) = c.step(healthy(), vec![], &mut net, &mut bus);
            out = o;
        }
        assert!(
            !out.wrap && !out.knife,
            "watchdog de-energizes the outputs when the ManualIo stream stops"
        );

        // Normal control is restored: a Wrap is honoured again.
        let (o, snap) = c.step(healthy(), vec![Command::Wrap], &mut net, &mut bus);
        assert!(o.wrap, "normal control resumes after the watchdog exits manual-IO");
        assert!(snap.wrap_active);
    }

    #[test]
    fn manual_io_outputs_suppressed_when_bus_unhealthy() {
        let mut c = Control::new(scratch("manualunhealthy")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus);

        // Bus down: manual-IO must not energize anything.
        let down = Inputs { bale_full: false, knife_in: false, healthy: false };
        let (out, _) = c.step(
            down,
            vec![Command::ManualIo { wrap: true, knife: true }],
            &mut net,
            &mut bus,
        );
        assert!(!out.wrap && !out.knife, "manual-IO never drives a faulted bus");
    }

    #[test]
    fn wrap_command_drives_do1_while_pulse_active() {
        let mut c = Control::new(scratch("wrap")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        let (out, snap) = c.step(healthy(), vec![Command::Wrap], &mut net, &mut bus);

        assert!(out.wrap, "an accepted Wrap fires the DO1 pulse");
        assert!(snap.wrap_active);
        assert!(!out.knife);
    }

    #[test]
    fn knife_command_drives_do2() {
        let mut c = Control::new(scratch("knife")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        let (out, snap) = c.step(healthy(), vec![Command::ToggleKnife], &mut net, &mut bus);

        assert!(out.knife, "an accepted ToggleKnife fires the DO2 pulse");
        assert!(snap.knife_active);
        assert!(!out.wrap);
    }

    #[test]
    fn completed_wrap_pulse_increments_counters_once() {
        let mut c = Control::new(scratch("complete")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational
        c.step(healthy(), vec![Command::Wrap], &mut net, &mut bus); // fire the pulse

        // 5 s pulse at a 10 ms scan = 500 ticks; drive well past completion.
        let mut last = c.snapshot();
        for _ in 0..600 {
            last = c.step(healthy(), vec![], &mut net, &mut bus).1;
        }

        assert_eq!(last.session, 1, "a clean wrap counts exactly once");
        assert_eq!(last.total, 1);
        assert!(!last.wrap_active, "the pulse has ended");
    }

    #[test]
    fn ethernet_switch_drives_network_and_snapshot_ip() {
        let mut c = Control::new(scratch("netmode")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        let (_, snap) = c.step(healthy(), vec![Command::EnterEthernet], &mut net, &mut bus);
        assert_eq!(net.to_ethernet, 1, "entered Ethernet maintenance mode");
        assert!(snap.ip_valid);
        assert_eq!(snap.ip, [192, 168, 1, 102]);
        assert_eq!(snap.mode, Mode::Ethernet);

        let (_, snap2) = c.step(healthy(), vec![Command::ReturnToEthercat], &mut net, &mut bus);
        assert_eq!(net.to_ethercat, 1);
        assert!(!snap2.ip_valid, "returning to EtherCAT clears the static IP");
    }

    #[test]
    fn entering_ethernet_mode_suspends_the_ethercat_bus() {
        let mut c = Control::new(scratch("suspend")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        c.step(healthy(), vec![Command::EnterEthernet], &mut net, &mut bus);

        assert_eq!(bus.suspends, 1, "entering Ethernet stops the EtherCAT master");
        assert_eq!(bus.restarts, 0, "no restart on the way into Ethernet");
    }

    #[test]
    fn normal_cycles_leave_the_bus_master_untouched() {
        let mut c = Control::new(scratch("nobus")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();

        // A handful of ordinary operational cycles, including a wrap pulse.
        c.step(healthy(), vec![], &mut net, &mut bus);
        c.step(healthy(), vec![Command::Wrap], &mut net, &mut bus);
        for _ in 0..5 {
            c.step(full(), vec![], &mut net, &mut bus);
        }

        assert_eq!(bus.suspends, 0, "only a mode switch may stop the master");
        assert_eq!(bus.restarts, 0, "only a mode switch may restart the master");
    }

    #[test]
    fn returning_to_ethercat_restarts_the_bus() {
        let mut c = Control::new(scratch("restart")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational
        c.step(healthy(), vec![Command::EnterEthernet], &mut net, &mut bus); // suspend

        c.step(healthy(), vec![Command::ReturnToEthercat], &mut net, &mut bus);

        assert_eq!(bus.restarts, 1, "returning to EtherCAT rebuilds the master");
        assert_eq!(bus.suspends, 1, "the return must not suspend again");
    }

    #[test]
    fn bus_loss_faults_clears_inputs_and_aborts_pulse_without_counting() {
        let mut c = Control::new(scratch("busloss")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        for _ in 0..3 {
            c.step(full(), vec![], &mut net, &mut bus); // Operational + debounced full
        }
        c.step(full(), vec![Command::Wrap], &mut net, &mut bus); // wrap pulse in flight

        let unhealthy = Inputs {
            bale_full: true,
            knife_in: false,
            healthy: false,
        };
        let (out, snap) = c.step(unhealthy, vec![], &mut net, &mut bus);

        assert_eq!(snap.mode, Mode::Fault);
        assert!(!snap.bale_full, "faulted inputs report unknown");
        assert_eq!(snap.knife, KnifePos::Unknown);
        assert!(!out.wrap, "the pulse aborts low on bus loss");
        assert_eq!(snap.session, 0, "an aborted wrap must not count");
    }
}
