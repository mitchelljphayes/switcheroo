#!/usr/bin/env bash
set -euo pipefail

PLIST_NAME="com.local.keytap"
PLIST_DST="${HOME}/Library/LaunchAgents/${PLIST_NAME}.plist"

echo "==> Stopping keytap..."
launchctl bootout "gui/$(id -u)/${PLIST_NAME}" 2>/dev/null || true

echo "==> Removing LaunchAgent"
rm -f "${PLIST_DST}"

echo "==> Removing binary"
sudo rm -f /usr/local/bin/keytap

echo ""
echo "Done. Config preserved at ~/.config/keytap/config.toml"
echo "Remove it manually if you want: rm -rf ~/.config/keytap"
