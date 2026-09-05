#!/bin/bash
# Source-build installer for Switcheroo.
#
# Builds the daemon from source, assembles + signs the .app bundle in a
# same-filesystem staging directory, transactionally swaps it into place with
# backup + rollback, and installs a LaunchAgent. Handles the v0.1.x identity
# migration from com.local.switcheroo -> com.mitchelljphayes.switcheroo,
# snapshotting and restoring only foreign hidutil mappings around the legacy
# daemon's destructive shutdown using the NEW staged binary.
set -euo pipefail

PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/lib.sh
. "${SCRIPT_DIR}/scripts/lib.sh"

# Parse --cargo /absolute/path
SW_CARGO_BIN=""
while [ $# -gt 0 ]; do
  case "$1" in
    --cargo) SW_CARGO_BIN="$2"; shift 2;;
    *) sw_err "unknown argument: $1";;
  esac
done
if [ -n "$SW_CARGO_BIN" ]; then
  export SW_CARGO_BIN
fi

BINARY_NAME="switcheroo"
APP_NAME="Switcheroo.app"
REAL_HOME="$(sw_real_home)"
APP_DIR="${REAL_HOME}/.local/bin/${APP_NAME}"
APP_DIR_PARENT="${REAL_HOME}/.local/bin"
CONFIG_DIR="${REAL_HOME}/.config/switcheroo"
PLIST_NAME="com.mitchelljphayes.switcheroo"
PLIST_SRC="${SCRIPT_DIR}/${PLIST_NAME}.plist"
PLIST_DST="${REAL_HOME}/Library/LaunchAgents/${PLIST_NAME}.plist"
PLIST_DST_DIR="${REAL_HOME}/Library/LaunchAgents"
SIGNING_IDENTITY="Switcheroo Dev"
OLD_LABEL="com.local.switcheroo"
OLD_PLIST_DST="${REAL_HOME}/Library/LaunchAgents/${OLD_LABEL}.plist"

# Validate HOME and walk every parent component.
sw_assert_safe_dir "${REAL_HOME}"
sw_ensure_safe_path "${APP_DIR_PARENT}"
sw_ensure_safe_path "${CONFIG_DIR}"
sw_ensure_safe_path "${PLIST_DST_DIR}"

# Build from source with --locked for reproducible builds.
CARGO_BIN="$(sw_find_cargo)"
echo "==> Building switcheroo (release) with ${CARGO_BIN}..."
"${CARGO_BIN}" build --release --locked --manifest-path "${SCRIPT_DIR}/Cargo.toml" \
  || sw_err "cargo build failed"

# Stage the new app bundle on the SAME filesystem as the destination.
echo "==> Staging app bundle (same-filesystem, transactional)"
_SW_STAGING="$("${SW_USR_MKTEMP}" -d -p "${APP_DIR_PARENT}" -t switcheroo.stage.XXXXXX)"
STAGE_APP="${_SW_STAGING}/${APP_NAME}"
STAGE_BINARY="${STAGE_APP}/Contents/MacOS/${BINARY_NAME}"
"${SW_BIN_MKDIR}" -p "${STAGE_APP}/Contents/MacOS"
"${SW_BIN_MKDIR}" -p "${STAGE_APP}/Contents/Resources"
"${SW_BIN_CP}" "${SCRIPT_DIR}/target/release/${BINARY_NAME}" "${STAGE_APP}/Contents/MacOS/${BINARY_NAME}"
"${SW_BIN_CP}" "${SCRIPT_DIR}/bundle/Info.plist" "${STAGE_APP}/Contents/Info.plist"
# Stamp Info.plist version from the built binary so source builds always
# report the Cargo version, not the hardcoded committed plist version.
# Fail-closed: require exact "switcheroo X.Y.Z" output matching Cargo.toml.
STAGED_BIN_VERSION_LINE="$("${STAGE_BINARY}" --version 2>/dev/null | "${SW_USR_HEAD}" -1 || true)"
STAGED_BIN_VERSION="$(printf '%s' "${STAGED_BIN_VERSION_LINE}" | "${SW_USR_SED}" 's/^switcheroo //' || true)"
if printf '%s' "${STAGED_BIN_VERSION}" | "${SW_USR_GREP}" -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  # Require Cargo metadata extraction to succeed (fail-closed, no empty bypass)
  CARGO_PKG_VER="$("${CARGO_BIN}" metadata --no-deps --format-version 1 --manifest-path "${SCRIPT_DIR}/Cargo.toml" 2>/dev/null \
    | "${SW_USR_GREP}" -o '"version":"[^"]*"' | "${SW_USR_HEAD}" -1 | "${SW_USR_SED}" 's/"version":"\([^"]*\)"/\1/' || true)"
  if [ -z "${CARGO_PKG_VER}" ]; then
    sw_err "cargo metadata extraction failed — cannot verify binary version matches Cargo.toml"
  fi
  if ! printf '%s' "${CARGO_PKG_VER}" | "${SW_USR_GREP}" -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    sw_err "Cargo.toml version '${CARGO_PKG_VER}' is not strict semver — refusing to install"
  fi
  if [ "${STAGED_BIN_VERSION}" != "${CARGO_PKG_VER}" ]; then
    sw_err "binary version '${STAGED_BIN_VERSION}' != Cargo.toml version '${CARGO_PKG_VER}' — refusing to install with mismatched version"
  fi
  "${SW_USR_PLUTIL}" -replace CFBundleVersion -string "${STAGED_BIN_VERSION}" "${STAGE_APP}/Contents/Info.plist"
  "${SW_USR_PLUTIL}" -replace CFBundleShortVersionString -string "${STAGED_BIN_VERSION}" "${STAGE_APP}/Contents/Info.plist"
else
  sw_err "binary --version output not exact 'switcheroo X.Y.Z': got '${STAGED_BIN_VERSION_LINE}' — refusing to install with unvalidated version"
fi
"${SW_USR_PLUTIL}" -lint "${STAGE_APP}/Contents/Info.plist" >/dev/null
if [ -d "${SCRIPT_DIR}/bundle/Switcheroo.iconset" ]; then
  "${SW_USR_ICONUTIL}" -c icns "${SCRIPT_DIR}/bundle/Switcheroo.iconset" \
    -o "${STAGE_APP}/Contents/Resources/AppIcon.icns" || sw_err "iconutil failed"
elif [ -f "${APP_DIR}/Contents/Resources/AppIcon.icns" ]; then
  "${SW_BIN_CP}" "${APP_DIR}/Contents/Resources/AppIcon.icns" "${STAGE_APP}/Contents/Resources/AppIcon.icns"
fi

echo "==> Signing staged app bundle"
if "${SW_USR_SECURITY}" find-identity -v -p codesigning 2>/dev/null | "${SW_USR_GREP}" -q "${SIGNING_IDENTITY}"; then
  "${SW_USR_CODESIGN}" --force --sign "${SIGNING_IDENTITY}" "${STAGE_APP}" || sw_err "codesign failed"
  echo "    Signed with '${SIGNING_IDENTITY}'"
else
  echo "    No '${SIGNING_IDENTITY}' certificate found — using ad-hoc signing."
  "${SW_USR_CODESIGN}" --force --sign - "${STAGE_APP}" || sw_err "ad-hoc codesign failed"
fi

# CRITICAL: capture pre-existing state BEFORE installing the rollback trap.
# Early failure (e.g. migration snapshot failure) must never delete a
# pre-existing installation.
sw_capture_state "${APP_DIR}" "${PLIST_DST}" "${PLIST_NAME}" "${OLD_LABEL}"
trap sw_rollback EXIT

# Stop existing agents BEFORE overwriting the live bundle.
echo "==> Stopping existing Switcheroo agents"
if [ "$_SW_NEW_WAS_RUNNING" = "yes" ]; then
  sw_bootout_safe "${PLIST_NAME}"
fi

# Old label migration: use the NEW staged binary (not the old installed one)
# for the foreign-only snapshot. The old binary doesn't support the flag.
if [ "$_SW_OLD_WAS_RUNNING" = "yes" ]; then
  echo "==> Migrating from old label ${OLD_LABEL} (verified live Switcheroo job)..."
  echo "    Snapshotting foreign hidutil mappings (using NEW staged binary)..."
  LEGACY_CFG="${CONFIG_DIR}/config.toml"
  FOREIGN_JSON="$(sw_snapshot_legacy_foreign "${STAGE_BINARY}" "${LEGACY_CFG}")"
  sw_bootout_safe "${OLD_LABEL}"
  "${SW_BIN_SLEEP}" 1
  echo "    Restoring foreign hidutil mappings after legacy shutdown..."
  sw_hidutil_restore_json "${FOREIGN_JSON}"
elif [ -f "${OLD_PLIST_DST}" ] && sw_plist_is_switcheroo "${OLD_PLIST_DST}"; then
  echo "    Note: ${OLD_LABEL} plist exists (Switcheroo) but job is not loaded — will remove plist"
fi
"${SW_BIN_SLEEP}" 1

# Config (preserve existing).
echo "==> Installing config to ${CONFIG_DIR}/config.toml"
if [ ! -f "${CONFIG_DIR}/config.toml" ]; then
  sw_reject_symlink "${CONFIG_DIR}/config.toml"
  "${SW_BIN_CP}" "${SCRIPT_DIR}/config.toml" "${CONFIG_DIR}/config.toml"
  "${SW_BIN_CHMOD}" 600 "${CONFIG_DIR}/config.toml"
  echo "    Created new config"
else
  sw_reject_symlink "${CONFIG_DIR}/config.toml"
  echo "    Config already exists, skipping (edit ${CONFIG_DIR}/config.toml)"
fi

# Transactional app swap: back up old, move new into place.
sw_reject_symlink "${APP_DIR}"
if [ "$_SW_APP_HAD_PRIOR" = "yes" ]; then
  _SW_APP_BACKUP="$("${SW_USR_MKTEMP}" -d -p "${APP_DIR_PARENT}" -t switcheroo.app.bak.XXXXXX)"
  "${SW_BIN_MV}" "${APP_DIR}" "${_SW_APP_BACKUP}/${APP_NAME}"
  _SW_APP_BACKUP="${_SW_APP_BACKUP}/${APP_NAME}"
fi
_SW_PHASE="app_backed_up"
# Failure-injection point: if the staged move below fails, the app_backed_up
# phase ensures rollback restores the prior app from backup.
"${SW_BIN_MV}" "${STAGE_APP}" "${APP_DIR}"
_SW_PHASE="app_swapped"

# Transactional plist install: back up old, move new into place.
echo "==> Installing LaunchAgent"
TMP_PLIST="$(sw_render_plist "${PLIST_SRC}" "${APP_DIR}" "${PLIST_DST_DIR}")"
if [ "$_SW_PLIST_HAD_PRIOR" = "yes" ]; then
  _SW_PLIST_BACKUP="$("${SW_USR_MKTEMP}" -p "${PLIST_DST_DIR}" -t switcheroo.plist.bak.XXXXXX)"
  "${SW_BIN_CP}" "${PLIST_DST}" "${_SW_PLIST_BACKUP}"
fi
"${SW_BIN_MV}" "${TMP_PLIST}" "${PLIST_DST}"
"${SW_BIN_CHMOD}" 644 "${PLIST_DST}"
_SW_PHASE="plist_swapped"

# Bootstrap + verify.
echo "==> Starting switcheroo"
if ! "${SW_BIN_LAUNCHCTL}" bootstrap "gui/$(sw_uid)" "${PLIST_DST}"; then
  sw_err "launchctl bootstrap failed"
fi
if ! sw_loaded_job_is_switcheroo "${PLIST_NAME}"; then
  sw_err "post-bootstrap verification failed: registered job does not point at ${APP_DIR}"
fi
_SW_BOOTSTRAPPED_BY_US="yes"
_SW_PHASE="bootstrapped"

# Success: commit the transaction, clean up backups.
if [ -f "${OLD_PLIST_DST}" ] && sw_plist_is_switcheroo "${OLD_PLIST_DST}"; then
  "${SW_BIN_RM}" -f "${OLD_PLIST_DST}"
fi
[ -n "${_SW_APP_BACKUP:-}" ] && [ -d "${_SW_APP_BACKUP:-}" ] && "${SW_BIN_RM}" -rf "${_SW_APP_BACKUP}" 2>/dev/null || true
[ -n "${_SW_PLIST_BACKUP:-}" ] && [ -f "${_SW_PLIST_BACKUP:-}" ] && "${SW_BIN_RM}" -f "${_SW_PLIST_BACKUP}" 2>/dev/null || true
[ -n "${_SW_STAGING:-}" ] && "${SW_BIN_RM}" -rf "${_SW_STAGING}" 2>/dev/null || true
_SW_APP_BACKUP=""
_SW_PLIST_BACKUP=""
_SW_STAGING=""
_SW_PHASE="done"
trap - EXIT

echo ""
echo "Done! Switcheroo is running."
echo ""
echo "First install only (or after the bundle-id migration): Grant Accessibility access:"
echo "  System Settings -> Privacy & Security -> Accessibility"
echo "  Add ${APP_DIR}"
echo "  (Subsequent rebuilds preserve the permission via code signing)"
echo "  Note: the v0.1.x migration changed the bundle id from com.local.switcheroo"
echo "  to com.mitchelljphayes.switcheroo, so an existing Accessibility grant must"
echo "  be re-issued once after upgrade."
echo ""
echo "Commands:"
echo "  Stop:    ${SW_BIN_LAUNCHCTL} bootout gui/$(sw_uid)/${PLIST_NAME}"
echo "  Start:   ${SW_BIN_LAUNCHCTL} bootstrap gui/$(sw_uid) ${PLIST_DST}"
echo "  Logs:    ${SW_USR_TAIL} -f ${REAL_HOME}/Library/Logs/${PLIST_NAME}/daemon.err"
echo "  Config:  ${CONFIG_DIR}/config.toml"
