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
use baler_core::pulse::PulseEngine;
use baler_core::state::{Action, BalerState};
use baler_core::{Command, KnifePos, Mode, StateSnapshot};

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
    /// Directional knife pulses. A `FireKnife` triggers exactly one based on the
    /// live ch2 (DI2) value, and only while neither is active (interlock) — DO2
    /// and DO3 can never be energized together.
    knives_in: PulseEngine,
    knives_out: PulseEngine,
    full_db: Debouncer,
    knife_db: Debouncer,
    /// Baler-fully-open (DI3/ch3) debouncer. Its debounced rising edge counts one
    /// ejected bale (REQ_0004); `last_open_level` holds the previous debounced
    /// level so each open is counted exactly once.
    open_db: Debouncer,
    last_open_level: bool,
    last_ip: Option<Ipv4Addr>,
    /// Bale-full attention latch: cycles remaining that "FULL" stays shown after a
    /// rising debounced DI1, cleared early by a wrap (REQ_0017).
    full_latch_remaining: u32,
    /// Raw (un-debounced) discrete inputs, surfaced for the IO test screen (REQ_0018).
    last_di1: bool,
    last_di2: bool,
    last_di3: bool,
    /// Manual-IO mode (REQ_0018): cycles remaining before the command watchdog
    /// fails safe. >0 means the IO test screen is driving the outputs directly;
    /// `manual_wrap`/`manual_knives_in`/`manual_knives_out` are the latest
    /// operator-held bits.
    manual_io_remaining: u32,
    manual_wrap: bool,
    manual_knives_in: bool,
    manual_knives_out: bool,
}

impl Control {
    /// Build the control cycle, loading persisted counters from `counter_path`.
    pub fn new(counter_path: impl Into<PathBuf>) -> std::io::Result<Self> {
        Ok(Self {
            counters: CounterStore::load(counter_path.into())?,
            state: BalerState::new(),
            wrap: PulseEngine::new(PULSE),
            knives_in: PulseEngine::new(PULSE),
            knives_out: PulseEngine::new(PULSE),
            full_db: Debouncer::new(DEBOUNCE, false),
            knife_db: Debouncer::new(DEBOUNCE, false),
            open_db: Debouncer::new(DEBOUNCE, false),
            last_open_level: false,
            last_ip: None,
            full_latch_remaining: 0,
            last_di1: false,
            last_di2: false,
            last_di3: false,
            manual_io_remaining: 0,
            manual_wrap: false,
            manual_knives_in: false,
            manual_knives_out: false,
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
        self.last_di3 = inputs.bale_open;
        self.full_db.update(inputs.bale_full);
        self.knife_db.update(inputs.knife_in);
        self.open_db.update(inputs.bale_open);
        if inputs.healthy {
            self.state.on_inputs(self.full_db.level(), self.knife_db.level());
        }

        // Bale count (REQ_0004): one ejected bale per debounced rising edge of the
        // baler-fully-open input (DI3/ch3), counted only while the bus is healthy
        // so a gate left open across a fault doesn't double-count on recovery.
        let open_level = self.open_db.level();
        if inputs.healthy && open_level && !self.last_open_level {
            if let Err(e) = self.counters.increment_bale() {
                // A failed persist must not crash the safety loop; log and carry on.
                eprintln!("[control] bale counter persist failed: {e}");
            }
        }
        self.last_open_level = open_level;

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
            if let Command::ManualIo { wrap, knives_in, knives_out } = cmd {
                got_manual = true;
                self.manual_wrap = wrap;
                self.manual_knives_in = knives_in;
                self.manual_knives_out = knives_out;
                continue;
            }
            if self.manual_io_remaining > 0 {
                continue; // manual-IO mode: ignore machine/mode commands
            }
            let knife_active = self.knives_in.is_active() || self.knives_out.is_active();
            let any_active = self.wrap.is_active() || knife_active;
            match self.state.handle(cmd, any_active) {
                Ok(Action::FireWrap) => {
                    self.wrap.trigger();
                    // The operator handled the full bale — clear the attention
                    // latch so it stops nagging (REQ_0017).
                    self.full_latch_remaining = 0;
                }
                Ok(Action::FireKnife) => {
                    // Directional: ch2 (debounced DI2) picks the output — true →
                    // knives-in (DO2), false → knives-out (DO3). Interlock on the
                    // OR of both pulses so DO2 and DO3 are never driven together,
                    // even if ch2 flips mid-pulse and the operator re-fires.
                    if !knife_active {
                        if self.knife_db.level() {
                            self.knives_in.trigger();
                        } else {
                            self.knives_out.trigger();
                        }
                    }
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
            // The knife outputs are interlocked even here (knives-in wins) so the
            // test screen can never drive DO2 and DO3 together.
            let knives_in = inputs.healthy && self.manual_knives_in;
            Outputs {
                wrap: inputs.healthy && self.manual_wrap,
                knives_in,
                knives_out: inputs.healthy && self.manual_knives_out && !knives_in,
            }
        } else {
            // Normal control: independent pulses. The wrap pulse still fires the
            // wrapper, but no longer drives the bale count — counting now keys off
            // the DI3 baler-open edge above (REQ_0004).
            let _ = self.wrap.tick(SCAN, inputs.healthy);
            let _ = self.knives_in.tick(SCAN, inputs.healthy);
            let _ = self.knives_out.tick(SCAN, inputs.healthy);
            Outputs {
                wrap: self.wrap.output(),
                knives_in: self.knives_in.output(),
                knives_out: self.knives_out.output(),
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
            knife_active: self.knives_in.is_active() || self.knives_out.is_active(),
            session: counts.session,
            total: counts.total,
            di1: self.last_di1,
            di2: self.last_di2,
            di3: self.last_di3,
            ip: self.last_ip.map(|i| i.octets()).unwrap_or([0, 0, 0, 0]),
            ip_valid: self.last_ip.is_some(),
        }
    }

    /// A snapshot for a pre-bus state — used while the EtherCAT daemon is waiting
    /// for the eth0 carrier before the connector exists (no cable/coupler). Reports
    /// `mode` (typically [`Mode::Fault`], which raises the panel's fault overlay)
    /// with current counters and everything else idle/unknown, so the operator sees
    /// a message instead of a reboot loop while the watchdog keeps being petted.
    ///
    /// Only the EtherCAT+transport run path publishes it; without `transport` there
    /// is no UI to receive it, so it is dead there (the unit test still covers it).
    #[cfg_attr(not(feature = "transport"), allow(dead_code))]
    pub fn idle_snapshot(&self, mode: Mode) -> StateSnapshot {
        let counts = self.counters.snapshot();
        StateSnapshot {
            mode,
            bale_full: false,
            knife: KnifePos::Unknown,
            wrap_armed: false,
            full_latched: false,
            wrap_active: false,
            knife_active: false,
            session: counts.session,
            total: counts.total,
            di1: false,
            di2: false,
            di3: false,
            ip: [0, 0, 0, 0],
            ip_valid: false,
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
            bale_open: false,
            healthy: true,
        }
    }

    fn full() -> Inputs {
        Inputs {
            bale_full: true,
            knife_in: false,
            bale_open: false,
            healthy: true,
        }
    }

    /// Healthy bus with the baler-fully-open (DI3) input asserted.
    fn open() -> Inputs {
        Inputs {
            bale_full: false,
            knife_in: false,
            bale_open: true,
            healthy: true,
        }
    }

    #[test]
    fn idle_snapshot_reports_the_given_mode_with_idle_io_and_live_counters() {
        let mut c = Control::new(scratch("idlesnap")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        // Land a bale (a debounced DI3 open edge) so the counters are non-zero.
        c.step(healthy(), vec![], &mut net, &mut bus);
        for _ in 0..DEBOUNCE {
            c.step(open(), vec![], &mut net, &mut bus);
        }

        // Waiting-for-link snapshot: Fault overlay, unknown IO, but counters live.
        let snap = c.idle_snapshot(Mode::Fault);
        assert_eq!(snap.mode, Mode::Fault);
        assert_eq!(snap.knife, KnifePos::Unknown);
        assert!(!snap.bale_full && !snap.wrap_active && !snap.knife_active);
        assert!(!snap.di1 && !snap.di2 && !snap.di3 && !snap.ip_valid);
        assert_eq!(snap.session, 1, "counters survive into the idle snapshot");
        assert_eq!(snap.total, 1);
    }

    #[test]
    fn healthy_cycle_is_operational_with_low_outputs() {
        let mut c = Control::new(scratch("tracer")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();

        let (out, snap) = c.step(healthy(), vec![], &mut net, &mut bus);

        assert_eq!(snap.mode, Mode::Operational);
        assert!(!out.wrap);
        assert!(!out.knives_in);
        assert!(!out.knives_out);
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
            vec![Command::ManualIo { wrap: true, knives_in: false, knives_out: false }],
            &mut net,
            &mut bus,
        );
        assert!(out.wrap, "manual-IO energizes DO1 directly");
        assert!(!out.knives_in);
        assert!(!out.knives_out);

        let (out, _) = c.step(
            healthy(),
            vec![Command::ManualIo { wrap: false, knives_in: true, knives_out: false }],
            &mut net,
            &mut bus,
        );
        assert!(!out.wrap);
        assert!(out.knives_in, "manual-IO energizes DO2 directly");
        assert!(!out.knives_out);

        let (out, _) = c.step(
            healthy(),
            vec![Command::ManualIo { wrap: false, knives_in: false, knives_out: true }],
            &mut net,
            &mut bus,
        );
        assert!(out.knives_out, "manual-IO energizes DO3 directly");
        assert!(!out.knives_in);
    }

    #[test]
    fn manual_io_interlocks_the_two_knife_outputs() {
        let mut c = Control::new(scratch("manualinterlock")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        // Both knife keys held at once: the interlock lets knives-in win and keeps
        // DO3 low — DO2 and DO3 are never energized together.
        let (out, _) = c.step(
            healthy(),
            vec![Command::ManualIo { wrap: false, knives_in: true, knives_out: true }],
            &mut net,
            &mut bus,
        );
        assert!(out.knives_in, "knives-in wins the interlock");
        assert!(!out.knives_out, "knives-out is suppressed while knives-in is driven");
    }

    #[test]
    fn manual_io_watchdog_de_energizes_and_restores_control_when_commands_stop() {
        let mut c = Control::new(scratch("manualwd")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        let (out, _) = c.step(
            healthy(),
            vec![Command::ManualIo { wrap: true, knives_in: true, knives_out: false }],
            &mut net,
            &mut bus,
        );
        assert!(out.wrap && out.knives_in, "manual-IO energizes both outputs");

        // Stop sending ManualIo — within the watchdog window the outputs fall.
        let mut out = out;
        for _ in 0..MANUAL_IO_WATCHDOG {
            let (o, _) = c.step(healthy(), vec![], &mut net, &mut bus);
            out = o;
        }
        assert!(
            !out.wrap && !out.knives_in && !out.knives_out,
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
        let down = Inputs { bale_full: false, knife_in: false, bale_open: false, healthy: false };
        let (out, _) = c.step(
            down,
            vec![Command::ManualIo { wrap: true, knives_in: true, knives_out: true }],
            &mut net,
            &mut bus,
        );
        assert!(
            !out.wrap && !out.knives_in && !out.knives_out,
            "manual-IO never drives a faulted bus"
        );
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
        assert!(!out.knives_in);
        assert!(!out.knives_out);
    }

    /// Steady `knife_in` (ch2) input until the debounce settles, so direction
    /// selection sees the intended level.
    fn knives_in_input() -> Inputs {
        Inputs { bale_full: false, knife_in: true, bale_open: false, healthy: true }
    }

    #[test]
    fn knife_command_with_ch2_false_drives_knives_out_do3() {
        let mut c = Control::new(scratch("knifeout")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational; ch2 low

        let (out, snap) = c.step(healthy(), vec![Command::ToggleKnife], &mut net, &mut bus);

        assert!(out.knives_out, "ch2 false fires the DO3 (knives-out) pulse");
        assert!(!out.knives_in);
        assert!(snap.knife_active);
        assert!(!out.wrap);
    }

    #[test]
    fn knife_command_with_ch2_true_drives_knives_in_do2() {
        let mut c = Control::new(scratch("knifein")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        // Hold ch2 high long enough to cross the debounce before firing.
        for _ in 0..DEBOUNCE {
            c.step(knives_in_input(), vec![], &mut net, &mut bus);
        }

        let (out, snap) =
            c.step(knives_in_input(), vec![Command::ToggleKnife], &mut net, &mut bus);

        assert!(out.knives_in, "ch2 true fires the DO2 (knives-in) pulse");
        assert!(!out.knives_out);
        assert!(snap.knife_active);
        assert!(!out.wrap);
    }

    #[test]
    fn knife_outputs_interlock_when_ch2_flips_mid_pulse() {
        let mut c = Control::new(scratch("knifeinterlock")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // Operational; ch2 low

        // ch2 low → a knives-out pulse starts.
        let (out, _) = c.step(healthy(), vec![Command::ToggleKnife], &mut net, &mut bus);
        assert!(out.knives_out && !out.knives_in);

        // ch2 flips high and the operator re-fires while the pulse is still in
        // flight: the interlock must keep knives-in low (no opposing output).
        for _ in 0..DEBOUNCE {
            c.step(knives_in_input(), vec![], &mut net, &mut bus);
        }
        let (out, _) =
            c.step(knives_in_input(), vec![Command::ToggleKnife], &mut net, &mut bus);
        assert!(out.knives_out, "the original knives-out pulse keeps running");
        assert!(!out.knives_in, "the interlock blocks the opposing output");
    }

    #[test]
    fn completed_wrap_pulse_no_longer_counts() {
        let mut c = Control::new(scratch("wrapnocount")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational
        c.step(healthy(), vec![Command::Wrap], &mut net, &mut bus); // fire the pulse

        // Drive the 5 s pulse well past completion — counting now keys off DI3,
        // not the wrap, so the count must stay at zero.
        let mut last = c.snapshot();
        for _ in 0..600 {
            last = c.step(healthy(), vec![], &mut net, &mut bus).1;
        }

        assert!(!last.wrap_active, "the pulse has ended");
        assert_eq!(last.session, 0, "a wrap no longer counts a bale");
        assert_eq!(last.total, 0);
    }

    #[test]
    fn di3_open_edge_counts_one_bale_after_debounce() {
        let mut c = Control::new(scratch("baleedge")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        // Sustained DI3 only crosses the debounce after DEBOUNCE cycles; the count
        // fires exactly once on that rising edge.
        let mut last = c.snapshot();
        for _ in 0..DEBOUNCE {
            last = c.step(open(), vec![], &mut net, &mut bus).1;
        }
        assert_eq!(last.session, 1, "a debounced DI3 open edge counts one bale");
        assert_eq!(last.total, 1);

        // Holding DI3 high must not re-count — only the edge counts.
        for _ in 0..50 {
            last = c.step(open(), vec![], &mut net, &mut bus).1;
        }
        assert_eq!(last.session, 1, "a held-open input counts only once");

        // A second open cycle (close, then open again) counts a second bale.
        for _ in 0..DEBOUNCE {
            c.step(healthy(), vec![], &mut net, &mut bus); // DI3 low, debounce down
        }
        for _ in 0..DEBOUNCE {
            last = c.step(open(), vec![], &mut net, &mut bus).1;
        }
        assert_eq!(last.session, 2, "a fresh open edge counts the next bale");
        assert_eq!(last.total, 2);
    }

    #[test]
    fn di3_open_edge_while_unhealthy_does_not_count() {
        let mut c = Control::new(scratch("baleunhealthy")).unwrap();
        let mut net = FakeNet::default();
        let mut bus = FakeBus::default();
        c.step(healthy(), vec![], &mut net, &mut bus); // reach Operational

        // DI3 asserted while the bus is down (gate opened during a fault): no count.
        let down_open = || Inputs { bale_full: false, knife_in: false, bale_open: true, healthy: false };
        let mut last = c.snapshot();
        for _ in 0..10 {
            last = c.step(down_open(), vec![], &mut net, &mut bus).1;
        }
        assert_eq!(last.session, 0, "an open edge during a fault must not count");
        assert_eq!(snap_session_after_recovery(&mut c, &mut net, &mut bus), 0,
            "a gate left open across recovery still must not count (no fresh edge)");
    }

    /// Hold DI3 high while the bus recovers; with the level already high there is
    /// no rising edge, so no bale is counted. Returns the resulting session count.
    fn snap_session_after_recovery(
        c: &mut Control,
        net: &mut dyn NetworkController,
        bus: &mut dyn BusController,
    ) -> u64 {
        let mut last = c.snapshot();
        for _ in 0..DEBOUNCE + 2 {
            last = c.step(open(), vec![], net, bus).1;
        }
        last.session
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
            bale_open: false,
            healthy: false,
        };
        let (out, snap) = c.step(unhealthy, vec![], &mut net, &mut bus);

        assert_eq!(snap.mode, Mode::Fault);
        assert!(!snap.bale_full, "faulted inputs report unknown");
        assert_eq!(snap.knife, KnifePos::Unknown);
        assert!(!out.wrap, "the pulse aborts low on bus loss");
        assert_eq!(snap.session, 0, "no bale counted across a bus loss");
    }
}
