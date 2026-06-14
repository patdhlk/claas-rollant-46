#!/usr/bin/env bash
# Build the standalone baler-ui for the CR1140, push it, and replace the stock
# cr1140-app demo with our systemd service.
#
# Credentials come from deploy.env (gitignored). Never commit real creds — this
# project is open-source.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_ROOT="$(cd "$HERE/.." && pwd)"

# --- config ---------------------------------------------------------------
[ -f "$HERE/deploy.env" ] && . "$HERE/deploy.env"
DEVICE_IP="${DEVICE_IP:?set DEVICE_IP (see deploy.env.example)}"
DEVICE_USER="${DEVICE_USER:-root}"
DEVICE_PASS="${DEVICE_PASS:?set DEVICE_PASS}"
TARGET="aarch64-unknown-linux-musl"
BIN="$SRC_ROOT/target/$TARGET/release/baler-ui"

SSH_OPTS=(-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null \
          -o PreferredAuthentications=password -o PubkeyAuthentication=no \
          -o NumberOfPasswordPrompts=1 -o ConnectTimeout=10)

dev_ssh()  { sshpass -p "$DEVICE_PASS" ssh  "${SSH_OPTS[@]}" "$DEVICE_USER@$DEVICE_IP" "$@"; }
dev_scp()  { sshpass -p "$DEVICE_PASS" scp  "${SSH_OPTS[@]}" "$1" "$DEVICE_USER@$DEVICE_IP:$2"; }

# --- build ----------------------------------------------------------------
echo ">> building baler-ui ($TARGET, --features device, release)"
( cd "$SRC_ROOT" && cargo zigbuild -p baler-ui --features device --target "$TARGET" --release )
[ -f "$BIN" ] || { echo "!! binary not found: $BIN"; exit 1; }
ls -lh "$BIN"

# --- push -----------------------------------------------------------------
echo ">> copying binary + unit to $DEVICE_IP"
# Stage in /tmp, then move into place (target dir may not exist; mv over a
# running binary is fine on Linux — the old inode is unlinked).
dev_scp "$BIN" "/tmp/baler-ui.new"
dev_scp "$HERE/baler-ui.service" "/tmp/baler-ui.service"

# --- install + swap services ---------------------------------------------
echo ">> installing service, replacing cr1140-app"
dev_ssh '
set -e
mkdir -p /usr/local/bin /etc/systemd/system
mv -f /tmp/baler-ui.new /usr/local/bin/baler-ui
mv -f /tmp/baler-ui.service /etc/systemd/system/baler-ui.service
chmod +x /usr/local/bin/baler-ui
mkdir -p /var/lib/baler
systemctl daemon-reload
systemctl disable --now cr1140-app.service 2>/dev/null || true
systemctl enable --now baler-ui.service
sleep 1
systemctl --no-pager status baler-ui.service | head -n 12 || true
'
echo ">> done. baler-ui is now the operator panel on $DEVICE_IP"
