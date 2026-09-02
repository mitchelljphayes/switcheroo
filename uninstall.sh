#!/bin/bash
# Uninstaller for Switcheroo. Idempotently handles both pre-migration
# (com.local.switcheroo) and post-migration (com.mitchelljphayes.switcheroo)
# installs, removing only Switcheroo-owned plists and the app bundle while
# preserving config and unrelated hidutil mappings.
#
# FAIL-CLOSED: aborts before ANY bootout or binary removal whenever a
# verified NEW helper is unavailable or reconciliation fails — not only
# for legacy label cases. Never deletes the recovery binary without
# successful reconciliation.
#
# Helper binary candidates (known-current only, no arbitrary/legacy
# installed binary probing):
#   1. Script-adjacent Switcheroo.app (from an extracted release archive)
#   2. Source-tree target/release/switcheroo
# Both are validated: regular file, current-uid ownership, exact
# --capabilities output.
set -euo pipefail

PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/lib.sh
. "${SCRIPT_DIR}/scripts/lib.sh"

REAL_HOME="$(sw_real_home)"
APP_DIR="${REAL_HOME}/.local/bin/Switcheroo.app"
CONFIG_DIR="${REAL_HOME}/.config/switcheroo"
PLIST_DST_DIR="${REAL_HOME}/Library/LaunchAgents"

# Validate HOME.
sw_assert_safe_dir "${REAL_HOME}"
sw_assert_safe_dir "${PLIST_DST_DIR}"

# Locate a verified NEW helper binary — known-current candidates only.
# Do NOT probe arbitrary installed binaries (legacy binaries may not
# support --capabilities and can enter the event tap).
SW_HELPER=""
for candidate in \
  "${SCRIPT_DIR}/Switcheroo.app/Contents/MacOS/switcheroo" \
  "${SCRIPT_DIR}/target/release/switcheroo"
do
  if [ -f "$candidate" ] && sw_helper_has_capabilities "$candidate"; then
    SW_HELPER="$candidate"
    break
  fi
done

# If any Switcheroo installation exists (app dir or plist), a verified NEW
# helper is REQUIRED for reconciliation. Abort before any bootout/removal.
if [ -z "$SW_HELPER" ]; then
  if [ -d "${APP_DIR}" ] || [ -f "${REAL_HOME}/Library/LaunchAgents/com.mitchelljphayes.switcheroo.plist" ] || [ -f "${REAL_HOME}/Library/LaunchAgents/com.local.switcheroo.plist" ]; then
    echo "ERROR: a Switcheroo installation exists, but no verified NEW helper" >&2
    echo "       binary supporting --reconcile-hidutil-state and" >&2
    echo "       --snapshot-legacy-foreign-hidutil is available." >&2
    echo "       Refusing to proceed: stale hidutil mappings or legacy" >&2
    echo "       blanket cleanup could destroy unrelated mappings." >&2
    echo "" >&2
    echo "       To proceed:" >&2
    echo "         1. Run this uninstaller from the current release archive" >&2
    echo "            (extract the archive and run ./uninstall.sh), or" >&2
    echo "         2. Build from source and run ./uninstall.sh from the repo root." >&2
    exit 1
  fi
  # No installation found — nothing to do.
  echo "No Switcheroo installation found."
  exit 0
fi

echo "==> Verified helper: ${SW_HELPER}"

# Reconcile stale hidutil state before any bootout or binary removal.
# FAIL-CLOSED: if reconciliation fails, abort before bootout/removal.
echo "==> Reconciling stale hidutil state..."
if ! env -i HOME="${REAL_HOME}" PATH=/usr/bin:/bin "$SW_HELPER" --reconcile-hidutil-state; then
  echo "ERROR: hidutil state reconciliation failed — refusing to proceed" >&2
  echo "       with bootout or binary removal while stale mappings may persist." >&2
  exit 1
fi

echo "==> Stopping Switcheroo launch agents..."
for label in com.local.switcheroo com.mitchelljphayes.switcheroo; do
  plist="${REAL_HOME}/Library/LaunchAgents/${label}.plist"

  if sw_label_is_loaded "${label}"; then
    if sw_loaded_job_is_switcheroo "${label}"; then
      # Legacy label: snapshot/restore foreign mappings around blanket cleanup.
      if [ "${label}" = "com.local.switcheroo" ]; then
        echo "    Snapshotting foreign hidutil mappings (excluding legacy-owned)..."
        LEGACY_CFG="${CONFIG_DIR}/config.toml"
        FOREIGN_JSON="$(sw_snapshot_legacy_foreign "$SW_HELPER" "${LEGACY_CFG}")"
        sw_bootout_safe "${label}"
        "${SW_BIN_SLEEP}" 1
        echo "    Restoring foreign hidutil mappings after legacy shutdown..."
        sw_hidutil_restore_json "${FOREIGN_JSON}"
      else
        sw_bootout_safe "${label}"
      fi
      sw_reject_symlink "${plist}"
      "${SW_BIN_RM}" -f "${plist}"
    else
      echo "    Skipping ${label}: loaded but not Switcheroo (possible collision)"
    fi
  elif [ -f "${plist}" ] && sw_plist_is_switcheroo "${plist}"; then
    sw_reject_symlink "${plist}"
    "${SW_BIN_RM}" -f "${plist}"
  fi
done
"${SW_BIN_SLEEP}" 1

echo "==> Removing app bundle"
sw_reject_symlink "${APP_DIR}"
if [ -d "${APP_DIR}" ] && [ ! -L "${APP_DIR}" ]; then
  "${SW_BIN_RM}" -rf "${APP_DIR}"
fi

echo ""
echo "Done. Switcheroo-owned hidutil mappings were removed on daemon shutdown"
echo "(legacy foreign mappings restored from snapshot)."
echo "Config preserved at ${CONFIG_DIR}/config.toml"
echo "Remove it manually if you want: ${SW_BIN_RM} -rf ${CONFIG_DIR}"
