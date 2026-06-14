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
   - **Counting**: session + total increment once on clean pulse completion;
     interrupted pulses do not count.
   - **Knife toggle**: fire-and-forget 5 s pulse on the button rising edge,
     independent of knife input and of the wrap pulse (outputs may overlap).
   - **Inputs** are debounced, edge-detected, status-only; bale-full clears with
     its input.
   - **Counters** persist via temp-file + ``rename`` on each change, reload on
     boot. Session resets on operator action only; total reset is PIN-gated.
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
     F6 Service), four screens (Main / Service-PIN / Ethernet / Fault overlay),
     LED beacon (green idle, amber-pulse full, red fault, blue Ethernet, white
     flash on pulse), blinking done in software.
   - **IO**: WAGO 750-354 / 750-430 / 750-530; DI1 = bale full, DI2 = knife
     position (24 V = in); DO1 = wrap, DO2 = knife switch; data at process-image
     byte offset 4; 10 ms scan, 50 ms watchdog; built on the taktora
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

.. feat:: Operator UI language selection (English / German)
   :id: FEAT_0002
   :status: open

   **Problem Statement.**
   The ``baler-ui`` ships its on-screen text in English only. German-speaking
   operators and technicians working the CR1140 panel cannot read the machine
   state, softkey labels, the service screen, or the fault overlay in their own
   language, which slows operation and makes faults harder to act on under
   pressure.

   **Solution.**
   ``baler-ui`` gains a second display language, German, alongside English.
   Every operator-facing string — the main screen, service screen, Ethernet
   screen, the fault overlay, and the Rust-driven mode/knife/fault texts — is
   available in both languages from a single source of truth. The operator
   switches language from the service screen with a dedicated softkey that needs
   no service PIN, so changing language is always one screen away. The choice
   defaults to German on a fresh device and persists across power cycles, so the
   panel comes back up in the chosen language after a field restart.

   **User Stories.**

   1. As a German-speaking operator, I want the machine state shown in German, so
      that I can read it at a glance.
   2. As a German-speaking operator, I want the softkey labels in German, so that
      I know what each button does.
   3. As a German-speaking operator, I want the bale-full banner in German, so
      that I recognise a wrap is needed without translating in my head.
   4. As a German-speaking technician, I want the service screen in German, so
      that maintenance actions are unambiguous.
   5. As a German-speaking operator, I want the fault overlay in German, so that I
      understand why controls are locked and what happens next.
   6. As an operator, I want to switch the display language from the service
      screen, so that I can change it without special tools.
   7. As an operator, I want to switch language without entering the service PIN,
      so that a harmless display setting is never gated behind a code.
   8. As an operator, I want the language softkey to show the language it switches
      to, so that I know what pressing it will do.
   9. As an English-speaking operator, I want to switch back to English the same
      way, so that the toggle works in both directions.
   10. As an operator, I want my language choice to survive a power cycle, so that
       a field restart does not revert the panel to a language I cannot read.
   11. As an owner commissioning a German-market machine, I want German to be the
       default out of the box, so that the panel is usable before anyone touches a
       setting.
   12. As a German-speaking operator, I want umlauts and ß to render correctly, so
       that the German text is legible rather than blank boxes.
   13. As a maintainer, I want all UI text in one place per language, so that a new
       string cannot be added in one language and forgotten in the other.

   **Implementation Decisions.**

   - **Rust string table, view stays pure.** A new host-testable ``i18n`` module
     in ``baler-ui`` owns a ``Lang { En, De }`` enum and a ``Strings`` table of
     ``&'static str`` (``EN`` / ``DE`` constants) — the single source of truth for
     all static UI text. This keeps the project's "pure view, all logic in Rust"
     design (the ``.slint`` file already takes mode/knife/fault text as inputs).
   - **One Slint struct, not many properties.** ``baler.slint`` declares a single
     ``I18n`` struct and one ``in property <I18n> tr``; every hard-coded literal
     becomes ``root.tr.<field>``, populated from ``Strings`` in ``push_view``.
   - **State-derived text is language-aware in Rust.** ``mode_text``,
     ``knife_text``, and the fault string are computed from the snapshot *and* the
     active language and set on the existing string properties.
   - **PIN-free toggle on the service screen, F3.** The service screen's first
     free softkey (F3) toggles language; it is enabled regardless of PIN unlock,
     unlike Reset Total (F1) and the network switch (F2). Its label shows the
     *target* language (``DEUTSCH`` when in English, ``ENGLISH`` when in German).
     No backend ``Command`` is involved — language is a UI-only concern, identical
     for the local and daemon-coupled backends.
   - **Default German, persisted like counters.** Language is read on boot from
     ``/var/lib/baler/language`` (single line ``de`` / ``en``) with a
     ``/tmp/baler-language`` fallback, defaulting to German when absent, and
     written on each toggle — mirroring ``CounterStore``'s primary/fallback
     pattern.
   - **Glyph embedding.** The ``EmbedForSoftwareRenderer`` build only embeds
     glyphs found in ``.slint`` literals, so the German strings (living in Rust)
     would otherwise miss ``Ä Ö Ü ä ö ü ß``. A hidden glyph-anchor ``Text`` in
     ``AppWindow`` carries those characters so the compiler embeds them; verified
     on device.
   - **Agreed German wording** (subject to review): BETRIEB / STÖRUNG /
     INITIALISIERUNG (mode); SCHICHT / GESAMT / MESSER (counters, knife);
     INNEN / AUSSEN / UNBEKANNT (knife position); WICKELN / MESSER SCHALTEN /
     SCHICHT NULLEN / WARTUNG (main softkeys); GESAMT NULLEN / ETHERNET NUTZEN /
     ETHERCAT NUTZEN / ZURÜCK (service); ETHERNET-MODUS / STEUERUNG OFFLINE —
     ETHERCAT GESTOPPT / STATISCHE IP / ZURÜCK ZU ETHERCAT (Ethernet);
     ⚠ STÖRUNG / ETHERCAT-VERBINDUNG VERLOREN (fault). The protocol names
     ETHERNET and ETHERCAT stay untranslated.

   **Testing Decisions.**

   - Good tests exercise external behaviour, not internals. The ``i18n`` string
     table is pure Rust and tested on the host with no Slint or hardware.
   - **Unit tests** (``i18n``): every ``Strings`` field is non-empty in both
     ``EN`` and ``DE`` (catches a missed translation), and ``Lang`` language-code
     round-trips with unknown codes falling back to the default. Prior art:
     ``CounterStore``'s temp-dir-based unit tests.
   - Language persistence (load/save) and the language-aware mode/knife/fault
     mappers are covered by on-device checks rather than dedicated unit tests, as
     is the rest of ``baler-ui``.

   **Out of Scope.**

   - Any third language, or runtime-loadable / file-based translation catalogs —
     the two languages are compiled in.
   - Per-string localisation of numbers, dates, or units (counts stay numeric).
   - Translating ``baler-daemon`` logs, ``config.toml`` keys, or SSH-side tooling.
   - A language setting in ``config.toml`` — the choice lives in its own persisted
     file and is changed only from the panel.

   **Further Notes.**

   On-device verification must confirm that German umlauts render (glyph anchor
   embedded correctly) and that the longer German labels fit the fixed-width
   softkeys (126 px, word-wrap) and the mode/fault banners without clipping.
