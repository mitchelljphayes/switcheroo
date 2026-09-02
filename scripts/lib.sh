# shellcheck shell=bash
# Shared helpers for Switcheroo install/uninstall scripts.
#
# Sourced by install.sh, install-binary.sh, uninstall.sh. Provides hardened,
# shell-testable helpers for: safe directory/path validation, cargo discovery,
# launchctl bootout classification, job identity validation, hidutil
# snapshot/restore around the legacy destructive daemon, transactional
# app/plist swap with rollback, and plist rendering.
#
# shellcheck disable=SC2034
#
# Security review:
#   - All system tools invoked by absolute path with a fixed minimal PATH.
#   - stat uses numeric permission bits (%Lp), rejecting group/world-writable.
#   - sed is /usr/bin/sed (macOS has no /bin/sed).
#   - cargo is discovered safely (validated fixed candidates or --cargo flag).
#   - HOME is resolved from trusted OS account metadata (dscl), not $HOME.
#   - launchctl print parser reads the real `program =` line. Loaded jobs
#     require live identity; plist fallback never authorizes bootout.
#   - Migration uses the NEW helper binary (staged or packaged), never the
#     old installed binary which doesn't support the new one-shot flags.
#   - Pre-existing app/plist state is captured BEFORE the rollback trap so
#     early failure never deletes a pre-existing installation.

# Fixed minimal PATH for system tools. Exported by the caller before sourcing.
SW_BIN_LAUNCHCTL=/bin/launchctl
SW_BIN_MKDIR=/bin/mkdir
SW_BIN_CP=/bin/cp
SW_BIN_MV=/bin/mv
SW_BIN_RM=/bin/rm
SW_BIN_CHMOD=/bin/chmod
SW_BIN_SLEEP=/bin/sleep
SW_BIN_CAT=/bin/cat
SW_USR_STAT=/usr/bin/stat
SW_USR_GREP=/usr/bin/grep
SW_USR_PLUTIL=/usr/bin/plutil
SW_USR_SED=/usr/bin/sed
SW_USR_CODESIGN=/usr/bin/codesign
SW_USR_SECURITY=/usr/bin/security
SW_USR_ICONUTIL=/usr/bin/iconutil
SW_USR_MKTEMP=/usr/bin/mktemp
SW_USR_ID=/usr/bin/id
SW_USR_HIDUTIL=/usr/bin/hidutil
SW_USR_TAIL=/usr/bin/tail
SW_USR_HEAD=/usr/bin/head
SW_USR_DEFAULTS=/usr/bin/defaults
SW_USR_FILE=/usr/bin/file
SW_USR_LIPO=/usr/bin/lipo
SW_USR_DSCL=/usr/bin/dscl

sw_err() { printf 'Error: %s\n' "$*" >&2; exit 1; }

sw_uid() { "${SW_USR_ID}" -u; }

sw_real_home() {
  local home
  home="$("${SW_USR_DSCL}" . -read "/Users/$(/usr/bin/id -un)" NFSHomeDirectory 2>/dev/null | "${SW_USR_SED}" 's/^NFSHomeDirectory: //')"
  if [ -n "$home" ] && [ -d "$home" ]; then
    printf '%s\n' "$home"
    return 0
  fi
  sw_err "could not resolve account home from DirectoryService — refusing to use \$HOME"
}

sw_is_symlink() { [ -L "$1" ]; }

sw_mode_numeric() { "${SW_USR_STAT}" -f '%Lp' "$1" 2>/dev/null; }

sw_owner_of() { "${SW_USR_STAT}" -f '%u' "$1" 2>/dev/null; }

sw_reject_group_world_writable() {
  local path="$1" mode perm
  mode="$(sw_mode_numeric "$path")" || sw_err "$path: stat mode failed"
  perm="${mode#"${mode%???}"}"
  if [ $(( 8#$perm & 0022 )) -ne 0 ]; then
    sw_err "$path is group/world-writable (mode $perm) — refusing"
  fi
}

sw_assert_safe_dir() {
  local path="$1" owner
  [ -n "$path" ] || sw_err "sw_assert_safe_dir: empty path"
  if sw_is_symlink "$path"; then sw_err "$path is a symlink — refusing (possible attack)"; fi
  [ -d "$path" ] || sw_err "$path is not a directory (or does not exist)"
  owner="$(sw_owner_of "$path")" || sw_err "$path: stat failed"
  [ "$owner" = "$(sw_uid)" ] || sw_err "$path owned by uid $owner (current $(sw_uid)) — refusing"
  sw_reject_group_world_writable "$path"
}

sw_ensure_safe_path() {
  local path="$1" comp accum="" home_prefix
  [ -n "$path" ] || sw_err "sw_ensure_safe_path: empty path"
  case "$path" in /*) ;; *) sw_err "sw_ensure_safe_path: $path is not absolute";; esac
  home_prefix="$(sw_real_home)"
  IFS='/' read -ra parts <<< "$path"
  for comp in "${parts[@]}"; do
    [ -n "$comp" ] || continue
    if [ -z "$accum" ]; then accum="/${comp}"; else accum="${accum}/${comp}"; fi
    if [ ! -e "$accum" ]; then
      "${SW_BIN_MKDIR}" "$accum" || sw_err "mkdir $accum failed"
      "${SW_BIN_CHMOD}" 700 "$accum" || sw_err "chmod $accum failed"
    fi
    if sw_is_symlink "$accum"; then sw_err "$accum is a symlink — refusing (possible attack)"; fi
    [ -d "$accum" ] || sw_err "$accum is not a directory"
    sw_reject_group_world_writable "$accum"
    case "$accum" in
      "${home_prefix}"|"${home_prefix}"/*)
        local owner
        owner="$(sw_owner_of "$accum")" || sw_err "$accum: stat owner failed"
        [ "$owner" = "$(sw_uid)" ] || sw_err "$accum owned by uid $owner (current $(sw_uid)) — refusing"
        ;;
    esac
  done
}

sw_reject_symlink() {
  local path="$1"
  if [ -L "$path" ]; then sw_err "$path is a symlink — refusing to overwrite (possible attack)"; fi
}

# Discover the cargo binary for source builds. Ignores $CARGO and PATH.
# Checks: --cargo flag (SW_CARGO_BIN from install.sh arg parsing), then fixed
# candidates. Validates ownership and runs `cargo --version` to confirm.
sw_find_cargo() {
  local c real_owner version
  if [ -n "${SW_CARGO_BIN:-}" ] && [ -x "$SW_CARGO_BIN" ]; then
    c="$SW_CARGO_BIN"
  else
    for c in "$(sw_real_home)/.cargo/bin/cargo" /opt/homebrew/bin/cargo /usr/local/bin/cargo; do
      [ -x "$c" ] && break
      c=""
    done
    [ -n "$c" ] || sw_err "cargo not found. Pass --cargo /absolute/path to install.sh."
  fi
  real_owner="$("${SW_USR_STAT}" -L -f '%u' "$c" 2>/dev/null || printf 'err')"
  [ "$real_owner" = "$(sw_uid)" ] \
    || sw_err "cargo at $c is owned by uid $real_owner (current $(sw_uid)) — refusing to execute"
  version="$("$c" --version 2>/dev/null)" || sw_err "cargo at $c failed --version check"
  printf '%s' "$version" | "${SW_USR_GREP}" -q '^cargo ' \
    || sw_err "cargo at $c does not report as cargo (got: '$version')"
  printf '%s\n' "$c"
}

sw_bootout_safe() {
  local label="$1" rc stderr_out
  stderr_out="$("${SW_BIN_LAUNCHCTL}" bootout "gui/$(sw_uid)/${label}" 2>&1 1>/dev/null)" && rc=0 || rc=$?
  if [ "$rc" -eq 0 ]; then return 0; fi
  if printf '%s' "$stderr_out" | "${SW_USR_GREP}" -Eqi 'Could not find|No such process|Input/output error'; then
    return 0
  fi
  sw_err "launchctl bootout $label failed (rc=$rc): $stderr_out"
}

sw_plist_program_arg0() {
  local plist="$1"
  [ -f "$plist" ] || { printf ''; return; }
  "${SW_USR_PLUTIL}" -extract 'ProgramArguments.0' raw -o - "$plist" 2>/dev/null || printf ''
}

SW_EXPECTED_EXEC_SUFFIX=".local/bin/Switcheroo.app/Contents/MacOS/switcheroo"

sw_expected_exec() {
  printf '%s/%s\n' "$(sw_real_home)" "${SW_EXPECTED_EXEC_SUFFIX}"
}

sw_plist_is_switcheroo() {
  local plist="$1" prog expected
  prog="$(sw_plist_program_arg0 "$plist")"
  expected="$(sw_expected_exec)"
  [ "$prog" = "$expected" ]
}

sw_parse_launchctl_program() {
  local out="$1"
  printf '%s\n' "$out" | "${SW_USR_SED}" -n 's/^[[:space:]]*program = \(.*\)/\1/p' | "${SW_USR_HEAD}" -1
}

sw_loaded_job_is_switcheroo() {
  local label="$1" out prog expected
  if ! out="$("${SW_BIN_LAUNCHCTL}" print "gui/$(sw_uid)/${label}" 2>/dev/null)"; then
    return 1
  fi
  prog="$(sw_parse_launchctl_program "$out")"
  expected="$(sw_expected_exec)"
  [ -n "$prog" ] && [ "$prog" = "$expected" ]
}

sw_label_is_loaded() {
  local label="$1"
  "${SW_BIN_LAUNCHCTL}" print "gui/$(sw_uid)/${label}" >/dev/null 2>&1
}

# ── hidutil snapshot / restore ───────────────────────────────────────
# The legacy com.local.switcheroo daemon does blanket UserKeyMapping:[]
# cleanup on shutdown. Before stopping it, we use the NEW helper binary
# (which supports --snapshot-legacy-foreign-hidutil) to snapshot current
# mappings, filter out the legacy daemon's configured-owned pairs, and
# restore only foreign mappings after bootout. The NEW binary is the
# staged/packaged binary — NEVER the old installed one, which doesn't
# support the flag. Fail-closed on any error.

# Capability probe for a KNOWN-CURRENT helper binary. Does NOT use `timeout`
# (not on stock macOS). Only called on known-current candidates:
#   - script-adjacent Switcheroo.app/Contents/MacOS/switcheroo (release archive)
#   - source-tree target/release/switcheroo
# Validates: path exists, owned by current uid, is a regular file (not
# symlink/dir), then runs `--capabilities` directly (the new binary exits
# in <50ms without event tap; old binaries treat it as a config path and
# fail on file-not-found). Checks output for required capabilities.
# $1 = binary path. Returns 0 if the binary supports both
# --snapshot-legacy-foreign-hidutil and --reconcile-hidutil-state.
sw_helper_has_capabilities() {
  local binary="$1" caps owner
  [ -n "$binary" ] || return 1
  [ -f "$binary" ] || return 1
  [ ! -L "$binary" ] || return 1
  owner="$("${SW_USR_STAT}" -L -f '%u' "$binary" 2>/dev/null || printf 'err')"
  [ "$owner" = "$(sw_uid)" ] || return 1
  # Direct --capabilities: new binary exits 0 immediately. Old binary
  # treats it as a config path, fails to load, exits non-zero.
  caps="$(env -i HOME="$(sw_real_home)" PATH=/usr/bin:/bin "$binary" --capabilities 2>/dev/null)" || return 1
  printf '%s' "$caps" | "${SW_USR_GREP}" -q -- '--snapshot-legacy-foreign-hidutil' || return 1
  printf '%s' "$caps" | "${SW_USR_GREP}" -q -- '--reconcile-hidutil-state' || return 1
  return 0
}

# Snapshot foreign hidutil mappings using the NEW helper binary.
# $1 = helper binary path (must pass sw_helper_has_capabilities)
# $2 = path to the config file. Prints foreign-only JSON to stdout.
sw_snapshot_legacy_foreign() {
  local binary="$1" config="$2"
  if ! sw_helper_has_capabilities "$binary"; then
    sw_err "helper binary at $binary does not support --snapshot-legacy-foreign-hidutil. Upgrade to the current version first, or run the installer/uninstaller from a current release archive."
  fi
  env -i HOME="$(sw_real_home)" PATH=/usr/bin:/bin \
    "$binary" --snapshot-legacy-foreign-hidutil "$config" \
    || sw_err "snapshot-legacy-foreign-hidutil failed"
}

sw_hidutil_restore_json() {
  local json="$1"
  env -i PATH=/usr/bin:/bin "${SW_USR_HIDUTIL}" property --set "{\"UserKeyMapping\":${json}}" >/dev/null 2>&1 \
    || sw_err "hidutil --set failed during restore"
}

# ── transactional app/plist swap with rollback ────────────────────────
#
# CRITICAL: pre-existing app/plist state is captured BEFORE the rollback trap
# is installed. This ensures early failure (before backup) never deletes a
# pre-existing installation. Transaction phases track which objects the
# transaction created/replaced, so rollback only removes/replaces what this
# transaction actually changed.

_SW_APP_DIR=""
_SW_APP_HAD_PRIOR="no"
_SW_APP_BACKUP=""
_SW_PLIST_DST=""
_SW_PLIST_HAD_PRIOR="no"
_SW_PLIST_BACKUP=""
_SW_NEW_LABEL=""
_SW_NEW_WAS_RUNNING="no"
_SW_OLD_WAS_RUNNING="no"
_SW_OLD_LABEL=""
_SW_STAGING=""
# Transaction phases: "init" → "app_backed_up" → "app_swapped" → "plist_swapped" → "bootstrapped" → "done"
# Track whether this transaction bootstrapped the new label (so rollback
# can boot it out — but ONLY after revalidating identity).
_SW_BOOTSTRAPPED_BY_US="no"
_SW_PHASE="init"

# Capture pre-existing state BEFORE installing the trap. Called first.
# FAIL-CLOSED: if either label is loaded but does NOT match Switcheroo's
# exact live program, abort immediately before any mutation — a foreign
# job occupying the label must never be silently overwritten or booted out.
sw_capture_state() {
  local app_dir="$1" plist_dst="$2" new_label="$3" old_label="$4"
  _SW_APP_DIR="$app_dir"
  _SW_PLIST_DST="$plist_dst"
  _SW_NEW_LABEL="$new_label"
  _SW_OLD_LABEL="$old_label"

  if [ -d "$app_dir" ] && [ ! -L "$app_dir" ]; then
    _SW_APP_HAD_PRIOR="yes"
  else
    _SW_APP_HAD_PRIOR="no"
  fi

  if [ -f "$plist_dst" ] && [ ! -L "$plist_dst" ]; then
    _SW_PLIST_HAD_PRIOR="yes"
  else
    _SW_PLIST_HAD_PRIOR="no"
  fi

  # Check new label: if loaded, must be Switcheroo or abort.
  if sw_label_is_loaded "$new_label"; then
    if sw_loaded_job_is_switcheroo "$new_label"; then
      _SW_NEW_WAS_RUNNING="yes"
    else
      sw_err "ABORT: label ${new_label} is loaded but its live program is not Switcheroo (foreign job collision) — refusing to proceed"
    fi
  fi

  # Check old label: if loaded, must be Switcheroo or abort.
  if sw_label_is_loaded "$old_label"; then
    if sw_loaded_job_is_switcheroo "$old_label"; then
      _SW_OLD_WAS_RUNNING="yes"
    else
      sw_err "ABORT: label ${old_label} is loaded but its live program is not Switcheroo (foreign job collision) — refusing to proceed"
    fi
  fi
}

sw_rollback() {
  local rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "==> Install failed (rc=$rc); rolling back..." >&2
    # 1. Boot out the failed new job ONLY if this transaction bootstrapped it.
    #    Revalidate exact live identity before bootout — never boot out a
    #    foreign job that may have taken the label.
    if [ "$_SW_BOOTSTRAPPED_BY_US" = "yes" ] && [ -n "$_SW_NEW_LABEL" ]; then
      if sw_loaded_job_is_switcheroo "$_SW_NEW_LABEL" 2>/dev/null; then
        "${SW_BIN_LAUNCHCTL}" bootout "gui/$(sw_uid)/${_SW_NEW_LABEL}" 2>/dev/null || true
      fi
    fi
    # 2. Restore app (before plist, so a restarted job uses the right binary).
    # app_backed_up: prior app was moved to backup but staged move may have
    #   failed — restore from backup.
    # app_swapped and later: same — restore from backup if prior existed,
    #   remove transaction-created app if no prior.
    if [ "$_SW_PHASE" = "app_backed_up" ] || [ "$_SW_PHASE" = "app_swapped" ] || [ "$_SW_PHASE" = "plist_swapped" ] || [ "$_SW_PHASE" = "bootstrapped" ]; then
      if [ "$_SW_APP_HAD_PRIOR" = "yes" ] && [ -n "$_SW_APP_BACKUP" ] && [ -d "$_SW_APP_BACKUP" ]; then
        "${SW_BIN_RM}" -rf "$_SW_APP_DIR" 2>/dev/null || true
        "${SW_BIN_MV}" "$_SW_APP_BACKUP" "$_SW_APP_DIR" 2>/dev/null || true
      elif [ "$_SW_APP_HAD_PRIOR" = "no" ]; then
        [ -n "$_SW_APP_DIR" ] && [ -d "$_SW_APP_DIR" ] && "${SW_BIN_RM}" -rf "$_SW_APP_DIR" 2>/dev/null || true
      fi
    fi
    # 3. Restore plist (only if this transaction swapped it).
    if [ "$_SW_PHASE" = "plist_swapped" ] || [ "$_SW_PHASE" = "bootstrapped" ]; then
      if [ "$_SW_PLIST_HAD_PRIOR" = "yes" ] && [ -n "$_SW_PLIST_BACKUP" ] && [ -f "$_SW_PLIST_BACKUP" ]; then
        "${SW_BIN_MV}" -f "$_SW_PLIST_BACKUP" "$_SW_PLIST_DST" 2>/dev/null || true
      elif [ "$_SW_PLIST_HAD_PRIOR" = "no" ]; then
        [ -n "$_SW_PLIST_DST" ] && "${SW_BIN_RM}" -f "$_SW_PLIST_DST" 2>/dev/null || true
      fi
    fi
    # 4. Restart only jobs proven running before the transaction.
    #    Revalidate plist identity before re-bootstrapping — if the plist
    #    was replaced or raced to a foreign program, do NOT bootstrap it.
    if [ "$_SW_NEW_WAS_RUNNING" = "yes" ] && [ -f "$_SW_PLIST_DST" ]; then
      if sw_plist_is_switcheroo "$_SW_PLIST_DST" 2>/dev/null; then
        "${SW_BIN_SLEEP}" 1
        "${SW_BIN_LAUNCHCTL}" bootstrap "gui/$(sw_uid)" "$_SW_PLIST_DST" 2>/dev/null || true
      else
        echo "==> ROLLBACK WARNING: new-label plist identity changed — not re-bootstrapping (possible race)" >&2
      fi
    fi
    local _old_plist
    _old_plist="$(sw_real_home)/Library/LaunchAgents/${_SW_OLD_LABEL}.plist"
    if [ "$_SW_OLD_WAS_RUNNING" = "yes" ] && [ -f "$_old_plist" ]; then
      if sw_plist_is_switcheroo "$_old_plist" 2>/dev/null; then
        "${SW_BIN_SLEEP}" 1
        "${SW_BIN_LAUNCHCTL}" bootstrap "gui/$(sw_uid)" "$_old_plist" 2>/dev/null || true
      else
        echo "==> ROLLBACK WARNING: old-label plist identity changed — not re-bootstrapping (possible race)" >&2
      fi
    fi
  fi
  [ -n "${_SW_STAGING:-}" ] && "${SW_BIN_RM}" -rf "$_SW_STAGING" 2>/dev/null || true
}

sw_render_plist() {
  local plist_src="$1" app_dir="$2" plist_dst_dir="$3" tmp
  tmp="$("${SW_USR_MKTEMP}" -p "$plist_dst_dir" -t switcheroo.plist.XXXXXX)"
  "${SW_USR_SED}" "s|__APP_DIR__|${app_dir}|g" "$plist_src" > "$tmp" \
    || { "${SW_BIN_RM}" -f "$tmp"; sw_err "plist render (sed) failed"; }
  "${SW_BIN_CHMOD}" 644 "$tmp"
  if ! "${SW_USR_PLUTIL}" -lint "$tmp" >/dev/null 2>&1; then
    "${SW_BIN_RM}" -f "$tmp"
    sw_err "generated plist failed plutil -lint"
  fi
  printf '%s\n' "$tmp"
}
