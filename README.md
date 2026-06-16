# claas-rollant-46 — baler controller for the ifm CR1140

A round-baler controller for the **ifm CR1140** (`ecomat` display PLC: aarch64
Linux, 800×480 framebuffer + function keypad, a single `eth0` NIC). It drives a
**WAGO 750-354 EtherCAT coupler** (+750-430 8×DI, +750-530 8×DO) and presents an
operator panel: wrap/knife pulses, session/total counters, a fault overlay, and
an Ethernet maintenance mode for servicing the single-NIC box.

The project is **spec-driven**: requirements, architecture decisions and issues
live as [sphinx-needs](https://sphinx-needs.readthedocs.io/) objects under
[`spec/`](spec/) and the implementation under [`src/`](src/) traces back to them.

## Architecture

Two processes on the device, communicating over **iceoryx2** shared memory via
the **[taktora](https://github.com/patdhlk/taktora)** `transport-iox` connector
(zero-copy, no central broker):

```
 ┌──────────────────────────┐         ┌────────────────────────┐
 │ baler-ethercat (daemon)  │         │ baler-ui (operator panel) │
 │  taktora executor:       │  iox    │  Slint @ 800×480 fb       │
 │   • EtherCAT WAGO bus    │ ◀─────▶ │  • reads StateSnapshot    │
 │   • 10 ms control cycle  │ state / │  • sends Command          │
 │   • /dev/watchdog        │ command │  • EN/DE, keypad-driven   │
 └──────────────────────────┘         └────────────────────────┘
```

The daemon is the always-on safety authority; the UI is a pure view. The same
`Control::step` runs on both the host **sim** loop and the on-device **EtherCAT**
loop (the taktora executor run on the main thread).

## Repository layout

| Path | What |
|---|---|
| [`spec/`](spec/) | sphinx-needs spec — `features/`, `requirements/`, `architecture/` (decisions), `issues/`, `glossary.rst`. The source of truth. |
| [`src/`](src/) | Rust Cargo workspace (see below). |
| [`src/deploy/`](src/deploy/) | systemd units + `deploy*.sh` scripts for the CR1140. |
| [`Makefile`](Makefile) | spec build / strict gate / needs.json targets. |
| [`CLAUDE.md`](CLAUDE.md) | agent workflow notes (issue lifecycle, the strict gate). |
| `ubproject.toml` | sphinx-needs config + the patdhlk-skills role map. |

### Crates (`src/`)

| Crate | Role |
|---|---|
| `baler-core` | Pure, host-testable core: the mode state machine, 5 s pulse engine, input debounce/edge-detection, persisted counters — plus the shared IPC contract (`Mode`/`Command`/`StateSnapshot`), the `postcard` wire codec, the channel contract, and the transport bring-up (race-recovery) policy. No hardware deps. |
| `baler-daemon` | The safety authority: `Control` 10 ms cycle, the taktora EtherCAT-WAGO connector (`ethercat_io`), the iceoryx2 link to the UI (`transport`), the `ip`-based NIC switch (`network_mode`), and the `/dev/watchdog` petter. |
| `baler-ui` | Slint framebuffer UI + keypad model. Two backends: in-process standalone (`device`) and daemon-coupled over iceoryx2 (`transport`). EN/DE i18n. |

## Build & test (host)

The default build uses `Sim*` ports and no Linux-only deps, so it builds and
tests on any host (incl. macOS):

```sh
cd src
cargo test     # baler-core + daemon control + ui (next_nav, i18n) unit tests
cargo build    # all three crates, sim ports
```

The on-device edges (iceoryx2, taktora, ethercrab, Slint, nix) are gated behind
feature flags and **cannot build on a non-Linux host** — they are off by default
so the host stays green, and are verified by cross-compiling.

### Feature flags

* **baler-daemon:** `transport` (iceoryx2 link), `ethercat` (WAGO bus),
  `watchdog-hw`, `netmode-hw`, and `hardware` = all of them.
* **baler-ui:** `device` (standalone Slint + control logic), `transport`
  (daemon-coupled), `hardware` = `transport`.

## Cross-build & deploy (CR1140 — aarch64)

iceoryx2 0.8 does not cross-compile with musl, so the two-process build targets
`aarch64-unknown-linux-gnu` via [`cross`](https://github.com/cross-rs/cross)
(Docker). The standalone UI can target musl via `cargo zigbuild`. Credentials
and the device IP come from `src/deploy/deploy.env` (gitignored).

```sh
cd src
# Real EtherCAT daemon (enables on boot; needs the coupler on eth0):
deploy/deploy-ethercat.sh
# Two-process sim demo (Sim bus/net — never touches eth0, no coupler needed):
deploy/deploy-2proc.sh
# Standalone UI only (musl, in-process control, zero iceoryx2):
deploy/deploy.sh
```

On a single-NIC box the coupler and SSH share `eth0`: switch the panel to
**Ethernet maintenance mode** (raises the static IP) and move `eth0` back to the
LAN to reconnect. See `src/deploy/deploy-ethercat.sh` for the operating model.

## Spec workflow

Requirements/decisions/issues are sphinx-needs objects. Query them via
`needs.json`, and run the **strict gate** after every spec change:

```sh
make needs     # rebuild spec/_build/needs/needs.json for jq queries
make strict    # uv run sphinx-build -W … — must exit 0
make html      # render the HTML spec
```

Issue `:status:` carries the triage state machine
(`needs-triage → ready-for-agent | ready-for-human → in-progress → done | wontfix`);
git history is the audit trail. See [`CLAUDE.md`](CLAUDE.md) for details.

## License

GPL-3.0-only.
