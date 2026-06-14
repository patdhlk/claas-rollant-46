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

.. issue:: i18n string table and host tests for English/German UI text
   :id: ISSUE_0005
   :status: done
   :kind: feature
   :links: REQ_0017

   **Parent.** FEAT_0002.

   **What to build.** Introduce a host-testable ``i18n`` module in ``baler-ui``
   that is the single in-binary source of truth for the static UI text in both
   English and German. It provides a ``Lang`` type distinguishing the two
   languages with conversion to and from a language code (``en`` / ``de``, unknown
   codes falling back to the default), and a ``Strings`` table that exposes every
   static UI label with an English and a German value drawn from one place per
   language. The module is pure Rust — no Slint, no filesystem — so it compiles
   and is tested on the host as well as the device.

   **Acceptance criteria.**

   - [ ] A ``Lang`` type distinguishes English and German and converts to/from a
     language code, with unknown codes falling back to the default (German).
   - [ ] A ``Strings`` table provides every static UI label with both an English
     and a German value, sourced from one place per language.
   - [ ] Host unit tests assert every ``Strings`` field is non-empty in both
     languages and that the language-code conversion round-trips, including the
     unknown-code-to-default case.
   - [ ] ``cargo test`` passes on the host without the ``device`` feature.

   **Blocked by.** None — can start immediately.

.. issue:: Render the baler-ui in both English and German from the i18n table
   :id: ISSUE_0006
   :status: in-progress
   :kind: feature
   :links: REQ_0017

   **Parent.** FEAT_0002.

   **What to build.** Make ``baler-ui`` render entirely from the i18n table so the
   panel can display in German or English. Replace the hard-coded literals in the
   Slint view with a single ``I18n`` struct fed from Rust through one ``tr``
   property; add a hidden glyph-anchor ``Text`` so the software-renderer build
   embeds the German characters ``Ä Ö Ü ä ö ü ß``; route the active language
   through ``push_view``; and make the snapshot-derived mode, knife, and fault
   texts language-aware. The language is fixed to the compile-time default
   (German) in this slice — the operator toggle comes later. Verify on device that
   German renders with umlauts and that the longer labels fit the layout.

   **Acceptance criteria.**

   - [ ] Every operator-facing string on the main, service, Ethernet, and fault
     screens comes from the i18n table; no user-facing literal remains hard-coded
     in the view.
   - [ ] The mode, knife, and fault texts driven from Rust display in the active
     language.
   - [ ] German characters ``Ä Ö Ü ä ö ü ß`` render correctly in the
     software-rendered build (glyph anchor embedded).
   - [ ] With the default language the whole UI displays in German; changing the
     default to English displays the whole UI in English.
   - [ ] On-device check confirms German labels fit the fixed-width softkeys
     (126 px, word-wrap) and the mode/fault banners without clipping.

   **Blocked by.** ISSUE_0005.

.. issue:: PIN-free language toggle softkey on the service screen
   :id: ISSUE_0007
   :status: ready-for-agent
   :kind: feature
   :links: REQ_0018

   **Parent.** FEAT_0002.

   **What to build.** Add a PIN-free language toggle on the service screen. The
   first free service softkey (F3) flips the display language between English and
   German, takes effect immediately across all screens, and issues no control
   command. It is enabled regardless of PIN unlock — unlike Reset Total (F1) and
   the network switch (F2) — and is labelled with the language it will switch to
   (``DEUTSCH`` when currently English, ``ENGLISH`` when currently German). The
   selection is in-memory in this slice; persistence comes next.

   **Acceptance criteria.**

   - [ ] The service screen shows a language softkey on F3 labelled with the
     target language.
   - [ ] Pressing F3 toggles EN↔DE and every screen updates immediately.
   - [ ] The toggle works without entering the service PIN and issues no backend
     command.
   - [ ] Reset Total and the network switch remain PIN-gated and unaffected.

   **Blocked by.** ISSUE_0006.

.. issue:: Default German and persistence of the language choice across power cycles
   :id: ISSUE_0008
   :status: ready-for-agent
   :kind: feature
   :links: REQ_0019

   **Parent.** FEAT_0002.

   **What to build.** Persist the operator's language choice across power cycles.
   On boot, load the language from ``/var/lib/baler/language`` (a single line,
   ``de`` or ``en``) with a ``/tmp/baler-language`` fallback, defaulting to German
   when no selection is stored. Save the selection whenever the operator toggles
   it, mirroring ``CounterStore``'s primary/fallback write pattern. After a
   restart the panel returns in the previously selected language.

   **Acceptance criteria.**

   - [ ] A fresh device with no stored selection boots in German.
   - [ ] Toggling the language writes the selection under ``/var/lib/baler/``,
     falling back to ``/tmp`` when that path is not writable.
   - [ ] After a power cycle the panel returns in the previously selected
     language.
   - [ ] Loading tolerates a missing or malformed file by falling back to the
     default.

   **Blocked by.** ISSUE_0007.
