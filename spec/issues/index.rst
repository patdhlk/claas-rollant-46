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
   :status: needs-triage
   :kind: improvement
   :links: REQ_0009, ISSUE_0009

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
