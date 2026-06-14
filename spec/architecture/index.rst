Architecture Decisions
======================

Architecture decision records (``arch-decision``) live here. Each
``.. arch-decision::`` carries a stable ``ADR_`` ID and a body structured as
**Context**, **Decision**, and **Consequences**.

.. needtable::
   :types: arch-decision
   :columns: id;title;status
   :style: table

.. arch-decision:: Two-process split with the daemon as safety authority
   :id: ADR_0001
   :status: accepted
   :refines: FEAT_0001
   :links: REQ_0001, REQ_0002

   **Context.** The control logic must keep the machine safe even when the UI
   misbehaves, and the Slint UI is GPL-3.0. taktora is built for multi-process
   zero-copy IPC over iceoryx2. We need a boundary that isolates the display
   from the safety-relevant control path.

   **Decision.** Two processes: ``baler-daemon`` is the always-on safety
   authority (root / ``CAP_NET_RAW`` + ``CAP_NET_ADMIN``) owning EtherCAT, the
   state machine, counters, and network mode; ``baler-ui`` is a replaceable
   Slint front-end. They communicate over iceoryx2 shared memory. Considered and
   rejected: a single combined binary — simpler to deploy, but it couples the
   GPL UI to the control logic and a UI panic could take down the safety
   process.

   **Consequences.** ✅ A UI crash cannot affect outputs; the daemon keeps the
   machine safe and systemd restarts the UI. ✅ Matches both upstream stacks.
   ❌ Two binaries, two systemd units, and an IPC contract to version and keep
   in sync.

.. arch-decision:: EtherCAT and Ethernet are a mutually exclusive, idle-only mode switch
   :id: ADR_0002
   :status: accepted
   :refines: FEAT_0001
   :links: REQ_0008, REQ_0009

   **Context.** The CR1140 has a single Ethernet port. The EtherCAT master owns
   that NIC in raw mode, while firmware/application updates need the same NIC
   with an IP stack. The two cannot run at once.

   **Decision.** Treat Ethernet as a deliberate maintenance mode: the switch is
   allowed only when the machine is idle (no output pulse active) and only from
   the PIN-gated service screen. Switching stops the EtherCAT master, lets the
   coupler watchdog drop outputs safe, then brings up a static IP. Considered and
   rejected: dual-port hardware (not available on this device); allowing the
   switch at any time (would drop control mid-cycle).

   **Consequences.** ✅ The mode switch can never strand the machine mid-actuation.
   ✅ Update access is deterministic. ❌ The baler is fully uncontrollable while
   in Ethernet mode until the operator switches back.

.. arch-decision:: Three-layer watchdog with the coupler SM watchdog as backstop
   :id: ADR_0003
   :status: accepted
   :refines: FEAT_0001
   :links: REQ_0010, REQ_0002

   **Context.** Process death, process hang, and UI loss are distinct failure
   modes, and no single mechanism catches all three. Outputs must fail safe in
   every case.

   **Decision.** Layer three mechanisms: systemd ``Restart=always`` on both
   services (clean death), a hardware ``/dev/watchdog`` petted only by the
   daemon's healthy scan loop (hangs reboot the device), and an iceoryx2
   heartbeat for status display. The WAGO coupler's 50 ms SM watchdog drops the
   outputs to a safe state whenever frames stop, independent of software.
   Considered and rejected: software-only supervision — it cannot detect a
   daemon that is alive but hung.

   **Consequences.** ✅ Every failure mode leaves outputs safe within the
   coupler's 50 ms window. ✅ Hangs self-recover via reboot. ❌ A daemon hang
   reboots the whole device rather than recovering in place.

.. arch-decision:: Wrapping is operator-confirmed, never auto-triggered
   :id: ADR_0004
   :status: accepted
   :refines: FEAT_0001
   :links: REQ_0003

   **Context.** Wrapping is a physical actuation on the machine. The bale-full
   input is a sensor edge that could chatter or fire while an operator is at the
   back of the machine.

   **Decision.** Bale-full only arms the wrap softkey and pulses the keypad
   backlight; the 5 s wrap pulse fires solely on the operator's button press.
   Considered and rejected: auto-wrap on the full edge — convenient for
   hands-free operation but actuates with no human in the loop and is exposed to
   sensor chatter. (Auto-wrap is left as a possible future configurable mode.)

   **Consequences.** ✅ The machine never wraps without operator intent; a clean
   point exists to enforce pre-conditions. ❌ The operator must be present and
   press a button for every bale.

.. arch-decision:: No built-in updater — open a door, update over SSH
   :id: ADR_0005
   :status: accepted
   :refines: FEAT_0001
   :links: REQ_0009

   **Context.** Updates must happen over Ethernet, but a self-updating appliance
   carries heavy machinery: image signing, A/B rollback, and failure recovery.

   **Decision.** Ethernet mode only brings up a reachable static IP and displays
   it; a technician performs application and OS updates externally over SSH. The
   app contains no fetch/apply logic. Considered and rejected: a built-in updater
   that pulls and applies an image — large attack and failure surface,
   disproportionate for a single open-source machine.

   **Consequences.** ✅ Small, auditable surface; updates use standard SSH
   tooling. ❌ Updating requires a technician with network access; there is no
   one-button self-update.
