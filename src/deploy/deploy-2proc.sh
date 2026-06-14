#!/usr/bin/env bash
# Two-process deploy: baler-daemon (Sim bus/net + real watchdog + iceoryx2) and
# baler-ui (Slint + iceoryx2), talking over iceoryx2. Built with `cross` for the
# gnu target because iceoryx2 0.8 does not cross-compile with zig/musl.
#
# Safe: the daemon runs Sim ports, so it never touches eth0 (your SSH link) and
# needs no coupler. Credentials come from deploy.env (gitignored).
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ROOT="$(cd "$HERE/.." && pwd)"

[ -f "$HERE/deploy.env" ] && . "$HERE/deploy.env"
DEVICE_IP="${DEVICE_IP:?set DEVICE_IP}"
DEVICE_USER="${DEVICE_USER:-root}"
DEVICE_PASS="${DEVICE_PASS:?set DEVICE_PASS}"
TARGET="aarch64-unknown-linux-gnu"
UI_BIN="$SRC_ROOT/target/$TARGET/release/baler-ui"
DAEMON_BIN="$SRC_ROOT/target/$TARGET/release/baler-daemon"

SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
          -o PreferredAuthentications=password -o PubkeyAuthentication=no \
          -o NumberOfPasswordPrompts=1 -o ConnectTimeout=10)
dev_ssh() { sshpass -p "$DEVICE_PASS" ssh "${SSH_OPTS[@]}" "$DEVICE_USER@$DEVICE_IP" "$@"; }
dev_scp() { sshpass -p "$DEVICE_PASS" scp "${SSH_OPTS[@]}" "$1" "$DEVICE_USER@$DEVICE_IP:$2"; }

echo ">> building (cross, $TARGET, release)"
( cd "$SRC_ROOT" && cross build -p baler-daemon --features transport,watchdog-hw --target "$TARGET" --release )
( cd "$SRC_ROOT" && cross build -p baler-ui --features hardware --target "$TARGET" --release )
[ -f "$UI_BIN" ] && [ -f "$DAEMON_BIN" ] || { echo "!! binaries missing"; exit 1; }
ls -lh "$UI_BIN" "$DAEMON_BIN"

echo ">> copying to $DEVICE_IP"
dev_scp "$DAEMON_BIN" "/tmp/baler-daemon.new"
dev_scp "$UI_BIN" "/tmp/baler-ui.new"
dev_scp "$HERE/baler-daemon.service" "/tmp/baler-daemon.service"
dev_scp "$HERE/baler-ui.service" "/tmp/baler-ui.service"

echo ">> installing both services, replacing cr1140-app"
dev_ssh '
set -e
mkdir -p /usr/local/bin /etc/systemd/system /var/lib/baler
mv -f /tmp/baler-daemon.new /usr/local/bin/baler-daemon
mv -f /tmp/baler-ui.new /usr/local/bin/baler-ui
chmod +x /usr/local/bin/baler-daemon /usr/local/bin/baler-ui
mv -f /tmp/baler-daemon.service /etc/systemd/system/baler-daemon.service
mv -f /tmp/baler-ui.service /etc/systemd/system/baler-ui.service
systemctl daemon-reload
systemctl disable --now cr1140-app.service 2>/dev/null || true
systemctl enable baler-daemon.service baler-ui.service
# restart (not just enable --now) so an already-running unit reloads the new binary
systemctl restart baler-daemon.service
systemctl restart baler-ui.service
sleep 2
for u in baler-daemon baler-ui; do
  echo "--- $u ---"
  systemctl --no-pager status "$u.service" | head -n 6 || true
done
'
echo ">> done. two-process baler running on $DEVICE_IP"
