# baler — CR1140 baler controller (Cargo workspace)

Implements `FEAT_0001` (see the sphinx-needs spec under `../spec/`). Two
processes communicating over iceoryx2 via the taktora `transport-iox` connector
(see the [top-level README](../README.md) for the project overview):

| Crate | Role |
|---|---|
| `baler-core` | Pure, host-testable control logic — `input_conditioner`, `pulse`, `state`, `counter` — plus the shared IPC contract (`ipc`: `Command`/`StateSnapshot`/`Mode`), the `postcard` wire `codec`, the `channel` contract, and the transport `bringup` (race-recovery) policy. No hardware deps. |
| `baler-daemon` | The always-on safety authority. `Control` 10 ms cycle wired to the real ports: `ethercat_io` (taktora EtherCAT-WAGO connector), `transport` (iceoryx2 `DaemonLink` to the UI), `network_mode` (`ip`-based NIC switch), `watchdog` (`/dev/watchdog`) — each with a `Sim*`/no-op host fallback. |
| `baler-ui` | Slint front-end for the 800×480 framebuffer + keypad. In-process (`device`) and daemon-coupled (`transport`) backends; EN/DE `i18n`. |

## Status

- **Host-tested:** the pure `baler-core` modules + the daemon `Control` cycle +
  the UI `next_nav`/`i18n` logic, all driven by synthetic ticks. `cargo build` /
  `cargo test` stay green on any host (`Sim*`/no-op ports, no hardware deps).
- **Validated on the CR1140:** EtherCAT bring-up to `Up` on the WAGO coupler, the
  two-process iceoryx2 link (taktora `transport-iox`), Ethernet maintenance mode
  (master suspend), the operator panel, the IO test page, and the EN/DE toggle.
  The `transport`/`ethercat`/`hardware` features pull Linux/aarch64-only deps
  (iceoryx2, taktora, ethercrab, Slint, nix) that **cannot compile on a
  non-Linux host** — they are gated off by default and cross-compiled.

## Develop & test on the host

```sh
cargo test          # baler-core, daemon control, ui (next_nav + i18n) tests
cargo build         # all three crates, sim/no-op ports
```

## Cross-build & deploy (CR1140, aarch64)

| Route | Features | Toolchain | Deploy |
|---|---|---|---|
| Real EtherCAT daemon | `hardware` | `cross` → gnu | `deploy/deploy-ethercat.sh` |
| Two-process sim demo | daemon `transport,watchdog-hw` + ui `hardware` | `cross` → gnu | `deploy/deploy-2proc.sh` |
| Standalone panel | ui `device` | `cargo zigbuild` → musl (static) | `deploy/deploy.sh` |

**iceoryx2 0.8 only cross-compiles via `cross` (Docker) on the gnu target** —
zig/musl fails (bindgen needs clang; `libc_platform` has a cpu_set_t bug), so the
two-process build uses gnu while the standalone (iceoryx2-free) panel can use
musl. `Cross.toml` installs a font for Slint's software-renderer embedder.

The daemon needs `CAP_NET_RAW` + `CAP_NET_ADMIN` (run as root on the device).
Services run as `Restart=always` systemd units with the daemon ordered before
the UI; counters persist under `/var/lib/baler/` (`REQ_0016`).
