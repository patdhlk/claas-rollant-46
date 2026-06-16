//! The master mode state machine and command gating (REQ_0008, REQ_0011, REQ_0012).
//!
//! [`BalerState`] is a pure reducer: feed it bus health, conditioned inputs, and
//! commands; it owns the mode and reports the [`Action`] a command implies (or
//! why it was [`Reject`]ed). It does NOT own the pulses or counters — the daemon
//! wires those together — so transition coverage is table-testable in isolation.

use crate::ipc::{Command, Mode};

/// A side effect the daemon must carry out as a result of a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    FireWrap,
    FireKnife,
    ResetSession,
    ResetTotal,
    SwitchToEthernet,
    SwitchToEthercat,
}

/// Why a command was not accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reject {
    /// Bus not healthy (Initializing or Fault) — operator commands blocked.
    NotReady,
    /// A pulse is in flight; the idle-only mode switch is refused.
    NotIdle,
    /// Already in Ethernet mode.
    AlreadyEthernet,
    /// Not in Ethernet mode, so there is nothing to return from.
    NotInEthernet,
}

#[derive(Debug, Clone)]
pub struct BalerState {
    mode: Mode,
    bale_full: bool,
    knife_in: Option<bool>,
}

impl Default for BalerState {
    fn default() -> Self {
        Self::new()
    }
}

impl BalerState {
    pub fn new() -> Self {
        Self {
            mode: Mode::Initializing,
            bale_full: false,
            knife_in: None,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn bale_full(&self) -> bool {
        self.bale_full
    }
    pub fn knife_in(&self) -> Option<bool> {
        self.knife_in
    }
    /// The F1 wrap softkey is armed only when operational and the bale is full.
    pub fn wrap_armed(&self) -> bool {
        self.mode == Mode::Operational && self.bale_full
    }

    /// Apply the latest EtherCAT health. Ethernet mode is unaffected (EtherCAT
    /// is intentionally down there). A loss while operational enters Fault and
    /// marks the inputs unknown (REQ_0012); recovery returns to Operational.
    pub fn on_bus_health(&mut self, healthy: bool) {
        if self.mode == Mode::Ethernet {
            return;
        }
        if healthy {
            self.mode = Mode::Operational;
        } else {
            self.mark_inputs_unknown();
            self.mode = match self.mode {
                Mode::Operational | Mode::Fault => Mode::Fault,
                _ => Mode::Initializing,
            };
        }
    }

    /// Update the conditioned status inputs. Ignored unless operational — while
    /// faulted or initializing the inputs are reported as unknown.
    pub fn on_inputs(&mut self, bale_full: bool, knife_in: bool) {
        if self.mode == Mode::Operational {
            self.bale_full = bale_full;
            self.knife_in = Some(knife_in);
        }
    }

    /// Resolve a command into an [`Action`] or a [`Reject`]. `any_pulse_active`
    /// is the OR of both pulse engines — it gates the idle-only mode switch.
    pub fn handle(&mut self, cmd: Command, any_pulse_active: bool) -> Result<Action, Reject> {
        match cmd {
            Command::Wrap => self.require_operational().map(|_| Action::FireWrap),
            Command::ToggleKnife => self.require_operational().map(|_| Action::FireKnife),
            // Counter resets are harmless bookkeeping — allowed in any mode.
            Command::ResetSession => Ok(Action::ResetSession),
            Command::ResetTotal => Ok(Action::ResetTotal),
            Command::EnterEthernet => match self.mode {
                Mode::Ethernet => Err(Reject::AlreadyEthernet),
                // A pulse in flight (only possible while Operational) blocks the
                // idle-only switch.
                Mode::Operational if any_pulse_active => Err(Reject::NotIdle),
                // Operational (idle), Fault, or Initializing: enter maintenance
                // mode. This must NOT require a healthy bus — the operator most
                // needs it when the coupler is absent and the bus is flapping, to
                // stop the master and reclaim the NIC (ISSUE_0010).
                _ => {
                    self.mode = Mode::Ethernet;
                    self.mark_inputs_unknown();
                    Ok(Action::SwitchToEthernet)
                }
            },
            Command::ReturnToEthercat => {
                if self.mode == Mode::Ethernet {
                    // Re-init EtherCAT from scratch; health will lift us out.
                    self.mode = Mode::Initializing;
                    Ok(Action::SwitchToEthercat)
                } else {
                    Err(Reject::NotInEthernet)
                }
            }
            // The IO test screen bypasses the state machine entirely — the daemon
            // intercepts `ManualIo` before it reaches here (REQ_0018). Reject
            // defensively so this never silently drives the machine.
            Command::ManualIo { .. } => Err(Reject::NotReady),
        }
    }

    fn require_operational(&self) -> Result<(), Reject> {
        if self.mode == Mode::Operational {
            Ok(())
        } else {
            Err(Reject::NotReady)
        }
    }

    fn mark_inputs_unknown(&mut self) {
        self.bale_full = false;
        self.knife_in = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn operational() -> BalerState {
        let mut s = BalerState::new();
        s.on_bus_health(true);
        s
    }

    #[test]
    fn boot_blocks_commands_until_healthy() {
        let mut s = BalerState::new();
        assert_eq!(s.mode(), Mode::Initializing);
        assert_eq!(s.handle(Command::Wrap, false), Err(Reject::NotReady));
        assert_eq!(s.handle(Command::ToggleKnife, false), Err(Reject::NotReady));
        s.on_bus_health(true);
        assert_eq!(s.handle(Command::Wrap, false), Ok(Action::FireWrap));
    }

    #[test]
    fn wrap_arms_only_when_full_and_operational() {
        let mut s = operational();
        assert!(!s.wrap_armed());
        s.on_inputs(true, false);
        assert!(s.wrap_armed());
        assert_eq!(s.knife_in(), Some(false));
    }

    #[test]
    fn knife_toggle_independent_of_position() {
        let mut s = operational();
        s.on_inputs(false, true);
        assert_eq!(s.handle(Command::ToggleKnife, false), Ok(Action::FireKnife));
        s.on_inputs(false, false);
        assert_eq!(s.handle(Command::ToggleKnife, false), Ok(Action::FireKnife));
    }

    #[test]
    fn mode_switch_is_idle_only() {
        let mut s = operational();
        assert_eq!(s.handle(Command::EnterEthernet, true), Err(Reject::NotIdle));
        assert_eq!(s.mode(), Mode::Operational);
        assert_eq!(
            s.handle(Command::EnterEthernet, false),
            Ok(Action::SwitchToEthernet)
        );
        assert_eq!(s.mode(), Mode::Ethernet);
    }

    #[test]
    fn ethernet_mode_blocks_machine_commands() {
        let mut s = operational();
        s.handle(Command::EnterEthernet, false).unwrap();
        assert_eq!(s.handle(Command::Wrap, false), Err(Reject::NotReady));
        assert_eq!(
            s.handle(Command::EnterEthernet, false),
            Err(Reject::AlreadyEthernet)
        );
        assert_eq!(
            s.handle(Command::ReturnToEthercat, false),
            Ok(Action::SwitchToEthercat)
        );
        assert_eq!(s.mode(), Mode::Initializing);
    }

    #[test]
    fn enter_ethernet_allowed_while_bus_is_down() {
        // The operator most needs maintenance mode exactly when the coupler is
        // gone and the bus is flapping (ISSUE_0010) — a Fault must not block it.
        let mut s = operational();
        s.on_bus_health(false);
        assert_eq!(s.mode(), Mode::Fault);
        assert_eq!(
            s.handle(Command::EnterEthernet, false),
            Ok(Action::SwitchToEthernet)
        );
        assert_eq!(s.mode(), Mode::Ethernet);
    }

    #[test]
    fn enter_ethernet_allowed_at_boot_before_first_health() {
        // Coupler never came up at boot: still let the operator reclaim the NIC.
        let mut s = BalerState::new();
        assert_eq!(s.mode(), Mode::Initializing);
        assert_eq!(
            s.handle(Command::EnterEthernet, false),
            Ok(Action::SwitchToEthernet)
        );
        assert_eq!(s.mode(), Mode::Ethernet);
    }

    #[test]
    fn ethernet_mode_ignores_bus_health() {
        let mut s = operational();
        s.handle(Command::EnterEthernet, false).unwrap();
        s.on_bus_health(true); // a stray healthy report must not flip us out
        assert_eq!(s.mode(), Mode::Ethernet);
    }

    #[test]
    fn return_requires_ethernet_mode() {
        let mut s = operational();
        assert_eq!(
            s.handle(Command::ReturnToEthercat, false),
            Err(Reject::NotInEthernet)
        );
    }

    #[test]
    fn bus_loss_faults_and_clears_inputs_then_recovers() {
        let mut s = operational();
        s.on_inputs(true, true);
        assert!(s.wrap_armed());
        s.on_bus_health(false);
        assert_eq!(s.mode(), Mode::Fault);
        assert_eq!(s.knife_in(), None);
        assert!(!s.bale_full());
        assert_eq!(s.handle(Command::Wrap, false), Err(Reject::NotReady));
        // Inputs during fault are ignored.
        s.on_inputs(true, true);
        assert_eq!(s.knife_in(), None);
        // Recovery.
        s.on_bus_health(true);
        assert_eq!(s.mode(), Mode::Operational);
    }

    #[test]
    fn resets_allowed_in_any_mode() {
        let mut s = BalerState::new(); // Initializing
        assert_eq!(s.handle(Command::ResetSession, false), Ok(Action::ResetSession));
        assert_eq!(s.handle(Command::ResetTotal, false), Ok(Action::ResetTotal));
    }
}
