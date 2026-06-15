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
use baler_ipc::{Command, KnifePos, StateSnapshot};

use crate::ports::{Inputs, NetworkController, Outputs};

const SCAN: Duration = Duration::from_millis(10);
const DEBOUNCE: u8 = 3; // ~30 ms at the 10 ms scan
const PULSE: Duration = Duration::from_secs(5);

/// Owns the control-cycle state. One `step` per 10 ms scan.
pub struct Control {
    counters: CounterStore,
    state: BalerState,
    wrap: PulseEngine,
    knife: PulseEngine,
    full_db: Debouncer,
    knife_db: Debouncer,
    last_ip: Option<Ipv4Addr>,
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
        })
    }

    /// Advance one 10 ms scan cycle: fold in bus health and conditioned inputs,
    /// apply operator `commands` (driving the network controller for mode
    /// switches), tick the pulses, and return the commanded [`Outputs`] plus the
    /// [`StateSnapshot`] to publish.
    pub fn step(
        &mut self,
        inputs: Inputs,
        commands: Vec<Command>,
        net: &mut dyn NetworkController,
    ) -> (Outputs, StateSnapshot) {
        self.state.on_bus_health(inputs.healthy);
        self.full_db.update(inputs.bale_full);
        self.knife_db.update(inputs.knife_in);
        if inputs.healthy {
            self.state.on_inputs(self.full_db.level(), self.knife_db.level());
        }

        for cmd in commands {
            let any_active = self.wrap.is_active() || self.knife.is_active();
            match self.state.handle(cmd, any_active) {
                Ok(Action::FireWrap) => {
                    self.wrap.trigger();
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
                    self.last_ip = net.enter_ethernet().ok();
                }
                Ok(Action::SwitchToEthercat) => {
                    let _ = net.enter_ethercat();
                    self.last_ip = None;
                }
                Err(_reject) => { /* surface "busy / not ready" on the UI */ }
            }
        }

        // Independent pulses; a wrap counts only on clean completion (REQ_0004).
        if let PulseEvent::Completed = self.wrap.tick(SCAN, inputs.healthy) {
            if let Err(e) = self.counters.increment_wrap() {
                // A failed persist must not crash the safety loop; log and carry on.
                eprintln!("[control] wrap counter persist failed: {e}");
            }
        }
        let _ = self.knife.tick(SCAN, inputs.healthy);

        let outputs = Outputs {
            wrap: self.wrap.output(),
            knife: self.knife.output(),
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
            wrap_active: self.wrap.is_active(),
            knife_active: self.knife.is_active(),
            session: counts.session,
            total: counts.total,
            ip: self.last_ip.map(|i| i.octets()).unwrap_or([0, 0, 0, 0]),
            ip_valid: self.last_ip.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::NetworkError;
    use baler_ipc::Mode;

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

        let (out, snap) = c.step(healthy(), vec![], &mut net);

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

        // Sustained `bale_full` only crosses the debounce after 3 cycles.
        let (_, s1) = c.step(full(), vec![], &mut net);
        assert!(!s1.bale_full, "first sample must not pass the debounce");
        let (_, _s2) = c.step(full(), vec![], &mut net);
        let (_, s3) = c.step(full(), vec![], &mut net);
        assert!(s3.bale_full, "third sustained sample crosses the debounce");
        assert!(s3.wrap_armed, "full + operational arms the wrap softkey");
    }

    #[test]
    fn wrap_command_drives_do1_while_pulse_active() {
        let mut c = Control::new(scratch("wrap")).unwrap();
        let mut net = FakeNet::default();
        c.step(healthy(), vec![], &mut net); // reach Operational

        let (out, snap) = c.step(healthy(), vec![Command::Wrap], &mut net);

        assert!(out.wrap, "an accepted Wrap fires the DO1 pulse");
        assert!(snap.wrap_active);
        assert!(!out.knife);
    }

    #[test]
    fn knife_command_drives_do2() {
        let mut c = Control::new(scratch("knife")).unwrap();
        let mut net = FakeNet::default();
        c.step(healthy(), vec![], &mut net); // reach Operational

        let (out, snap) = c.step(healthy(), vec![Command::ToggleKnife], &mut net);

        assert!(out.knife, "an accepted ToggleKnife fires the DO2 pulse");
        assert!(snap.knife_active);
        assert!(!out.wrap);
    }

    #[test]
    fn completed_wrap_pulse_increments_counters_once() {
        let mut c = Control::new(scratch("complete")).unwrap();
        let mut net = FakeNet::default();
        c.step(healthy(), vec![], &mut net); // reach Operational
        c.step(healthy(), vec![Command::Wrap], &mut net); // fire the pulse

        // 5 s pulse at a 10 ms scan = 500 ticks; drive well past completion.
        let mut last = c.snapshot();
        for _ in 0..600 {
            last = c.step(healthy(), vec![], &mut net).1;
        }

        assert_eq!(last.session, 1, "a clean wrap counts exactly once");
        assert_eq!(last.total, 1);
        assert!(!last.wrap_active, "the pulse has ended");
    }

    #[test]
    fn ethernet_switch_drives_network_and_snapshot_ip() {
        let mut c = Control::new(scratch("netmode")).unwrap();
        let mut net = FakeNet::default();
        c.step(healthy(), vec![], &mut net); // reach Operational

        let (_, snap) = c.step(healthy(), vec![Command::EnterEthernet], &mut net);
        assert_eq!(net.to_ethernet, 1, "entered Ethernet maintenance mode");
        assert!(snap.ip_valid);
        assert_eq!(snap.ip, [192, 168, 1, 102]);
        assert_eq!(snap.mode, Mode::Ethernet);

        let (_, snap2) = c.step(healthy(), vec![Command::ReturnToEthercat], &mut net);
        assert_eq!(net.to_ethercat, 1);
        assert!(!snap2.ip_valid, "returning to EtherCAT clears the static IP");
    }

    #[test]
    fn bus_loss_faults_clears_inputs_and_aborts_pulse_without_counting() {
        let mut c = Control::new(scratch("busloss")).unwrap();
        let mut net = FakeNet::default();
        for _ in 0..3 {
            c.step(full(), vec![], &mut net); // Operational + debounced full
        }
        c.step(full(), vec![Command::Wrap], &mut net); // wrap pulse in flight

        let unhealthy = Inputs {
            bale_full: true,
            knife_in: false,
            healthy: false,
        };
        let (out, snap) = c.step(unhealthy, vec![], &mut net);

        assert_eq!(snap.mode, Mode::Fault);
        assert!(!snap.bale_full, "faulted inputs report unknown");
        assert_eq!(snap.knife, KnifePos::Unknown);
        assert!(!out.wrap, "the pulse aborts low on bus loss");
        assert_eq!(snap.session, 0, "an aborted wrap must not count");
    }
}
