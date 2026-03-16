#!/usr/bin/env bash
set -euo pipefail

PLIST_NAME="com.local.switcheroo"
PLIST_DST="${HOME}/Library/LaunchAgents/${PLIST_NAME}.plist"
APP_DIR="${HOME}/.local/bin/Switcheroo.app"

echo "==> Stopping switcheroo..."
# bootout sends SIGTERM, which triggers the daemon's hidutil cleanup.
# The explicit hidutil clear below is belt-and-suspenders in case the
# daemon was already stopped or didn't clean up (e.g. after SIGKILL).
launchctl bootout "gui/$(id -u)/${PLIST_NAME}" 2>/dev/null || true

echo "==> Clearing hidutil modifier remaps"
hidutil property --set '{"UserKeyMapping":[]}' >/dev/null 2>&1 || true

echo "==> Removing LaunchAgent"
rm -f "${PLIST_DST}"

echo "==> Removing app bundle"
rm -rf "${APP_DIR}"

echo ""
echo "Done. Keyboard modifier remaps have been cleared."
echo "Config preserved at ~/.config/switcheroo/config.toml"
echo "Remove it manually if you want: rm -rf ~/.config/switcheroo"
