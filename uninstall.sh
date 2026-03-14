#!/usr/bin/env bash
set -euo pipefail

PLIST_NAME="com.local.rebind"
PLIST_DST="${HOME}/Library/LaunchAgents/${PLIST_NAME}.plist"
APP_DIR="${HOME}/.local/bin/Rebind.app"

echo "==> Stopping rebind..."
launchctl bootout "gui/$(id -u)/${PLIST_NAME}" 2>/dev/null || true

echo "==> Removing LaunchAgent"
rm -f "${PLIST_DST}"

echo "==> Removing app bundle"
rm -rf "${APP_DIR}"

echo ""
echo "Done. Config preserved at ~/.config/rebind/config.toml"
echo "Remove it manually if you want: rm -rf ~/.config/rebind"
