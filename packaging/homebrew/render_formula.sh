#!/bin/bash
# ─────────────────────────────────────────────────────────────────────
# packaging/homebrew/render_formula.sh — render the Homebrew Formula.
#
# Usage (production):
#   render_formula.sh --version <X.Y.Z> --sha256 <64-lowercase-hex>
#
# Usage (local rehearsal):
#   render_formula.sh --version <X.Y.Z> --local-archive <path> --rehearsal
#
# Production: URL is constructed internally from the version. No
# arbitrary URL input accepted. SHA must be exactly 64 lowercase hex.
#
# Rehearsal: The local archive path is validated (absolute, regular
# non-symlink file, safe characters only). The archive is copied to a
# script-created safe filename, and the file:// URL + checksum are
# derived from that copy. The rendered formula contains an unmistakable
# non-publishable rehearsal marker. No caller-supplied path enters the
# sed expression directly.
#
# Security: no arbitrary URL/Ruby fragments are accepted. All inputs
# are strictly validated. Output is written via mktemp + atomic rename.
# ─────────────────────────────────────────────────────────────────────
set -euo pipefail

PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH

SW_USR_SED=/usr/bin/sed
SW_USR_GREP=/usr/bin/grep
SW_USR_SHASUM=/usr/bin/shasum
SW_USR_MKTEMP=/usr/bin/mktemp
SW_BIN_RM=/bin/rm
SW_BIN_MV=/bin/mv

# Trap to clean up safe archive directory on exit
_SAFE_ARCHIVE_DIR=""
_render_cleanup() {
  if [ -n "$_SAFE_ARCHIVE_DIR" ] && [ -d "$_SAFE_ARCHIVE_DIR" ]; then
    $SW_BIN_RM -rf "$_SAFE_ARCHIVE_DIR" 2>/dev/null || true
  fi
}
trap _render_cleanup EXIT
SW_BIN_CP=/bin/cp

SW_ERR() { printf 'Error: %s\n' "$*" >&2; exit 1; }

REPO_OWNER="mitchelljphayes"
REPO_NAME="switcheroo"
GITHUB_BASE="https://github.com/${REPO_OWNER}/${REPO_NAME}"

VERSION=""
SHA256=""
LOCAL_ARCHIVE=""
REHEARSAL=false

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2;;
    --sha256) SHA256="$2"; shift 2;;
    --local-archive) LOCAL_ARCHIVE="$2"; shift 2;;
    --rehearsal) REHEARSAL=true; shift;;
    *) SW_ERR "unknown argument: $1";;
  esac
done

[ -n "$VERSION" ] || SW_ERR "--version is required"
printf '%s' "$VERSION" | $SW_USR_GREP -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' \
  || SW_ERR "--version must be strict semver X.Y.Z (got: '$VERSION')"

SCRIPT_DIR="${0%/*}"
case "$SCRIPT_DIR" in
  /*) ;;
  *) SCRIPT_DIR="${PWD}/${SCRIPT_DIR}";;
esac
SCRIPT_DIR="$(cd "$SCRIPT_DIR" && pwd)"
TEMPLATE="${SCRIPT_DIR}/switcheroo.rb.tpl"
OUTPUT="${SCRIPT_DIR}/switcheroo.rb"
[ -f "$TEMPLATE" ] || SW_ERR "template not found: $TEMPLATE"

# ── Determine mode ────────────────────────────────────────────────────
if [ "$REHEARSAL" = "true" ]; then
  [ -n "$LOCAL_ARCHIVE" ] || SW_ERR "--rehearsal requires --local-archive <path>"
  [ -z "$SHA256" ] || SW_ERR "--sha256 must not be provided in rehearsal mode"

  # Validate archive path: must be absolute, not a symlink, regular file
  case "$LOCAL_ARCHIVE" in
    /*) ;;
    *) SW_ERR "--local-archive must be an absolute path (got: '$LOCAL_ARCHIVE')";;
  esac
  [ -L "$LOCAL_ARCHIVE" ] && SW_ERR "--local-archive is a symlink — refusing: $LOCAL_ARCHIVE"
  [ -f "$LOCAL_ARCHIVE" ] || SW_ERR "--local-archive not found or not a regular file: $LOCAL_ARCHIVE"
  [ -p "$LOCAL_ARCHIVE" ] && SW_ERR "--local-archive is a FIFO — refusing: $LOCAL_ARCHIVE"

  # Reject paths with unsafe characters (spaces, quotes, backslashes, control chars, |, &, ;, etc.)
  # Allow: alphanumeric, /, -, _, ., : (for file:// URLs)
  case "$LOCAL_ARCHIVE" in
    *[!a-zA-Z0-9/._-]*) SW_ERR "--local-archive contains unsafe characters — refusing: $LOCAL_ARCHIVE";;
  esac

  # Copy archive to a script-created safe filename (eliminates any path injection)
  # The directory is cleaned up by the trap on exit
  _SAFE_ARCHIVE_DIR="$($SW_USR_MKTEMP -d -t switcheroo.rehearsal.XXXXXX)"
  SAFE_ARCHIVE_DIR="$_SAFE_ARCHIVE_DIR"
  SAFE_ARCHIVE="${SAFE_ARCHIVE_DIR}/switcheroo-${VERSION}-source-rehearsal.tar.gz"
  $SW_BIN_CP "$LOCAL_ARCHIVE" "$SAFE_ARCHIVE"
  [ -f "$SAFE_ARCHIVE" ] || { $SW_BIN_RM -rf "$SAFE_ARCHIVE_DIR"; SW_ERR "failed to copy archive"; }

  # Derive URL and checksum from the safe copy
  SHA256="$($SW_USR_SHASUM -a 256 "$SAFE_ARCHIVE" | { read -r h _; printf '%s' "$h"; })"
  URL="file://${SAFE_ARCHIVE}"

  echo "==> Rendering formula [REHEARSAL — NOT FOR PUBLICATION]"
  echo "    Version:  $VERSION"
  echo "    URL:      $URL"
  echo "    SHA-256:  $SHA256"
  echo "    Source:   $LOCAL_ARCHIVE → $SAFE_ARCHIVE (safe copy)"
  echo "    NOTE:     This is a REHEARSAL formula. Do not publish or distribute."
else
  [ -n "$SHA256" ] || SW_ERR "production mode requires --sha256 <64-lowercase-hex>"
  [ -z "$LOCAL_ARCHIVE" ] || SW_ERR "--local-archive is only valid with --rehearsal"
  printf '%s' "$SHA256" | $SW_USR_GREP -Eq '^[0-9a-f]{64}$' \
    || SW_ERR "--sha256 must be exactly 64 lowercase hex (got: '$SHA256')"
  URL="${GITHUB_BASE}/archive/refs/tags/v${VERSION}.tar.gz"
  echo "==> Rendering formula [PRODUCTION]"
  echo "    Version: $VERSION"
  echo "    URL:     $URL"
  echo "    SHA-256: $SHA256"
fi

# ── Substitute placeholders ───────────────────────────────────────────
# Determine rehearsal marker substitution
if [ "$REHEARSAL" = "true" ]; then
  REHEARSAL_LINE="# REHEARSAL — DO NOT PUBLISH — THIS IS NOT A PRODUCTION FORMULA"
else
  REHEARSAL_LINE=""
fi

if [ -L "$OUTPUT" ]; then
  SW_ERR "output path is a symlink — refusing: $OUTPUT"
fi
TMP_OUTPUT="$($SW_USR_MKTEMP -t switcheroo.rb.XXXXXX)"

$SW_USR_SED \
  -e "s|__URL__|${URL}|g" \
  -e "s|__SHA256__|${SHA256}|g" \
  -e "s|__VERSION__|${VERSION}|g" \
  -e "s|__REHEARSAL_MARKER__|${REHEARSAL_LINE}|g" \
  "$TEMPLATE" > "$TMP_OUTPUT" || {
  $SW_BIN_RM -f "$TMP_OUTPUT"
  [ -n "${SAFE_ARCHIVE_DIR:-}" ] && $SW_BIN_RM -rf "$SAFE_ARCHIVE_DIR"
  SW_ERR "sed substitution failed"
}

# Verify no placeholders remain (including rehearsal marker)
if $SW_USR_GREP -qE '__URL__|__SHA256__|__VERSION__|__REHEARSAL_MARKER__' "$TMP_OUTPUT"; then
  $SW_BIN_RM -f "$TMP_OUTPUT"
  [ -n "${SAFE_ARCHIVE_DIR:-}" ] && $SW_BIN_RM -rf "$SAFE_ARCHIVE_DIR"
  SW_ERR "rendered formula still contains unresolved placeholders"
fi

# For rehearsal: verify the DO NOT PUBLISH marker is present
if [ "$REHEARSAL" = "true" ]; then
  if ! $SW_USR_GREP -q 'DO NOT PUBLISH' "$TMP_OUTPUT"; then
    $SW_BIN_RM -f "$TMP_OUTPUT"
    [ -n "${SAFE_ARCHIVE_DIR:-}" ] && $SW_BIN_RM -rf "$SAFE_ARCHIVE_DIR"
    SW_ERR "rehearsal formula missing DO NOT PUBLISH marker"
  fi
fi

$SW_BIN_MV -f "$TMP_OUTPUT" "$OUTPUT"
echo "    Rendered: $OUTPUT"

# Note: safe archive copy is cleaned up by the EXIT trap after brew style.
# For tap-based audit/install rehearsal, copy the rendered formula to a
# tap before this script exits, or run brew commands within this script's
# lifetime (the safe archive is available until trap fires).

# ── Optional: brew style ──────────────────────────────────────────────
BREW=""
for c in /opt/homebrew/bin/brew /usr/local/bin/brew; do
  if [ -x "$c" ]; then BREW="$c"; break; fi
done

if [ -n "$BREW" ]; then
  echo "==> Running brew style"
  if "$BREW" style "$OUTPUT" 2>&1; then
    echo "    OK: brew style"
  else
    echo "    WARNING: brew style reported issues (non-blocking)"
  fi
  # Note: brew audit --strict --online requires a tap (not a file path).
  # See RELEASING.md for the exact tap-based audit command.
else
  echo "    (brew not available — skipping style)"
fi

echo "==> Formula rendered successfully"