Issues
======

Issues (``issue``) live here. ``:status:`` carries the triage state machine:
needs-triage → needs-info | ready-for-agent | ready-for-human → in-progress →
done | wontfix. Edit status in place — git history is the audit trail.

.. needtable::
   :types: issue
   :columns: id;title;status;kind
   :style: table

.. issue:: Confirm /dev/watchdog is exposed by the CR1140 Yocto image
   :id: ISSUE_0001
   :status: done
   :kind: chore
   :links: REQ_0010

   **What to investigate.** The hardware-watchdog layer of the three-layer
   watchdog design assumes a usable ``/dev/watchdog`` on the device. Confirm the
   node exists on the CR1140 Yocto image, determine its timeout and whether the
   timeout is configurable, and document how the displaced CODESYS runtime
   previously armed and petted it (the CODESYS runtime was watchdog-protected).

   **Acceptance criteria.**

   - [x] ``/dev/watchdog`` (or ``/dev/watchdogN``) is confirmed present, or its
     absence is documented with the alternative.
   - [x] The watchdog timeout and its configurability are recorded.
   - [x] The pet/arm mechanism the daemon will use is documented.

   **Findings.** Confirmed on the device (kernel 5.19.16, ecomat-display): both
   ``/dev/watchdog`` (10,130) and ``/dev/watchdog0`` (248,0) are present. The
   ``WatchdogPetter`` arms by opening the node, pets by writing a non-``'V'``
   byte each healthy scan cycle, and sets the timeout itself via
   ``WDIOC_SETTIMEOUT`` (so the firmware default is not load-bearing; the daemon
   requests its own, e.g. 15 s). Magic-close defaults off so the watchdog stays
   armed if the daemon exits. Hardware-watchdog layer of REQ_0010 is unblocked.

   **Blocked by.** None.

.. issue:: Determine iceoryx2 0.8 discovery wiring between the two processes
   :id: ISSUE_0002
   :status: done
   :kind: question
   :links: REQ_0001

   **What to investigate.** The two-process architecture needs the daemon and
   UI to share an iceoryx2 configuration. Determine from the taktora
   ethercat-wago-coupler example and taktora source whether iceoryx2 0.8 runs
   discovery in-process or requires a separate roudi daemon, and what startup
   ordering that implies for the ``baler-daemon`` and ``baler-ui`` systemd units.

   **Acceptance criteria.**

   - [x] The iceoryx2 0.8 discovery model (in-process vs roudi) is documented.
   - [x] The required systemd ordering / dependency between the two units is
     specified.
   - [x] Any shared iceoryx2 config the two processes must agree on is listed.

   **Findings.** iceoryx2 0.8 is **fully decentralized — no central daemon
   (no RouDi)** is needed for pub/sub. Discovery happens in-process, lazily, when
   each process calls ``node.service_builder(name).publish_subscribe::<T>()
   .open_or_create()``; resource cleanup is peer-driven. The optional
   ``iox2 service discovery`` service is only for *enumerating* live endpoints
   and is not needed for two fixed peers with known names. taktora pins a patched
   iceoryx2 0.8 and its transport lives in ``taktora-connector-transport-iox``
   (payloads wrapped as ``ConnectorEnvelope<N>``). Note: the
   ``ethercat-wago-coupler`` example is a **single binary**, so the daemon+UI
   split is new design, not copied from an existing example.

   *systemd ordering:* not required for correctness (``open_or_create`` is
   symmetric — whoever starts first creates the service). Recommended: start
   ``baler-daemon`` first with a soft ``After=baler-daemon.service`` +
   ``Wants=`` on ``baler-ui`` so the UI does not spin at boot. The one real
   constraint is the shared-memory **root path** (default ``/tmp/iceoryx2/``):
   make it a stable, writable directory via ``RuntimeDirectory=`` / tmpfiles
   rather than relying on ``/tmp``.

   *Shared config both processes must agree on:* the same ``iceoryx2.toml``
   (ship ``/etc/iceoryx2/iceoryx2.toml``); ``global.root-path``;
   ``global.prefix`` (doubles as domain/instance id); the ``ipc::Service``
   variant (not ``local``); identical service name(s); the same
   ``publish_subscribe`` pattern, payload type, and buffer sizes.

   *Open follow-up:* confirm where taktora constructs the ``Node`` / loads its
   ``Config`` (whether it sets a custom root-path or uses defaults) before the
   systemd units are finalised. Sources: iceoryx2 FAQ & config README
   (github.com/eclipse-iceoryx/iceoryx2), ekxide 0.4/0.6 release notes,
   taktora repo (github.com/patdhlk/taktora).

   **Blocked by.** None — can start immediately.

.. issue:: Confirm baler IO is 24V logic, not dry contacts
   :id: ISSUE_0003
   :status: ready-for-human
   :kind: question
   :links: REQ_0015

   **What to investigate.** The WAGO 750-430 / 750-530 wiring assumes the
   baler's full and knife-position signals are 24 V inputs and its wrap/knife
   command inputs accept 24 V sourcing outputs. Confirm from the baler's
   electrical documentation. If any signal is a dry contact or a different
   voltage level, interposing relays or signal conditioning must be added to the
   wiring plan.

   **Acceptance criteria.**

   - [ ] The electrical type of each of the four signals (full, knife position,
     wrap command, knife command) is confirmed.
   - [ ] Any interposing relays or conditioning needed are added to the wiring
     plan, or it is confirmed none are required.

   **Blocked by.** None — can start immediately (requires baler datasheet).

.. issue:: Confirm the Slint framebuffer rendering path via cr1140-hal FbDisplay
   :id: ISSUE_0004
   :status: done
   :kind: question
   :links: REQ_0013

   **What to investigate.** The ``baler-ui`` rendering approach depends on how
   Slint output reaches the CR1140 framebuffer. Confirm from the UpTux
   ifm-cr1140 demo how a Slint scene is rendered into the ``cr1140-hal``
   ``FbDisplay`` surface (xRGB8888, double-buffered, ``copy_from`` blit, then
   ``present``), and how keypad events from ``ButtonReader`` are fed into Slint.

   **Acceptance criteria.**

   - [x] The Slint-to-framebuffer rendering path (software renderer → surface
     blit → present) is documented.
   - [x] The keypad-event-to-Slint input path is documented.
   - [x] Any backend/feature flags Slint needs for this target are listed.

   **Findings.** The UpTux demo uses Slint's **pure-Rust software renderer**
   behind a custom ``slint::platform::Platform`` (``FbPlatform`` holding a
   ``MinimalSoftwareWindow``), full-frame render-to-buffer (not
   ``render_by_line``). The render loop: ``window.draw_if_needed(|r|
   r.render(&mut buf, pixel_stride))`` → ``fb.surface().copy_from(bytes,
   stride)`` → ``fb.present()``. A ``#[repr(transparent)]`` ``Xrgb8888`` u32 is
   the Slint ``TargetPixel`` and matches the framebuffer layout, so there is no
   per-pixel conversion. The platform is installed with
   ``slint::platform::set_platform`` and the app drives its own super-loop (no
   ``run_event_loop``).

   *Keypad input path:* a **custom state model, not Slint key events.** The loop
   polls ``cr1140_hal::input::ButtonReader``; on ``ButtonEvent::Pressed(btn)`` it
   sets Slint properties and performs direct HAL actions, matching
   ``Button::{F1..F6, Up, Down, Left, Right, Enter}``. There is no
   ``window.dispatch_event`` / ``WindowEvent::KeyPressed`` — this is exactly the
   softkey/navigation model REQ_0013 calls for.

   *Required Slint flags* (slint ``1.12``, ``default-features = false``):
   ``compat-1-2``, ``unsafe-single-threaded``, ``libm``, ``renderer-software``;
   no ``backend-default`` / winit / linuxkms / wgpu / ``std`` (``std`` would pull
   fontconfig and break the static-musl cross build). Fonts are embedded at build
   time via ``build.rs``.

   Sources: raw.githubusercontent.com/UpTux/ifm-cr1140/main/ —
   ``cr1140-slint/src/{platform.rs,pixel.rs}``,
   ``cr1140-slint/Cargo.toml``, ``cr1140-slint-demo/src/main.rs``,
   ``cr1140-hal/src/display/surface.rs``, ``docs/slint-spike.md``.

   **Blocked by.** None — can start immediately.

.. issue:: baler-daemon EtherCAT never reaches OP — run the taktora executor as the main loop
   :id: ISSUE_0009
   :status: done
   :kind: bug
   :links: REQ_0015

   **What to fix.** On the CR1140, ``baler-daemon`` built with the ``ethercat``
   feature never brings the WAGO 750-354 up: the connector sends one initial
   ``BRD`` and then stays ``Down`` with ``ethercrab: Timeout(Pdu)``, so no
   SubDevice is enumerated and no I/O flows. The cause is the execution model in
   ``EtherCatIo`` (``baler-daemon/src/ethercat_io.rs``): the taktora ``Executor``
   is run on a **detached background thread** (``thread::spawn(|| exec.run())``)
   while the daemon's synchronous 10 ms scan loop polls the connector through the
   iceoryx2 channel handles. On that background thread ``exec.run()`` does not
   schedule the registered interval items — neither the health pump / stop item
   nor, critically, the connector's own cyclic PDI driving — so bring-up never
   advances past the construction-time frame. Fix by mirroring the proven
   ``taktora examples/ethercat-wago-coupler``: run ``exec.run()`` as the daemon's
   main loop and drive the 10 ms control cycle as an executor item (the
   mirror-item equivalent), reading/writing the WAGO process image inside that
   item, rather than bridging to an external sync loop.

   **Evidence (on-device, 2026-06-15).** The known-good example binary,
   cross-built for ``aarch64-unknown-linux-gnu`` and run on this CR1140 (eth0,
   same coupler), reached ``Connecting -> Up`` and ran the live DI->DO mirror —
   proving the NIC, ``fec`` raw-socket path, coupler, port, power, and cabling are
   all good. Our daemon, under the same connect cycles, only ever logged the
   single ``BRD`` and ``Timeout(Pdu)``; its health-pump and stop items never ran
   (the latter forced a 90 s SIGTERM->SIGKILL on shutdown), confirming the
   background-thread executor does not run its items. ``worker_threads(2)`` and a
   multi-threaded tokio runtime did not change the symptom — it is the execution
   model, not worker count. Raw capture under ``bringup-logs/`` (gitignored).

   **Design (confirmed).** Split paths, sharing one control-cycle routine:

   - Extract the body of the current 10 ms scan ``loop`` into a shared
     control-cycle unit (a ``Control`` struct owning ``BalerState``, the two
     ``PulseEngine``\ s, the two ``Debouncer``\ s, ``CounterStore``, and
     ``last_ip``, with a ``step(inputs, commands, &mut net) -> (Outputs,
     StateSnapshot)`` method) so the logic is identical on both paths.
   - **Sim / host build** (no ``ethercat`` feature): keep today's manual
     ``loop { poll; step; write; publish; sleep }`` — no taktora dependency.
   - **EtherCAT build**: build the taktora ``Executor`` in ``main``,
     ``register_with`` the connector, add the control cycle as a 10 ms executor
     item (reading/writing the WAGO process image inside the item, plus the
     transport publish and ``wd.pet()``), add the health pump, and call
     ``exec.run()`` on the **main thread**. Remove the background-thread spawn in
     ``EtherCatIo``; rework its surface so it registers the connector into the
     caller's executor and hands back reader/writer/health handles (the current
     ``BusIo`` poll/write-from-outside shape conflicts with executor-driven IO).
   - Drop the diagnostic-only ``BALER_DIAG_EXIT_SECS`` restart-until-up shim once
     real bring-up works; keep the health-reason logging.

   **Acceptance criteria.**

   - [ ] The ``ethercat`` (and ``hardware``) build of ``baler-daemon`` reaches
     connector health ``Up`` against the WAGO 750-354 on the CR1140 within a few
     seconds of the coupler being present.
   - [ ] Debounced 750-430 inputs (DI1 bale-full, DI2 knife) reach the control
     state machine and the published ``StateSnapshot``; 750-530 outputs (DO1 wrap,
     DO2 knife) are driven from the pulses.
   - [ ] The daemon shuts down cleanly (no SIGKILL hang) and recovers on
     unplug/replug of the coupler.
   - [ ] Validated on-device with a captured ``-> Up`` + I/O log.

   **Blocked by.** None — root cause confirmed; can start immediately.

   **Resolution (2026-06-15).** Fixed and validated on-device (reached ``Up`` in
   ~3 s, DI→DO mirror confirmed; capture in ``bringup-logs/``). Two layers:

   - *Execution model* (the filed bug): extracted the control cycle into
     ``Control::step`` (``baler-daemon/src/control.rs``, unit-tested) and split the
     run paths — ``run`` keeps the sim loop, ``run_ethercat`` builds the taktora
     ``Executor``, registers the WAGO connector + health pump via
     ``ethercat_io::register``, and runs ``exec.run()`` on the **main thread** with
     the 10 ms control cycle as an executor item. The background-thread executor is
     gone. iceoryx2 ports are ``!Send`` (internal ``Rc``), so the transport
     publisher/subscriber run on a dedicated relay thread bridged by ``Send`` mpsc
     channels rather than inside the (``Send``) executor item.
   - *Boot link-race* (surfaced once the diagnostic ``BALER_DIAG_EXIT_SECS`` shim
     was dropped): ethercrab enumerates the bus once and the daemon's scan fired
     before ``eth0`` had carrier (``ENETDOWN``), wedging it ``Down`` forever.
     ``run_ethercat`` now waits for the link (carrier) before constructing the
     connector, and re-introduces a (non-diagnostic) restart-until-first-``Up``
     backstop: if not ``Up`` within 12 s it exits non-zero so systemd respawns a
     fresh scan (unit sets ``StartLimitIntervalSec=0``).

   Deploy: ``src/deploy/deploy-ethercat.sh`` + ``baler-ethercat.service`` (enabled
   on boot; EtherCAT is the device's normal run mode).

.. issue:: EtherCAT connector keeps flapping recovery in Ethernet/maintenance mode
   :id: ISSUE_0010
   :status: in-progress
   :kind: improvement
   :links: REQ_0009, ISSUE_0009, ISSUE_0011

   **What.** Follow-up from ISSUE_0009. The ``netmode-hw`` switch
   (``NetworkController::enter_ethernet`` / ``enter_ethercat``,
   ``baler-daemon/src/network_mode.rs``) is purely IP-level: it brings the static
   IP up/down but does **not** stop or recreate the taktora EtherCAT master. So in
   Ethernet maintenance mode (and any time the coupler is absent) the connector
   keeps running on ``eth0`` and flaps ``Connecting -> Degraded`` on every recovery
   attempt, logging each transition. Observed on-device 2026-06-15 after switching
   back to Ethernet: repeated ``recover failed: ethercrab: Timeout(Pdu)`` (with
   exponential backoff, so bounded — but noisy, and the raw socket stays active on
   the maintenance link).

   **Why it matters.** REQ_0009 and the ``ports.rs`` doc both state the intent that
   "the EtherCAT master is stopped/recreated by the daemon around these calls" —
   currently unimplemented. A flapping master on the maintenance NIC is untidy and
   could interfere with maintenance traffic; it also means EtherCAT health while in
   Ethernet mode is meaningless noise.

   **Possible fix.** On ``SwitchToEthernet``, pause/stop the executor's connector
   driving (or drop + later rebuild the connector) so the raw socket is released;
   on ``SwitchToEthercat`` (or reboot), recreate it. Needs a clean way to
   stop/restart just the connector within the running executor, or to gate the
   control item so it stops pumping the bus while in Ethernet mode.

   **Blocked by.** None.

   **Implementation (2026-06-15, host-tested; on-device validation pending).**
   New ``BusController`` port (``baler-daemon/src/ports.rs``) carries the EtherCAT
   *master* lifecycle, kept separate from the purely-L3 ``NetworkController``.
   ``Control::step`` drives it on the same once-per-transition actions that drive
   the NIC switch: ``SwitchToEthernet`` → ``bus.suspend()`` (stop the master before
   bringing the static IP up), ``SwitchToEthercat`` → ``bus.restart()``. The
   host/sim build uses a no-op ``NoopBus``; the ``ethercat`` build implements it on
   ``WagoBus`` — ``suspend`` calls the connector's ``stop_dispatcher()`` (the
   dispatcher loop exits, releasing the raw socket and ending the
   ``Connecting → Degraded`` flapping), and ``restart`` latches a flag the
   ``run_ethercat`` loop drains to exit non-zero so systemd respawns a fresh
   process. The single-enumeration restart reuses the ISSUE_0009 bring-up backstop:
   ethercrab enumerates once per process, so a return to EtherCAT can only come up
   on a fresh scan.

   *State-machine gate (found during the first on-device run).* The first deploy
   surfaced that ``EnterEthernet`` was only accepted from ``Mode::Operational``
   (``baler-core/src/state.rs``): an operator pressing the softkey while the coupler
   was already absent (bus in ``Fault``) had the command rejected ``NotReady``, so
   the master never suspended — exactly the "any time the coupler is absent" case
   this issue names. ``EnterEthernet`` now switches to maintenance mode from any
   non-Ethernet mode (Operational-idle, Fault, Initializing); only ``AlreadyEthernet``
   and the pulse-active ``NotIdle`` guard remain. The ``run_ethercat`` bring-up
   backstop is suppressed once maintenance mode is entered, so entering it at boot
   (coupler never up) does not get rebooted away.

   *UI gate (found during the second on-device run).* The operator panel
   (``baler-ui``) pinned itself to a modal ``Fault`` screen whenever the bus was
   faulted and swallowed every key (``Nav::Fault => {}``), so the operator could not
   reach Service → Ethernet maintenance mode precisely when the bus was flapping —
   the UI mirror of the daemon gate above. Fixed by extracting the mode→screen
   transition into a pure, unit-tested ``next_nav`` that never traps the operator in
   the Service menu, plus an ``F6 → Service`` route (and on-screen hint) on the Fault
   screen.

   Also learned on-device: ``eth0`` carries the ``192.168.1.102/24`` static IP from
   the OS network config at boot, independent of the daemon — so the box is
   reachable on that IP even in EtherCAT mode, and ``enter_ethernet`` is not what
   makes it reachable.

   Covered by unit tests: ``control.rs`` (``FakeBus`` recorder) — entering Ethernet
   suspends exactly once and never restarts, returning restarts exactly once without
   a second suspend, ordinary cycles leave the master untouched; ``state.rs`` —
   ``EnterEthernet`` accepted from ``Fault`` and ``Initializing``; ``baler-ui``
   ``next_nav`` — a ``Fault`` never traps the operator off the Service menu. Builds
   clean across ``ethercat`` / ``hardware`` / ``ethercat,transport`` and
   ``baler-ui --features hardware``.

   **On-device end-to-end validation: deferred (blocked by ISSUE_0011).** Exercising
   the suspend through the real panel needs the ``baler-ui`` ⇄ ``baler-daemon``
   iceoryx2 link, which was discovered to have never worked on this device: the two
   units race to open/create the shared ``baler/state`` service at boot and corrupt
   it (``ServiceInCorruptedState``), so the daemon's relay dies and it runs deaf and
   mute — no published state, no received commands. That is a separate, pre-existing
   transport-bring-up bug (masked until now because the panel was the standalone
   ``--features device`` sim that never used the link); tracked as ISSUE_0011. Once
   it is fixed, validate: coupler attached → ``Up`` → press EnterEthernet → confirm
   ``EtherCAT master suspended`` and **no further** ``Timeout(Pdu)`` flapping, then
   return to EtherCAT and confirm a fresh ``Up``.

   The code fix (daemon suspend/restart + the two gate relaxations) is complete and
   unit-tested; closing here on that basis with on-device validation tracked under
   ISSUE_0011.

   **REOPENED — on-device validation 2026-06-15 (after ISSUE_0011 fixed): suspend
   did NOT quiet the bus.** With the real ``baler-ethercat`` + transport ``baler-ui``
   running on the CR1140, EtherCAT reached ``Up``, the panel showed live state, and
   the operator's EnterEthernet keypress reached the daemon (``EtherCAT master
   suspended`` logged — proving the ISSUE_0011 link works end-to-end). **But the
   ``Timeout(Pdu)`` flapping continued for minutes after the suspend log.** Root
   cause traced into taktora ``connector-ethercat``: ``suspend()`` called only
   ``connector.stop_dispatcher()``, which sets a stop flag the ``dispatcher_loop``
   checks *between cycles* — but once the coupler drops, ``CycleRunner::tick`` enters
   ``recover_per_policy`` (``runner.rs``), an **infinite reconnect-backoff loop that
   never re-checks the stop flag**, so the runner is parked there and the flag is
   never seen.

   *Fix (2026-06-15).* ``WagoBus.connector`` is now an ``Option``; ``suspend()``
   **drops** the connector (after a best-effort ``stop_dispatcher``). Dropping it
   drops the ``EthercatGateway``, whose ``Drop`` runs ``runtime.shutdown_timeout``,
   aborting the dispatcher + ethercrab tx/rx wherever parked and closing the raw
   socket — which actually quiets ``eth0``. The drop runs on a detached thread so
   the gateway's blocking shutdown never stalls the 10 ms control cycle (or starves
   the watchdog); ``poll``/``write``/``is_healthy`` short-circuit to "bus down" once
   suspended.

   *UI navigation (found in the same run).* Returning to EtherCAT brought the bus
   back ``Up`` but the panel didn't switch back — and entering Ethernet from the
   Service menu didn't move to the Ethernet screen. ``next_nav`` (``baler-ui``) had a
   blanket "stay in Service" short-circuit; it now *follows the mode*: Ethernet →
   the Ethernet screen (even from Service), recovered-out-of-Ethernet → Main
   (mirroring Fault recovery), while still never yanking the operator off Service
   during a **Fault**. Six ``next_nav`` unit tests green.

   Both fixes deployed (``baler-ethercat`` + ``baler-ui``, cross-built gnu, host
   tests green: baler-core 31 / daemon 10 / ui 6). **Re-validation pending** on the
   next coupler-attached reboot: EnterEthernet → ``suspended`` → flapping **stops**,
   panel shows the Ethernet screen; ReturnToEthercat → fresh ``Up`` → panel returns
   to Main.

.. issue:: baler-ui ⇄ baler-daemon iceoryx2 link never connects (baler/state startup race)
   :id: ISSUE_0011
   :status: done
   :kind: bug
   :links: ISSUE_0010, ISSUE_0002

   **What.** In the two-process device build (``baler-ethercat`` daemon +
   ``baler-ui`` operator panel, both ``--features hardware``/``transport``), the UI
   never receives daemon state and the daemon never receives UI commands. On the
   CR1140 the panel sits permanently on ``Initializing`` while the bus is actually
   ``Up``, and operator softkeys (Wrap, ToggleKnife, EnterEthernet, …) have no
   effect — the daemon is deaf and mute.

   **Root cause (on-device, 2026-06-15).** ``baler-ui`` and ``baler-ethercat`` start
   concurrently at boot with no ordering and both *open-or-create* the shared
   iceoryx2 pub/sub service ``baler/state``. They race and corrupt it: the daemon's
   transport relay fails at construction with
   ``PublishSubscribeOpenError(ServiceInCorruptedState)``, logs ``[transport] state
   publisher failed: …`` once, and the relay thread returns — so the daemon runs
   with **no** state publisher and **no** command receiver for the rest of the
   process lifetime. The UI subscriber then has no publisher to read, so its
   ``IpcBackend`` never advances past the initial ``Mode::Initializing``. Observed:
   ``baler-ui`` started 12:28:20, daemon relay publisher failed 12:28:23 (same
   boot); a stale-resource variant also crash-loops the UI
   (``NRestarts`` in the hundreds) until the iceoryx2 tmpfs state under
   ``/tmp/iceoryx2`` is wiped (it is ``tmpfs``, so a reboot clears it — but the race
   re-corrupts on the next boot).

   **Why it was never seen before.** The deployed panel was the standalone
   ``baler-ui --features device`` build (in-process control, **zero** iceoryx2),
   which never used the link. The two-process transport path has therefore never
   actually run on this device. Surfaced while validating ISSUE_0010 (which needs
   the UI to deliver ``EnterEthernet`` to the daemon).

   **Possible fix.** Order the units (``baler-ui`` ``After=baler-ethercat.service``)
   and make the publisher the definitive creator before the subscriber opens; and/or
   make the ``baler-ipc`` transport bring-up robust to the race — clean a corrupted
   service and retry open-or-create with backoff rather than failing fatally, and do
   not let a relay/publisher construction failure permanently disable the daemon's
   transport (retry, or treat the relay as restartable). Consider an
   ``ExecStartPre`` clean-slate of ``/tmp/iceoryx2`` and a readiness wait.

   **Blocked by.** None. **Blocks** on-device end-to-end validation of ISSUE_0010.

   **Implementation (2026-06-15, host + cross-compile tested; on-device validation
   pending).** Root-caused as two parallel iceoryx2 stacks: the EtherCAT bus was
   already on taktora (``taktora-connector-ethercat``), but the UI⇄daemon link was
   a hand-rolled ``baler-ipc`` iceoryx2 wrapper bridged into the executor by a
   dedicated relay thread + mpsc channels — the relay existed only because
   ``baler-ipc``'s ports held an ``Rc`` and were ``!Send``. taktora already ships
   the intended transport (``taktora-connector-transport-iox``: ``ServiceFactory``
   + ``ChannelWriter`` / ``ChannelReader``), whose handles **are** ``Send``.

   *Re-architecture.* Deleted the ``baler-ipc`` crate. Its contract types
   (``Mode`` / ``Command`` / ``KnifePos`` / ``StateSnapshot``) moved into
   ``baler-core`` as plain ``serde`` types (the iceoryx2 ``ZeroCopySend`` /
   ``#[repr(C)]`` are gone — taktora frames a serialised payload inside its own
   envelope). The UI⇄daemon state/command now flow over two ``transport-iox``
   pub/sub services (``baler.state``, ``baler.command``) encoded with a new
   host-tested ``PostcardCodec`` (``baler-core::codec``). The daemon opens a
   ``DaemonLink`` and pumps it **inline in the executor control item** — the relay
   thread and mpsc bridge are deleted; the sim-loop path uses the same link. The
   UI's ``IpcBackend`` opens the mirror handles.

   *Race fix.* Both sides still ``open_or_create`` (taktora's ``ServiceFactory``
   does too), so the boot race is defeated by ``baler-core::bringup::retry_open``:
   on a corruption error it runs iceoryx2's ``Node::cleanup_dead_nodes`` (a
   *targeted* clear of resources a half-creating/dead process left behind —
   strictly better than the ``rm -rf /tmp/iceoryx2`` the original note floated,
   which would clobber a live daemon's services) and retries with linear backoff,
   rather than dying deaf-and-mute. systemd now orders ``baler-ui``
   ``After=baler-ethercat.service``/``baler-daemon.service`` so the daemon (the
   definitive creator) starts first; ``After=`` only orders the start, so
   ``retry_open`` bridges the readiness gap while the daemon brings the bus up.

   *Tests.* Host unit tests for the two pure, extracted pieces: ``codec`` —
   ``StateSnapshot`` / ``Command`` round-trip and too-small-buffer rejection;
   ``bringup::retry_open`` — ok-first-try, give-up-after-max, clean-on-corruption
   then recover, no-clean-on-transient. Full host suite green (``baler-core`` 31,
   ``baler-daemon`` 10, ``baler-ui`` 5). Cross-compiled clean for
   ``aarch64-unknown-linux-gnu`` across ``baler-daemon --features hardware`` and
   ``transport,watchdog-hw`` and ``baler-ui --features hardware`` and ``device`` —
   confirming ``DaemonLink`` satisfies the ``Send`` bound to live in the executor
   item and that the new code builds against real iceoryx2 + taktora.

   **On-device validation: pending (manual).** iceoryx2 cannot run on the macOS
   host, so the real two-process link is validated only on the CR1140: deploy via
   ``deploy/deploy-2proc.sh``, confirm the panel leaves ``Initializing`` and shows
   live bus state, softkeys reach the daemon, and a reboot no longer corrupts
   ``baler.state``. This same run also clears ISSUE_0010's deferred on-device
   validation (press EnterEthernet → ``EtherCAT master suspended``, no further
   ``Timeout(Pdu)`` flapping; return to EtherCAT → fresh ``Up``).

   The code re-architecture + race fix are complete and host/cross validated;
   closing on that basis with the on-device run tracked in this note.

.. issue:: Operator-flow features: F1-always, full-attention latch, IO test page
   :id: ISSUE_0012
   :status: done
   :kind: feature
   :links: REQ_0013, REQ_0017, REQ_0018

   **What.** Three operator-requested features for the panel (design agreed
   2026-06-15), to be implemented test-first.

   **1. F1 wrap always available (REQ_0013).** Firing a wrap is the operator's
   responsibility. Drop the ``if snap.wrap_armed`` guard on the ``baler-ui`` F1
   handler so F1 always issues ``Command::Wrap``; the daemon already accepts it
   whenever ``Operational`` (``state.rs`` gates only on operational, not on
   ``bale_full``). ``wrap_armed`` becomes a purely advisory on-screen hint.

   **2. Bale-full attention latch (REQ_0017).** Operators sometimes look away, so
   a true DI1 must stay visible. Add a latch in ``control.rs``: a rising debounced
   DI1 starts a ≥20 s countdown (2000 × 10 ms cycles); a new ``StateSnapshot``
   field ``full_latched`` is true while it runs and the Main screen renders "FULL"
   from it. A wrap firing (``Action::FireWrap``) clears the latch early; DI1
   dropping sooner does not. Pure/host-testable with synthetic ticks.

   **3. Manual IO test page (REQ_0018).** A PIN-gated ``Nav::IoTest`` screen
   (new ``Screen::IoTest``) reached from Service. Outputs are momentary: the UI
   tracks held F1/F2 (the HAL emits ``Pressed``/``Released``) and each frame sends
   ``Command::ManualIo { wrap, knife }`` with the held bits. The daemon enters a
   manual-IO mode on receiving ``ManualIo`` — the normal state machine is suspended
   and outputs are driven from the latest bits — guarded by a command watchdog
   (≤ 300 ms): if the stream stops (operator leaves the page, key released, UI
   crash, link drop) outputs de-energize and normal control resumes. Manual outputs
   apply only while ``Operational``. Add raw ``di1``/``di2`` bits to the snapshot so
   the page shows live, un-debounced inputs. ``Command`` is now a ``serde`` enum
   (post-ISSUE_0011), so the new fielded variant needs no special layout.

   **Blocked by.** None. Implement via ``/tdd`` (host tests for the latch, the
   manual-IO watchdog/mode, and ``next_nav`` IoTest routing; cross-build the
   device/transport feature sets).

   **Implementation (2026-06-15, host + cross tested; on-device IO-page check
   pending).** All three shipped test-first.

   *F1 always (REQ_0013).* Dropped the ``if snap.wrap_armed`` guard on the
   ``baler-ui`` Main F1 handler — F1 always issues ``Command::Wrap``; the daemon
   already accepts it whenever ``Operational``. ``wrap_armed`` is now a pure hint.

   *Full latch (REQ_0017).* ``control.rs`` holds a ``full_latch_remaining``
   countdown (``FULL_LATCH_CYCLES = 2000`` = 20 s) re-armed while debounced DI1 is
   true and counted down after; new snapshot field ``full_latched`` drives the
   Main "FULL" indicator. A wrap (``Action::FireWrap``) clears it early. Tests:
   holds ≥20 s after DI1 clears; a wrap clears it.

   *Manual IO test page (REQ_0018).* New ``Command::ManualIo { wrap, knife }``
   (serde enum, no layout constraints post-ISSUE_0011), intercepted in
   ``control.rs`` before the state machine. Receiving it (re)arms a 300 ms
   watchdog (``MANUAL_IO_WATCHDOG = 30``) and suspends normal control, driving
   outputs straight from the bits — but only while ``Operational``. When the
   stream stops the watchdog de-energizes and resumes normal control. Snapshot
   gained raw ``di1``/``di2``. UI: ``Nav::IoTest`` + ``Screen::iotest`` (Slint),
   reached from Service via PIN-gated F3; the loop tracks held F1/F2 (HAL
   ``Pressed``/``Released``) and streams ``ManualIo`` every frame, F6 exits.
   Tests: manual-IO drives outputs; watchdog de-energizes + restores control when
   the stream stops; outputs suppressed on an unhealthy bus; ``next_nav`` keeps
   IoTest while operational and yields to a Fault.

   Host green (baler-core 31, daemon 15, ui 7); cross-built clean (gnu) for
   baler-daemon ``hardware`` and baler-ui ``device`` + ``hardware``. On-device
   validation of the IO page (momentary energize, live inputs, fail-safe on exit)
   is the remaining manual check.

.. issue:: Restore the EN/DE language toggle onto the current UI
   :id: ISSUE_0013
   :status: done
   :kind: feature
   :links: REQ_0019

   **What.** The English/German language toggle (full i18n of the operator UI,
   persisted, German default) existed on the unmerged ``feat/ui-language-toggle``
   branch but was never on ``main`` — so the deployed transport UI had no language
   switch. Restore it onto the current UI (which has since gained the transport
   re-architecture, the IO test page, and the ``next_nav`` rework), resolving the
   collisions the stale branch could not be merged through.

   **Implementation (2026-06-16).** Brought ``src/baler-ui/src/i18n.rs`` forward
   (``Lang`` En/De, ``Strings`` table, EN/DE, persistence helpers) and extended it
   for the two labels the branch predated — the Service ``IO TEST`` softkey and the
   Fault ``F6 → Service`` hint; its translation-guard tests (every field non-empty,
   EN ≠ DE, field-count) pass on the host (15 i18n tests). Converted every
   operator-screen label in ``baler.slint`` to an ``I18n`` (``tr``) struct set as a
   whole from Rust each frame; ``main.rs`` resolves mode/knife/fault text and the
   ``tr`` struct from the active table, loads/saves the choice
   (``/var/lib/baler/language`` + ``/tmp`` fallback, German default), and toggles on
   **Service F4** (PIN-free, persisted immediately). ``lang`` is part of the view
   dedup key so a toggle repaints instantly.

   **Key-collision resolution.** The branch toggled language on Service F3, but
   ISSUE_0012 put the IO test entry there. Per the agreed decision the IO test
   stays on **F3** (PIN-gated) and the language toggle moved to **F4** (PIN-free).
   The IO test page itself stays English (technician screen; REQ_0018).

   Host green (baler-ui 22 incl. 15 i18n); cross-built clean (gnu) for baler-ui
   ``device`` + ``hardware``. The stale ``feat/ui-language-toggle`` branch (and its
   colliding ISSUE_0005-0008 / REQ_0017-0019 / FEAT_0002 spec IDs) is superseded by
   this issue and REQ_0019 and can be deleted.
