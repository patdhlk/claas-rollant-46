# baler — CR1140 baler controller (Cargo workspace)

Implements `FEAT_0001` (see the sphinx-needs spec under `../spec/`). Two
processes communicating over iceoryx2:

| Crate | Role |
|---|---|
| `baler-ipc` | Shared IPC contract (`Command`, `StateSnapshot`, `Mode`). Leaf crate. |
| `baler-core` | Pure, host-testable control logic — `input_conditioner`, `pulse`, `state`, `counter`. Holds the safety-relevant behaviour. |
| `baler-daemon` | The always-on safety authority. Wires `baler-core` to the hardware ports (`EtherCatIo`, `NetworkMode`, `WatchdogPetter` — currently `Sim*` stubs). |
| `baler-ui` | Slint front-end for the 800×480 framebuffer + keypad (scaffold). |

## Status

- **Done & host-tested:** the four pure `baler-core` modules, with unit tests
  (`InputConditioner`, `PulseEngine`, `BalerState`, `CounterStore`). The daemon
  scan loop and the `Sim*` ports build and run on any host.
- **Implemented behind the `hardware` feature, on-device-unverified:** the
  iceoryx2 transport (`baler-ipc::transport`), `EtherCatIo` (taktora
  ethercat-wago connector), `NetworkMode` (`ip`-based NIC switch),
  `WatchdogPetter` (`/dev/watchdog`), and the Slint framebuffer UI. These pull
  in Linux/aarch64-only deps (iceoryx2, taktora, ethercrab, Slint, nix) and
  **cannot compile on a non-Linux host** — they are gated off by default so
  `cargo build` / `cargo test` stay green, and must be compiled + verified on
  the CR1140 (`cargo zigbuild --features hardware`).

## Features

- *(default)* — pure logic + `Sim*` ports; host-buildable and tested.
- `hardware` — the real edges + transport + Slint UI; aarch64/Linux + device only.

## Develop & test on the host

```sh
cargo test          # runs the baler-core unit tests
cargo build         # builds all four crates (Sim* ports, no hardware)
```

## Cross-build for the CR1140 (aarch64, musl)

Following the UpTux ifm-cr1140 workflow:

```sh
cargo zigbuild --target aarch64-unknown-linux-musl --release --features hardware
# scp target/aarch64-unknown-linux-musl/release/{baler-daemon,baler-ui} to the device
```

## Two routes, two toolchains

| Route | Features | Toolchain | Notes |
|---|---|---|---|
| Standalone panel | `device` | `cargo zigbuild` → musl (static) | in-process `baler-core`, no iceoryx2. `deploy/deploy.sh`. |
| Split-process | `transport` / `hardware` | `cross` → gnu (dynamic) | daemon↔UI over iceoryx2. `deploy/deploy-2proc.sh`. |

**iceoryx2 only cross-compiles via `cross` (Docker) on the gnu target** — zig/musl
fails (bindgen needs clang; `libc_platform` has a cpu_set_t bug). The gnu build
needs `#[repr(C)]` on the `ZeroCopySend` enums and `Cross.toml` installs a font
for slint's embedder. Both verified live on the CR1140.

The full EtherCAT path (`--features ethercat`, taktora) is still untested — it
needs the WAGO coupler connected and resolves the taktora-API assumptions the
subagents flagged on first compile.

The daemon needs `CAP_NET_RAW` + `CAP_NET_ADMIN` (setcap on deploy or run as
root). Both run as `Restart=always` systemd units with `baler-daemon` ordered
first; config in `/etc/baler/config.toml`, counters in `/var/lib/baler/`
(`REQ_0016`).
