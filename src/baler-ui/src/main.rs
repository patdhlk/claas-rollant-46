// SPDX-License-Identifier: GPL-3.0-only
//! baler-ui — the Slint front-end for the CR1140 (REQ_0013 softkeys,
//! REQ_0014 LED beacon).
//!
//! Two device builds, both behind `device` (Slint + cr1140-hal; host build stays
//! a stub):
//!
//! * `device` (default device build) — standalone: the real `baler-core` control
//!   logic runs in-process ([`LocalBackend`]); no daemon, no iceoryx2, never
//!   touches eth0. This is what runs on the panel today.
//! * `transport` — same UI, but state comes from baler-daemon over iceoryx2
//!   ([`IpcBackend`]). (iceoryx2 0.8 does not yet cross-compile to musl.)
//!
//! The UI is a pure view; screen switching, PIN entry, and softkey→command
//! mapping are a custom Rust model driven off the keypad — NOT Slint key dispatch.

#[cfg(feature = "device")]
mod platform;

// i18n: pure-Rust string table — compiles on host (for tests) and on device.
#[cfg(any(feature = "device", test))]
pub mod i18n;

// ===========================================================================
// Host stub: no slint, no cr1140-hal. Keeps `cargo build` green on the host.
// ===========================================================================
#[cfg(not(feature = "device"))]
fn main() {
    use baler_ipc::{Command, StateSnapshot};
    eprintln!(
        "baler-ui stub — build with --features device (or hardware) on the CR1140 \
         (aarch64) for the Slint framebuffer UI."
    );
    let _sizes = (
        core::mem::size_of::<Command>(),
        core::mem::size_of::<StateSnapshot>(),
    );
}

// ===========================================================================
// Device UI.
// ===========================================================================
#[cfg(feature = "device")]
slint::include_modules!();

/// Source of [`StateSnapshot`]s and sink for [`baler_ipc::Command`]s. Either the
/// in-process control logic ([`LocalBackend`]) or the daemon over iceoryx2
/// ([`IpcBackend`]).
#[cfg(feature = "device")]
trait Backend {
    /// Advance one frame and return the current snapshot.
    fn step(&mut self) -> baler_ipc::StateSnapshot;
    /// Issue an operator command.
    fn command(&mut self, cmd: baler_ipc::Command);
    /// Demo-only: toggle a simulated "bale full" input. No-op for the daemon path.
    fn sim_toggle_full(&mut self) {}
}

// ---- in-process backend: the real baler-core logic, simulated inputs --------
#[cfg(all(feature = "device", not(feature = "transport")))]
struct LocalBackend {
    state: baler_core::state::BalerState,
    wrap: baler_core::pulse::PulseEngine,
    knife: baler_core::pulse::PulseEngine,
    counters: baler_core::counter::CounterStore,
    sim_full: bool,
    sim_knife_in: bool,
    last_ip: Option<std::net::Ipv4Addr>,
    last: std::time::Instant,
}

#[cfg(all(feature = "device", not(feature = "transport")))]
impl LocalBackend {
    fn new() -> Self {
        let mut state = baler_core::state::BalerState::new();
        state.on_bus_health(true); // simulated bus is always healthy
        Self {
            state,
            wrap: baler_core::pulse::PulseEngine::new(std::time::Duration::from_secs(5)),
            knife: baler_core::pulse::PulseEngine::new(std::time::Duration::from_secs(5)),
            counters: baler_core::counter::CounterStore::load("/var/lib/baler/counters")
                .unwrap_or_else(|_| {
                    // Fall back to a temp path if /var/lib is not writable.
                    baler_core::counter::CounterStore::load("/tmp/baler-counters").unwrap()
                }),
            sim_full: false,
            sim_knife_in: false,
            last_ip: None,
            last: std::time::Instant::now(),
        }
    }

    fn snapshot(&self) -> baler_ipc::StateSnapshot {
        use baler_ipc::KnifePos;
        let counts = self.counters.snapshot();
        baler_ipc::StateSnapshot {
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

#[cfg(all(feature = "device", not(feature = "transport")))]
impl Backend for LocalBackend {
    fn step(&mut self) -> baler_ipc::StateSnapshot {
        use baler_core::pulse::PulseEvent;
        let now = std::time::Instant::now();
        let dt = now.saturating_duration_since(self.last);
        self.last = now;

        self.state.on_bus_health(true);
        self.state.on_inputs(self.sim_full, self.sim_knife_in);

        if let PulseEvent::Completed = self.wrap.tick(dt, true) {
            let _ = self.counters.increment_wrap();
        }
        if let PulseEvent::Completed = self.knife.tick(dt, true) {
            // Simulate the knives physically flipping when the pulse completes.
            self.sim_knife_in = !self.sim_knife_in;
        }
        self.snapshot()
    }

    fn command(&mut self, cmd: baler_ipc::Command) {
        use baler_core::state::Action;
        let any_active = self.wrap.is_active() || self.knife.is_active();
        match self.state.handle(cmd, any_active) {
            Ok(Action::FireWrap) => {
                self.wrap.trigger();
            }
            Ok(Action::FireKnife) => {
                self.knife.trigger();
            }
            Ok(Action::ResetSession) => {
                let _ = self.counters.reset_session();
            }
            Ok(Action::ResetTotal) => {
                let _ = self.counters.reset_total();
            }
            Ok(Action::SwitchToEthernet) => {
                self.last_ip = Some(std::net::Ipv4Addr::new(192, 168, 1, 102))
            }
            Ok(Action::SwitchToEthercat) => self.last_ip = None,
            Err(_) => {}
        }
    }

    fn sim_toggle_full(&mut self) {
        self.sim_full = !self.sim_full;
    }
}

// ---- daemon-coupled backend: state over iceoryx2 ----------------------------
#[cfg(feature = "transport")]
struct IpcBackend {
    poller: baler_ipc::transport::StatePoller,
    sender: baler_ipc::transport::CommandSender,
    last: baler_ipc::StateSnapshot,
    _node: baler_ipc::transport::Node<baler_ipc::transport::Service>,
}

#[cfg(feature = "transport")]
impl IpcBackend {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        use baler_ipc::{KnifePos, Mode};
        let node = baler_ipc::transport::build_node()?;
        let poller = baler_ipc::transport::StatePoller::new(&node)?;
        let sender = baler_ipc::transport::CommandSender::new(&node)?;
        Ok(Self {
            poller,
            sender,
            last: baler_ipc::StateSnapshot {
                mode: Mode::Initializing,
                bale_full: false,
                knife: KnifePos::Unknown,
                wrap_armed: false,
                wrap_active: false,
                knife_active: false,
                session: 0,
                total: 0,
                ip: [0, 0, 0, 0],
                ip_valid: false,
            },
            _node: node,
        })
    }
}

#[cfg(feature = "transport")]
impl Backend for IpcBackend {
    fn step(&mut self) -> baler_ipc::StateSnapshot {
        if let Ok(Some(s)) = self.poller.latest() {
            self.last = s;
        }
        self.last
    }
    fn command(&mut self, cmd: baler_ipc::Command) {
        let _ = self.sender.send(cmd);
    }
}

#[cfg(all(feature = "device", not(feature = "transport")))]
fn make_backend() -> Result<LocalBackend, Box<dyn std::error::Error>> {
    Ok(LocalBackend::new())
}

#[cfg(feature = "transport")]
fn make_backend() -> Result<IpcBackend, Box<dyn std::error::Error>> {
    IpcBackend::new()
}

/// Map a physically-pressed function key to the logical softkey under its
/// on-screen label. The CR1140 keys are arranged F6,F4,F2,F1,F3,F5 left-to-right
/// while the UI labels them F1..F6 left-to-right. Non-function keys pass through.
#[cfg(feature = "device")]
fn remap_fkey(b: cr1140_hal::input::Button) -> cr1140_hal::input::Button {
    use cr1140_hal::input::Button::*;
    match b {
        F1 => F4,
        F2 => F3,
        F3 => F5,
        F4 => F2,
        F5 => F6,
        F6 => F1,
        other => other,
    }
}

#[cfg(feature = "device")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use crate::platform::{FbPlatform, Xrgb8888};
    use baler_ipc::{Command, KnifePos, Mode, StateSnapshot};
    use cr1140_hal::display::FbDisplay;
    use cr1140_hal::input::{Button, ButtonEvent, ButtonReader};
    use slint::platform::set_platform;
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use std::thread::sleep;
    use std::time::Duration;

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Nav {
        Main,
        Service,
        Ethernet,
        Fault,
    }

    const SERVICE_PIN: [u8; 4] = [1, 2, 3, 4];

    struct ServiceState {
        pin: [u8; 4],
        cursor: usize,
        ethernet_selected: bool,
    }
    impl ServiceState {
        fn new() -> Self {
            Self {
                pin: [0; 4],
                cursor: 0,
                ethernet_selected: false,
            }
        }
        fn reset(&mut self) {
            self.pin = [0; 4];
            self.cursor = 0;
        }
        fn unlocked(&self) -> bool {
            self.pin == SERVICE_PIN
        }
        fn display(&self) -> String {
            let mut s = String::new();
            for i in 0..4 {
                if i == self.cursor {
                    s.push(char::from(b'0' + self.pin[i]));
                } else if i < self.cursor {
                    s.push('*');
                } else {
                    s.push('_');
                }
            }
            s
        }
    }

    // ---- open hardware via the HAL ----------------------------------------
    let mut fb = FbDisplay::open_double_buffered("/dev/fb0")?;
    let (w, h) = (fb.width as usize, fb.height as usize);
    let mut reader = ButtonReader::open_keypad_nonblocking()?;

    // ---- Slint on our custom platform -------------------------------------
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    set_platform(Box::new(FbPlatform::new(window.clone())))
        .map_err(|e| format!("set_platform: {e}"))?;
    window.set_size(slint::PhysicalSize::new(fb.width, fb.height));

    let ui = AppWindow::new().map_err(|e| format!("AppWindow::new: {e}"))?;

    let pixel_stride = w;
    let mut buf = vec![Xrgb8888::default(); pixel_stride * h];

    let mut backend = make_backend()?;
    let mut snap: StateSnapshot = backend.step();

    let mut nav = Nav::Main;
    let mut service = ServiceState::new();

    // Language is fixed to the compile-time default (German) in this slice.
    // The operator toggle and persistence come in a later task (ISSUE_0007).
    use crate::i18n::{self, Lang};
    let lang = Lang::default();
    let strings = i18n::table(lang);

    let mode_text = |m: Mode| -> &'static str {
        match m {
            Mode::Initializing => strings.mode_initialising,
            Mode::Operational => strings.mode_operational,
            Mode::Fault => strings.mode_fault,
            Mode::Ethernet => strings.mode_ethernet,
        }
    };
    let knife_text = |k: KnifePos| -> &'static str {
        match k {
            KnifePos::Unknown => strings.knife_unknown,
            KnifePos::In => strings.knife_in,
            KnifePos::Out => strings.knife_out,
        }
    };
    let ip_text = |s: &StateSnapshot| -> String {
        if s.ip_valid {
            format!("{}.{}.{}.{}", s.ip[0], s.ip[1], s.ip[2], s.ip[3])
        } else {
            "—".into()
        }
    };

    // Build the I18n struct once from the string table and set it on the UI.
    // `lang` is constant for this slice so this only needs to run once.
    let tr = I18n {
        title: strings.title.into(),
        bale_full_banner: strings.bale_full_banner.into(),
        session_caption: strings.session_caption.into(),
        total_caption: strings.total_caption.into(),
        knife_caption: strings.knife_caption.into(),
        sk_wrap: strings.sk_wrap.into(),
        sk_wrapping: strings.sk_wrapping.into(),
        sk_knife_toggle: strings.sk_knife_toggle.into(),
        sk_knife_active: strings.sk_knife_active.into(),
        sk_reset_session: strings.sk_reset_session.into(),
        sk_service: strings.sk_service.into(),
        service_title: strings.service_title.into(),
        enter_pin: strings.enter_pin.into(),
        pin_hint: strings.pin_hint.into(),
        network_caption: strings.network_caption.into(),
        sk_reset_total: strings.sk_reset_total.into(),
        sk_use_ethernet: strings.sk_use_ethernet.into(),
        sk_use_ethercat: strings.sk_use_ethercat.into(),
        sk_back: strings.sk_back.into(),
        eth_title: strings.eth_title.into(),
        eth_offline: strings.eth_offline.into(),
        static_ip_caption: strings.static_ip_caption.into(),
        sk_return_ethercat: strings.sk_return_ethercat.into(),
        fault_title: strings.fault_title.into(),
        fault_detail: strings.fault_detail.into(),
    };
    ui.set_tr(tr);

    let push_view = |ui: &AppWindow, nav: Nav, snap: &StateSnapshot, service: &ServiceState| {
        let slint_screen = match nav {
            Nav::Main => Screen::Main,
            Nav::Service => Screen::Service,
            Nav::Ethernet => Screen::Ethernet,
            Nav::Fault => Screen::Fault,
        };
        ui.set_screen(slint_screen);
        ui.set_mode_text(mode_text(snap.mode).into());
        ui.set_bale_full(snap.bale_full);
        ui.set_knife_text(knife_text(snap.knife).into());
        ui.set_wrap_armed(snap.wrap_armed);
        ui.set_wrap_active(snap.wrap_active);
        ui.set_knife_active(snap.knife_active);
        ui.set_session_text(snap.session.to_string().into());
        ui.set_total_text(snap.total.to_string().into());
        ui.set_ethercat_healthy(snap.mode == Mode::Operational);
        ui.set_ip_text(ip_text(snap).into());
        ui.set_pin_display(service.display().into());
        ui.set_ethernet_selected(service.ethernet_selected);
        ui.set_fault_text(strings.fault_link_lost.into());
    };

    push_view(&ui, nav, &snap, &service);

    let mut led = LedBeacon::new();
    let frame_period = Duration::from_millis(16);
    let mut prev_view: Option<(Nav, StateSnapshot, [u8; 4], usize, bool)> = None;

    loop {
        slint::platform::update_timers_and_animations();

        snap = backend.step();

        // Daemon (or local logic) is authority on Fault/Ethernet modes.
        if snap.mode == Mode::Fault {
            nav = Nav::Fault;
        } else if nav == Nav::Fault {
            nav = Nav::Main;
        }
        if snap.mode == Mode::Ethernet && nav != Nav::Service {
            nav = Nav::Ethernet;
        }

        while let Some(ev) = reader.poll_button()? {
            let ButtonEvent::Pressed(btn) = ev else {
                continue;
            };
            // The CR1140 function keys are physically arranged F6,F4,F2,F1,F3,F5
            // left-to-right, but the UI labels softkeys F1..F6 left-to-right.
            // Remap the physical key to the logical softkey under its label so
            // the on-screen F-numbers stay correct. Arrows/Enter pass through.
            let btn = remap_fkey(btn);
            match nav {
                Nav::Main => match btn {
                    Button::F1 if snap.wrap_armed => backend.command(Command::Wrap),
                    Button::F2 => backend.command(Command::ToggleKnife),
                    Button::F3 => backend.command(Command::ResetSession),
                    Button::F4 => backend.sim_toggle_full(), // demo: simulate a full bale
                    Button::F6 => {
                        service.reset();
                        nav = Nav::Service;
                    }
                    _ => {}
                },
                Nav::Service => match btn {
                    Button::Up => {
                        service.pin[service.cursor] = (service.pin[service.cursor] + 1) % 10
                    }
                    Button::Down => {
                        service.pin[service.cursor] = (service.pin[service.cursor] + 9) % 10
                    }
                    Button::Left => {
                        service.cursor = service.cursor.saturating_sub(1);
                    }
                    Button::Right => {
                        if service.cursor < 3 {
                            service.cursor += 1;
                        }
                    }
                    Button::F1 if service.unlocked() => backend.command(Command::ResetTotal),
                    Button::F2 if service.unlocked() => {
                        service.ethernet_selected = !service.ethernet_selected;
                        backend.command(if service.ethernet_selected {
                            Command::EnterEthernet
                        } else {
                            Command::ReturnToEthercat
                        });
                    }
                    Button::F6 => {
                        service.reset();
                        nav = Nav::Main;
                    }
                    _ => {}
                },
                Nav::Ethernet => {
                    if btn == Button::F1 {
                        backend.command(Command::ReturnToEthercat);
                        service.ethernet_selected = false;
                    }
                }
                Nav::Fault => {}
            }
        }

        let view_key = (nav, snap, service.pin, service.cursor, service.ethernet_selected);
        if prev_view.as_ref() != Some(&view_key) {
            push_view(&ui, nav, &snap, &service);
            prev_view = Some(view_key);
        }

        led.tick(&snap);

        let drawn = window.draw_if_needed(|renderer| {
            renderer.render(&mut buf, pixel_stride);
        });
        if drawn {
            let src_bytes =
                unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, buf.len() * 4) };
            fb.surface().copy_from(src_bytes, (pixel_stride * 4) as u32);
            let _ = fb.present();
        }

        sleep(frame_period);
    }
}

// ===========================================================================
// LED state beacon (REQ_0014). Status LED = binary RGB (max 1); keypad
// backlight = 8-bit PWM RGB (max 255). Blinking/pulsing is software-timed.
// ===========================================================================
#[cfg(feature = "device")]
struct LedBeacon {
    start: std::time::Instant,
    last: Option<(u32, u32, u32, u8, u8, u8)>,
}

#[cfg(feature = "device")]
impl LedBeacon {
    fn new() -> Self {
        Self {
            start: std::time::Instant::now(),
            last: None,
        }
    }

    fn tick(&mut self, snap: &baler_ipc::StateSnapshot) {
        use baler_ipc::Mode;
        use cr1140_hal::sys::{set_led_typed, Led};

        let t = self.start.elapsed().as_millis() as u64;
        let phase = (t % 1200) as i64;
        let ramp = if phase < 600 { phase } else { 1200 - phase }; // 0..600
        let amber_pwm = (ramp * 255 / 600) as u8;
        let flash_on = (t / 125) % 2 == 0;

        let (sr, sg, sb, kr, kg, kb): (u32, u32, u32, u8, u8, u8) =
            if snap.wrap_active || snap.knife_active {
                if flash_on {
                    (1, 1, 1, 255, 255, 255)
                } else {
                    (0, 0, 0, 0, 0, 0)
                }
            } else {
                match snap.mode {
                    Mode::Fault => (1, 0, 0, 255, 0, 0),
                    Mode::Ethernet => (0, 0, 1, 0, 0, 255),
                    _ if snap.bale_full => {
                        (1, 1, 0, amber_pwm, (amber_pwm as u32 * 90 / 255) as u8, 0)
                    }
                    Mode::Operational => (0, 1, 0, 0, 200, 0),
                    Mode::Initializing => (0, 0, 1, 0, 0, 120),
                }
            };

        let next = (sr, sg, sb, kr, kg, kb);
        if self.last == Some(next) {
            return;
        }
        self.last = Some(next);

        let _ = set_led_typed(Led::StatusRed, sr);
        let _ = set_led_typed(Led::StatusGreen, sg);
        let _ = set_led_typed(Led::StatusBlue, sb);
        let _ = set_led_typed(Led::KbdRed, kr as u32);
        let _ = set_led_typed(Led::KbdGreen, kg as u32);
        let _ = set_led_typed(Led::KbdBlue, kb as u32);
    }
}
