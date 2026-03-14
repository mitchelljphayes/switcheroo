#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="rebind"
APP_NAME="Rebind.app"
APP_DIR="${HOME}/.local/bin/${APP_NAME}"
CONFIG_DIR="${HOME}/.config/rebind"
PLIST_NAME="com.local.rebind"
PLIST_SRC="$(cd "$(dirname "$0")" && pwd)/${PLIST_NAME}.plist"
PLIST_DST="${HOME}/Library/LaunchAgents/${PLIST_NAME}.plist"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SIGNING_IDENTITY="Rebind Dev"

echo "==> Building rebind (release)..."
cargo build --release --manifest-path "${SCRIPT_DIR}/Cargo.toml"

echo "==> Installing app bundle to ${APP_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources"
cp "${SCRIPT_DIR}/target/release/${BINARY_NAME}" "${APP_DIR}/Contents/MacOS/${BINARY_NAME}"
cp "${SCRIPT_DIR}/bundle/Info.plist" "${APP_DIR}/Contents/Info.plist"
# Generate icon if iconutil is available and iconset exists
if [ -d "${SCRIPT_DIR}/bundle/Rebind.iconset" ]; then
    iconutil -c icns "${SCRIPT_DIR}/bundle/Rebind.iconset" -o "${APP_DIR}/Contents/Resources/AppIcon.icns"
elif [ -f "${APP_DIR}/Contents/Resources/AppIcon.icns" ]; then
    echo "    Using existing icon"
fi

echo "==> Signing app bundle"
if security find-identity -v -p codesigning 2>/dev/null | grep -q "${SIGNING_IDENTITY}"; then
    codesign --force --sign "${SIGNING_IDENTITY}" "${APP_DIR}"
    echo "    Signed with '${SIGNING_IDENTITY}'"
else
    echo "    No '${SIGNING_IDENTITY}' certificate found — using ad-hoc signing."
    echo "    To avoid re-granting Accessibility after each rebuild, create a"
    echo "    self-signed code signing certificate named '${SIGNING_IDENTITY}':"
    echo "      1. Open Keychain Access"
    echo "      2. Menu: Keychain Access → Certificate Assistant → Create a Certificate..."
    echo "      3. Name: ${SIGNING_IDENTITY}"
    echo "      4. Certificate Type: Code Signing"
    echo "      5. Re-run ./install.sh"
    codesign --force --sign - "${APP_DIR}"
    echo "    Signed ad-hoc (Accessibility permission will reset on each rebuild)"
fi

echo "==> Installing config to ${CONFIG_DIR}/config.toml"
mkdir -p "${CONFIG_DIR}"
if [ ! -f "${CONFIG_DIR}/config.toml" ]; then
    cp "${SCRIPT_DIR}/config.toml" "${CONFIG_DIR}/config.toml"
    echo "    Created new config"
else
    echo "    Config already exists, skipping (edit ${CONFIG_DIR}/config.toml)"
fi

echo "==> Installing LaunchAgent"
# Stop existing agent if running
launchctl bootout "gui/$(id -u)/${PLIST_NAME}" 2>/dev/null || true

cp "${PLIST_SRC}" "${PLIST_DST}"

echo "==> Starting rebind"
launchctl bootstrap "gui/$(id -u)" "${PLIST_DST}"

echo ""
echo "Done! Rebind is running."
echo ""
echo "First install only: Grant Accessibility access:"
echo "  System Settings → Privacy & Security → Accessibility"
echo "  Add ${APP_DIR}"
echo "  (Subsequent rebuilds preserve the permission via code signing)"
echo ""
echo "Commands:"
echo "  Stop:    launchctl bootout gui/\$(id -u)/${PLIST_NAME}"
echo "  Start:   launchctl bootstrap gui/\$(id -u) ${PLIST_DST}"
echo "  Logs:    tail -f /tmp/rebind.err"
echo "  Config:  ${CONFIG_DIR}/config.toml"
