Requirements
============

Requirement needs (``req``) live here. Each ``.. req::`` carries a stable
``REQ_`` ID and refines the feature it belongs to.

.. needtable::
   :types: req
   :columns: id;title;status
   :style: table

.. req:: Two-process architecture
   :id: REQ_0001
   :status: open
   :refines: FEAT_0001

   The system shall run as two processes — a ``baler-daemon`` that owns the
   EtherCAT master, control state machine, counters, and network mode, and a
   ``baler-ui`` Slint front-end — communicating over iceoryx2 shared-memory IPC.

.. req:: UI isolation from output safety
   :id: REQ_0002
   :status: open
   :refines: FEAT_0001

   A crash of ``baler-ui`` shall not affect output safety. The daemon shall
   continue to hold outputs in their commanded or safe state independently of
   the UI, and systemd shall restart the UI automatically.

.. req:: Operator-confirmed wrap pulse
   :id: REQ_0003
   :status: open
   :refines: FEAT_0001

   The daemon shall drive the wrap output (DO1) as a single 5 s pulse only on
   the rising edge of the operator wrap button, and shall never auto-wrap from
   the bale-full signal.

.. req:: Wrap counting on clean completion
   :id: REQ_0004
   :status: open
   :refines: FEAT_0001

   The session and total counters shall each increment by one on clean
   completion of a wrap pulse. A pulse aborted by bus loss or daemon restart
   shall not increment any counter.

.. req:: Independent knife toggle pulse
   :id: REQ_0005
   :status: open
   :refines: FEAT_0001

   The daemon shall drive the knife output (DO2) as a single 5 s pulse on the
   rising edge of the operator knife button, independently of the knife-position
   input and of any wrap pulse; the two outputs may be active simultaneously.

.. req:: Input conditioning and edge detection
   :id: REQ_0006
   :status: open
   :refines: FEAT_0001

   The daemon shall debounce the bale-full and knife-position inputs and detect
   their rising edges. Inputs are status-only; the bale-full indication shall
   clear automatically when its input clears.

.. req:: Counter persistence and reset rules
   :id: REQ_0007
   :status: open
   :refines: FEAT_0001

   Counters shall persist across power loss via an atomic write (temp file plus
   rename) on each change and reload on boot. The session counter shall reset
   only on explicit operator action; the total counter reset shall be available
   only behind the PIN-gated service screen.

.. req:: Idle-only mode switch
   :id: REQ_0008
   :status: open
   :refines: FEAT_0001

   The EtherCAT-to-Ethernet switch shall be permitted only when the machine is
   idle (no output pulse active) and only from the PIN-gated service screen.

.. req:: Ethernet maintenance mode
   :id: REQ_0009
   :status: open
   :refines: FEAT_0001

   On switching to Ethernet, the daemon shall stop the EtherCAT master and bring
   up a configurable static IP, and the UI shall display that IP and indicate
   that control is offline. The system shall not include a built-in updater.

.. req:: Three-layer watchdog
   :id: REQ_0010
   :status: open
   :refines: FEAT_0001

   The system shall use systemd ``Restart=always`` on both services, a hardware
   ``/dev/watchdog`` petted only by a healthy daemon scan loop, and an iceoryx2
   heartbeat for status. The coupler's 50 ms SM watchdog shall serve as the
   output safety backstop.

.. req:: Boot command gating
   :id: REQ_0011
   :status: open
   :refines: FEAT_0001

   On boot the daemon shall reject all operator commands until EtherCAT health
   is established, and the UI shall indicate the initializing state.

.. req:: EtherCAT loss fault handling
   :id: REQ_0012
   :status: open
   :refines: FEAT_0001

   On EtherCAT loss the daemon shall enter a fault state in which outputs are
   dropped by the coupler watchdog, softkeys are locked, inputs are displayed as
   unknown, and any in-flight pulse is aborted and not counted. On recovery it
   shall auto-clear to idle with a transient reconnected notice.

.. req:: Softkey UI and screen set
   :id: REQ_0013
   :status: open
   :refines: FEAT_0001

   The UI shall present a softkey model — F1 Wrap (always available whenever the
   machine is operational; the bale-full state is an advisory hint, not a gate —
   firing a wrap is the operator's responsibility), F2 Toggle Knives, F3 Reset
   Session, F6 Service — across Main, Service (PIN), Ethernet, and Fault-overlay
   screens, with arrows and Enter for dialog navigation.

.. req:: LED state beacon
   :id: REQ_0014
   :status: open
   :refines: FEAT_0001

   The keypad backlight and status LED shall indicate machine state — green
   idle, pulsing amber when full, red on fault, blue in Ethernet mode, white
   flash on an active pulse — with blinking implemented in software.

.. req:: WAGO IO mapping
   :id: REQ_0015
   :status: open
   :refines: FEAT_0001

   IO shall use the WAGO 750-354 coupler with a 750-430 input module
   (DI1 = bale full, DI2 = knife position with 24 V = knives in) and a 750-530
   output module (DO1 = wrap, DO2 = knife switch), with process data at byte
   offset 4, a 10 ms scan, and a 50 ms SM watchdog, built on the taktora
   ethercat-wago-coupler example.

.. req:: Build and deployment
   :id: REQ_0016
   :status: open
   :refines: FEAT_0001

   The binaries shall be cross-compiled with cargo-zigbuild to
   ``aarch64-unknown-linux-musl`` and deployed as two ``Restart=always`` systemd
   units with the daemon ordered first, configured via ``/etc/baler/config.toml``
   with counters stored under ``/var/lib/baler/``.

.. req:: Bale-full attention latch
   :id: REQ_0017
   :status: open
   :refines: FEAT_0001

   When the bale-full input (DI1) becomes true, the UI shall hold the "full"
   indication for at least 20 s so the operator notices it even when looking
   away, regardless of the input clearing sooner. Firing a wrap shall clear the
   latch (the operator has handled it). The latch is an attention aid only; it
   does not gate the wrap softkey (see REQ_0013).

.. req:: Manual IO test mode
   :id: REQ_0018
   :status: open
   :refines: FEAT_0001

   The UI shall provide a PIN-gated IO test screen (reached from Service) that
   lets the operator momentarily energize each output (hold-to-energize DO1 wrap,
   DO2 knife) and observe the live, un-debounced inputs (DI1, DI2). While the
   screen is active the daemon shall enter a manual-IO mode that suspends the
   normal control state machine and drives outputs from the operator's held keys.
   Manual outputs shall only be applied while the bus is operational. A command
   watchdog shall de-energize all outputs and exit manual-IO mode if no manual-IO
   command is received within a short window (≤ 300 ms), so a released key, a UI
   crash, or a lost link always fails safe.

.. req:: UI language toggle (English / German)
   :id: REQ_0019
   :status: open
   :refines: FEAT_0001

   The operator UI shall render all operator-facing labels from a per-language
   string table and provide a PIN-free toggle (Service screen F4) between English
   and German. The selection shall persist across power cycles (stored under
   ``/var/lib/baler/``, ``/tmp`` fallback) and default to German when no valid
   stored choice exists. The toggle softkey shall show the *other* language's
   endonym so the operator knows what it switches to. The IO test screen (REQ_0018),
   a technician aid, may remain English-only.
