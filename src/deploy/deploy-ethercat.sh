#!/usr/bin/env bash
# Deploy the EtherCAT/WAGO baler-daemon (ISSUE_0009): the full `--features hardware`
# build that drives the real WAGO 750-354 coupler on eth0 via the taktora executor
# running on the main thread.
#
# OPERATING MODEL. EtherCAT is the device's normal run mode: this installs
# `baler-ethercat` and ENABLES it on boot (and disables the Sim `baler-daemon`, so
# the two don't race for the iceoryx2 state service — they Conflict=). A restart
# then brings the WAGO bus up automatically. To recover SSH for maintenance on a
# single-NIC box, switch to Ethernet mode from the UI (the EnterEthernet softkey
# raises the static IP) and reconnect eth0 to the LAN.
#
# By default this does NOT start the daemon in-session (that would seize eth0 and
# drop SSH); the enable takes effect on the next reboot, so the current session
# and the running Sim daemon stay up. Pass START=1 to start it immediately
# (stops the Sim daemon; SSH drops once the coupler is on eth0).
#
# Logs survive the cable swap via the persistent journal this sets up:
#   journalctl -u baler-ethercat -b --no-pager     (after switching to Ethernet/LAN)
#
# Credentials come from deploy.env (gitignored). Built with `cross` for the gnu
# target because iceoryx2 0.8 does not cross-compile with zig/musl.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ROOT="$(cd "$HERE/.." && pwd)"

[ -f "$HERE/deploy.env" ] && . "$HERE/deploy.env"
DEVICE_IP="${DEVICE_IP:?set DEVICE_IP}"
DEVICE_USER="${DEVICE_USER:-root}"
DEVICE_PASS="${DEVICE_PASS:?set DEVICE_PASS}"
TARGET="aarch64-unknown-linux-gnu"
DAEMON_BIN="$SRC_ROOT/target/$TARGET/release/baler-daemon"
START="${START:-0}"

SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
          -o PreferredAuthentications=password -o PubkeyAuthentication=no \
          -o NumberOfPasswordPrompts=1 -o ConnectTimeout=10)
dev_ssh() { sshpass -p "$DEVICE_PASS" ssh "${SSH_OPTS[@]}" "$DEVICE_USER@$DEVICE_IP" "$@"; }
dev_scp() { sshpass -p "$DEVICE_PASS" scp "${SSH_OPTS[@]}" "$1" "$DEVICE_USER@$DEVICE_IP:$2"; }

echo ">> building EtherCAT daemon (cross, $TARGET, --features hardware, release)"
( cd "$SRC_ROOT" && cross build -p baler-daemon --features hardware --target "$TARGET" --release )
[ -f "$DAEMON_BIN" ] || { echo "!! binary missing: $DAEMON_BIN"; exit 1; }
ls -lh "$DAEMON_BIN"

echo ">> copying to $DEVICE_IP (installs as /usr/local/bin/baler-ethercat, alongside the Sim baler-daemon)"
dev_scp "$DAEMON_BIN" "/tmp/baler-ethercat.new"
dev_scp "$HERE/baler-ethercat.service" "/tmp/baler-ethercat.service"

echo ">> installing unit + persistent journal, enabling on boot (NOT starting in-session)"
dev_ssh '
set -e
mkdir -p /usr/local/bin /etc/systemd/system /var/lib/baler
mv -f /tmp/baler-ethercat.new /usr/local/bin/baler-ethercat
chmod +x /usr/local/bin/baler-ethercat
mv -f /tmp/baler-ethercat.service /etc/systemd/system/baler-ethercat.service
# Persistent journal so the single-NIC bring-up logs survive the cable swap + reboots.
mkdir -p /var/log/journal && systemctl restart systemd-journald || true
systemctl daemon-reload
# EtherCAT is the boot default: enable it, disable the Sim daemon so they do not
# race for the iceoryx2 state service at boot (they Conflict=). The running Sim
# daemon is left up until the next reboot.
systemctl disable baler-daemon.service 2>/dev/null || true
systemctl enable baler-ethercat.service
echo "installed:"; ls -lh /usr/local/bin/baler-ethercat
echo "enablement:"; systemctl is-enabled baler-ethercat.service baler-daemon.service 2>/dev/null || true
'

if [ "$START" = "1" ]; then
  echo ">> START=1 — stopping Sim baler-daemon and starting baler-ethercat now (SSH will DROP once the coupler is on eth0)"
  # Fire-and-forget: the start can sever SSH, so do not wait on a status read.
  dev_ssh 'systemctl stop baler-daemon.service 2>/dev/null || true; systemctl start baler-ethercat.service' || true
  echo ">> start issued."
else
  echo ">> enabled for next boot — device still reachable now, Sim baler-daemon still running until reboot."
fi

cat <<EOF

Operating the EtherCAT daemon (single NIC: SSH and the coupler share eth0):
  1. Connect the WAGO 750-354 coupler to the eth0 cable.
  2. Reboot the device — baler-ethercat auto-starts and brings the bus up.
  3. To read logs / do maintenance: from the UI, switch to Ethernet mode
     (EnterEthernet softkey raises the static IP), reconnect eth0 to the LAN,
     SSH in, then:
       journalctl -u baler-ethercat -b --no-pager | grep -Ei 'health|-> up|timeout|inputs|wkc'
     Expect: 'connector health -> ... Up' and DI->DO mirror activity.
  Fall back to the safe Sim demo:
       systemctl disable --now baler-ethercat && systemctl enable --now baler-daemon
EOF
echo ">> done."
