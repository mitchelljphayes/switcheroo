#!/bin/bash
# Binary installer for Switcheroo — installs the prebuilt universal .app
# bundle shipped in the release archive. Does NOT run cargo; installs the
# exact packaged binary after verifying its bundle identity, version, and
# ad-hoc signature. Transactional same-filesystem swap with backup/rollback.
set -euo pipefail

PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/lib.sh
. "${SCRIPT_DIR}/scripts/lib.sh"

APP_NAME="Switcheroo.app"
REAL_HOME="$(sw_real_home)"
APP_DIR="${REAL_HOME}/.local/bin/${APP_NAME}"
APP_DIR_PARENT="${REAL_HOME}/.local/bin"
CONFIG_DIR="${REAL_HOME}/.config/switcheroo"
PLIST_NAME="com.mitchelljphayes.switcheroo"
PLIST_SRC="${SCRIPT_DIR}/${PLIST_NAME}.plist"
PLIST_DST="${REAL_HOME}/Library/LaunchAgents/${PLIST_NAME}.plist"
PLIST_DST_DIR="${REAL_HOME}/Library/LaunchAgents"
STAGED_APP_SRC="${SCRIPT_DIR}/${APP_NAME}"
STAGED_BINARY="${STAGED_APP_SRC}/Contents/MacOS/switcheroo"
EXPECTED_BUNDLE_ID="com.mitchelljphayes.switcheroo"
OLD_LABEL="com.local.switcheroo"
OLD_PLIST_DST="${REAL_HOME}/Library/LaunchAgents/${OLD_LABEL}.plist"

# Validate HOME and walk every parent component.
sw_assert_safe_dir "${REAL_HOME}"
sw_ensure_safe_path "${APP_DIR_PARENT}"
sw_ensure_safe_path "${CONFIG_DIR}"
sw_ensure_safe_path "${PLIST_DST_DIR}"

# Validate the packaged bundle before installing it.
[ -d "${STAGED_APP_SRC}" ] || sw_err "packaged ${APP_NAME} not found next to this script"
BUNDLE_ID="$("${SW_USR_DEFAULTS}" read "${STAGED_APP_SRC}/Contents/Info" CFBundleIdentifier 2>/dev/null || printf '')"
[ "${BUNDLE_ID}" = "${EXPECTED_BUNDLE_ID}" ] \
  || sw_err "packaged app bundle id '${BUNDLE_ID}' != expected '${EXPECTED_BUNDLE_ID}' — refusing to install"

BIN_VERSION="$("${STAGED_BINARY}" --version 2>/dev/null | "${SW_USR_HEAD}" -1 || printf '')"
printf '%s' "${BIN_VERSION}" | "${SW_USR_GREP}" -q '^switcheroo [0-9]' \
  || sw_err "packaged switcheroo binary failed --version smoke test (got: '${BIN_VERSION}')"

"${SW_USR_CODESIGN}" --verify --strict "${STAGED_APP_SRC}" 2>/dev/null \
  || sw_err "packaged app fails codesign --verify (signature invalid or absent)"

echo "==> Verified packaged ${APP_NAME} (id=${BUNDLE_ID}, ${BIN_VERSION}, ad-hoc signed)"

# Stage on same filesystem.
echo "==> Staging app bundle (same-filesystem, transactional)"
_SW_STAGING="$("${SW_USR_MKTEMP}" -d -p "${APP_DIR_PARENT}" -t switcheroo.stage.XXXXXX)"
"${SW_BIN_CP}" -R "${STAGED_APP_SRC}" "${_SW_STAGING}/${APP_NAME}"
STAGE_APP="${_SW_STAGING}/${APP_NAME}"
STAGE_BINARY="${STAGE_APP}/Contents/MacOS/switcheroo"

# CRITICAL: capture pre-existing state BEFORE the rollback trap.
sw_capture_state "${APP_DIR}" "${PLIST_DST}" "${PLIST_NAME}" "${OLD_LABEL}"
trap sw_rollback EXIT

# Stop existing agents.
echo "==> Stopping existing Switcheroo agents"
if [ "$_SW_NEW_WAS_RUNNING" = "yes" ]; then
  sw_bootout_safe "${PLIST_NAME}"
fi

# Old label migration: use the NEW staged binary (not the old installed one).
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
  [ -f "${SCRIPT_DIR}/config.toml" ] && "${SW_BIN_CP}" "${SCRIPT_DIR}/config.toml" "${CONFIG_DIR}/config.toml" \
    || printf '# Switcheroo config — see README.md\n' > "${CONFIG_DIR}/config.toml"
  "${SW_BIN_CHMOD}" 600 "${CONFIG_DIR}/config.toml"
  echo "    Created new config"
else
  sw_reject_symlink "${CONFIG_DIR}/config.toml"
  echo "    Config already exists, skipping"
fi

# Transactional app swap.
sw_reject_symlink "${APP_DIR}"
if [ "$_SW_APP_HAD_PRIOR" = "yes" ]; then
  _SW_APP_BACKUP="$("${SW_USR_MKTEMP}" -d -p "${APP_DIR_PARENT}" -t switcheroo.app.bak.XXXXXX)"
  "${SW_BIN_MV}" "${APP_DIR}" "${_SW_APP_BACKUP}/${APP_NAME}"
  _SW_APP_BACKUP="${_SW_APP_BACKUP}/${APP_NAME}"
fi
_SW_PHASE="app_backed_up"
# Failure-injection point: if the staged move below fails, rollback
# restores the prior app from backup (app_backed_up phase).
"${SW_BIN_MV}" "${STAGE_APP}" "${APP_DIR}"
_SW_PHASE="app_swapped"

# Transactional plist install.
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

# Success: commit.
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
echo "Done! Switcheroo is running (${BIN_VERSION})."
echo ""
echo "First install only (or after the bundle-id migration): Grant Accessibility access:"
echo "  System Settings -> Privacy & Security -> Accessibility"
echo "  Add ${APP_DIR}"
echo ""
echo "Commands:"
echo "  Stop:    ${SW_BIN_LAUNCHCTL} bootout gui/$(sw_uid)/${PLIST_NAME}"
echo "  Start:   ${SW_BIN_LAUNCHCTL} bootstrap gui/$(sw_uid) ${PLIST_DST}"
echo "  Logs:    ${SW_USR_TAIL} -f ${REAL_HOME}/Library/Logs/${PLIST_NAME}/daemon.err"
echo "  Config:  ${CONFIG_DIR}/config.toml"
