Features
========

Feature-level needs (``feat``) capture PRDs — the problem, solution, user
stories, and the decisions that shape an implementation. Requirements
(``req``) refine each feature.

.. needtable::
   :types: feat
   :columns: id;title;status
   :style: table

.. feat:: Baler control system for ifm CR1140
   :id: FEAT_0001
   :status: open

   **Problem Statement.**
   An operator running a baler needs a rugged, self-contained controller on the
   ifm CR1140 that can trigger wrapping, switch the knives, see whether the bale
   is full and where the knives are, and keep a running count of bales — without
   a CODESYS runtime. They also need a way to take the machine offline for
   firmware/application updates over Ethernet, because the CR1140 has a single
   network port shared between the EtherCAT fieldbus and normal IP networking.

   **Solution.**
   A two-process application on the CR1140. A ``baler-daemon`` owns the EtherCAT
   master (WAGO 750-354 coupler with 750-430 DI and 750-530 DO), the control
   state machine, the bale counters, and the network mode. A Slint
   ``baler-ui`` renders the 4.3" framebuffer and drives the 11-key keypad,
   talking to the daemon over iceoryx2 shared memory. Wrapping and knife
   switching are deliberate, operator-initiated 5 s output pulses. Taking the
   machine offline is an idle-only maintenance action that stops EtherCAT and
   brings up a static IP shown on screen, after which a technician updates the
   device over SSH.

   **User Stories.**

   1. As an operator, I want a clear on-screen indication when the bale is full,
      so that I know a wrap is needed.
   2. As an operator, I want the keypad backlight to pulse amber when the bale is
      full, so that I notice it from across the barn.
   3. As an operator, I want to start wrapping only by pressing a button, so that
      the machine never wraps without my intent.
   4. As an operator, I want the wrap output to be a fixed 5 s pulse, so that the
      baler's wrap cycle is triggered consistently.
   5. As an operator, I want the session and total bale counts to increase
      automatically after each wrap, so that I do not have to count manually.
   6. As an operator, I want to reset the session count at the start of a shift,
      so that I can track today's bales.
   7. As an owner, I want the total count to be protected from accidental reset,
      so that the lifetime odometer stays trustworthy.
   8. As an operator, I want the counts to survive a power cycle, so that a field
      restart does not lose my numbers.
   9. As an operator, I want to toggle the knives with a single button, so that I
      can switch them in or out on demand.
   10. As an operator, I want to see the live knife position, so that I know the
       current state before acting.
   11. As an operator, I want wrapping and knife switching to be independent, so
       that I can do either at any time.
   12. As a technician, I want to switch the machine to Ethernet from a service
       screen, so that I can update firmware or the application.
   13. As a technician, I want the static IP shown on screen in Ethernet mode, so
       that I can connect without guessing the address.
   14. As an operator, I want the machine to refuse the Ethernet switch unless it
       is idle, so that I never lose control mid-cycle.
   15. As an operator, I want outputs to drop to a safe state if the bus or
       controller fails, so that the machine cannot actuate uncommanded.
   16. As an operator, I want a clear fault screen when EtherCAT drops, so that I
       understand why controls are locked.
   17. As an operator, I want the machine to recover by itself when the bus comes
       back, so that a brief glitch does not require a restart.
   18. As an operator, I want commands blocked until the bus is healthy at boot,
       so that a half-initialised machine cannot actuate.
   19. As a maintainer, I want a UI crash to never affect output safety, so that a
       display bug cannot endanger the machine.
   20. As a maintainer, I want the UI to restart automatically if it dies, so that
       the operator regains the display without intervention.
   21. As a maintainer, I want the device to reboot if the daemon hangs, so that a
       stuck controller cannot leave the machine in limbo.
   22. As an installer, I want a documented IO wiring map, so that I can connect
       the WAGO modules correctly.
   23. As a maintainer, I want all knobs in one config file, so that I can adjust
       timings, IP, PIN, and IO bits without recompiling.

   **Implementation Decisions.**

   - **Two processes over iceoryx2.** ``baler-daemon`` is the safety authority
     (root / ``CAP_NET_RAW`` + ``CAP_NET_ADMIN``) and runs always; ``baler-ui``
     is a replaceable Slint front-end. A UI crash cannot affect outputs.
   - **Deep, pure control modules**: ``InputConditioner`` (debounce +
     R_TRIG edge detection), ``PulseEngine`` (5 s timed pulse with
     completed/aborted events), ``BalerState`` (master state machine
     Initializing → Idle → Full → Fault → EthernetMode), ``CounterStore``
     (atomic-persisted session/total). Edge modules: ``EtherCatIo`` (wraps the
     taktora ethercat-wago connector), ``NetworkMode`` (NIC EtherCAT↔static-IP),
     ``WatchdogPetter``, ``Ipc`` (iceoryx2 channels), ``baler-ui``.
   - **Operator-confirmed wrap, no auto-wrap.** Bale-full arms the wrap softkey
     and pulses the keypad backlight; the operator fires the 5 s pulse.
   - **Counting**: session + total increment once per ejected bale — the debounced
     rising edge of the baler-fully-open input (DI3/ch3), counted only while the bus
     is healthy. The wrap pulse no longer drives the count.
   - **Knife (directional)**: fire-and-forget 5 s pulse on the button rising edge,
     ch2 (DI2) selects the direction — true → knives-in (DO2), false → knives-out
     (DO3); the two are interlocked (never both on) but stay independent of the
     wrap pulse (knife + wrap may overlap).
   - **Inputs** are debounced, edge-detected, status-only; bale-full clears with
     its input.
   - **Counters** persist via temp-file + ``rename`` on each change, reload on
     boot. Session resets on operator action only and can be manually corrected
     ±1 (Main F4/F5, session-only, floors at zero); total reset is PIN-gated.
   - **Mode switch** is idle-only and behind the PIN-gated service screen. Entering
     Ethernet stops EtherCAT, brings up a configurable static IP (default on the
     ``192.168.1.x`` subnet), and displays it. No built-in updater; updates are
     done externally over SSH.
   - **Watchdog**: systemd ``Restart=always`` on both services; hardware
     ``/dev/watchdog`` petted only by a healthy daemon scan loop; iceoryx2
     heartbeat for status; coupler 50 ms SM watchdog as the output backstop.
   - **Boot/fault**: commands blocked until EtherCAT healthy; bus loss → fault
     (outputs dropped by watchdog, softkeys locked, inputs shown unknown,
     in-flight pulse aborted/uncounted); auto-clear to idle on recovery.
   - **UI**: softkey model (F1 Wrap, F2 Toggle Knives, F3 Reset Session,
     F4 Count +1, F5 Count −1, F6 Service), four screens (Main / Service-PIN /
     Ethernet / Fault overlay),
     LED beacon (green idle, amber-pulse full, red fault, blue Ethernet, white
     flash on pulse), blinking done in software.
   - **IO**: WAGO 750-354 / 750-430 / 750-530; DI1 = bale full, DI2/ch2 = knife
     position (24 V = in), DI3/ch3 = baler fully open (bale-eject / counting edge);
     DO1 = wrap, DO2 = knives-in, DO3 = knives-out; data at process-image byte
     offset 4; 10 ms scan, 50 ms watchdog; built on the taktora
     ``ethercat-wago-coupler`` example.
   - **Build/deploy**: cargo workspace, cargo-zigbuild →
     ``aarch64-unknown-linux-musl``, two ``Restart=always`` systemd units
     (daemon first), ``/etc/baler/config.toml``, counters in ``/var/lib/baler/``.

   **Testing Decisions.**

   - Good tests exercise external behaviour, not internals. The four pure modules
     are tested with synthetic scan ticks and a fake clock; no hardware needed.
   - **Unit tests**: ``InputConditioner`` (debounce + rising-edge over bit
     sequences), ``PulseEngine`` (5 s timing, completion vs abort, counting
     rule), ``BalerState`` (table-driven transition coverage), ``CounterStore``
     (persist/reload/reset against a temp dir).
   - **Integration test**: ``EtherCatIo`` against taktora's mock ``BusDriver``
     (decode DI byte at offset 4, encode outputs, health transitions).
   - ``NetworkMode`` and ``WatchdogPetter`` are covered by on-device smoke tests;
     ``baler-ui`` by visual checks.

   **Out of Scope.**

   - Any built-in firmware/application updater — updates are external over SSH.
   - OS/rootfs image update tooling (RAUC/swupdate/Mender) beyond guaranteeing
     network reachability.
   - Auto-wrap (hands-free) operation — reserved as a future configurable mode.
   - Use of the remaining DI 3–8 / DO 3–8 channels.
   - Any interlock between knife position and wrapping (operator responsibility).

   **Further Notes.**

   Open verification items tracked separately as issues: ``/dev/watchdog``
   availability on the CR1140 Yocto image; iceoryx2 0.8 discovery wiring between
   the two processes; whether the baler's IO is 24 V logic vs dry contacts; and
   the Slint framebuffer rendering path via ``cr1140-hal``'s ``FbDisplay``.
