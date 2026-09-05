#!/bin/bash
# ─────────────────────────────────────────────────────────────────────
# scripts/version-sync.sh — validate version consistency across surfaces.
#
# Cargo.toml is the canonical version source. This script validates that
# all other version surfaces exist and match:
#   - bundle/Info.plist CFBundleShortVersionString + CFBundleVersion
#   - CHANGELOG.md top `## [X.Y.Z]` entry
#   - Built binary --version (when --binary <path> is provided)
#   - Git tag (when --tag vX.Y.Z is provided)
#
# Fail-closed: all expected surfaces must exist and match. Missing
# files are errors, not skips (unless --allow-missing is provided for
# development).
#
# Usage:
#   scripts/version-sync.sh [--tag vX.Y.Z] [--binary /path/to/switcheroo]
#                           [--allow-missing]
#
# Exits 0 if all provided surfaces match, 1 on any mismatch or missing file.
# ─────────────────────────────────────────────────────────────────────
set -euo pipefail

PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH

SW_USR_PLUTIL=/usr/bin/plutil
SW_USR_GREP=/usr/bin/grep
SW_USR_SED=/usr/bin/sed
SW_USR_HEAD=/usr/bin/head

SW_ERR() { printf 'Error: %s\n' "$*" >&2; exit 1; }

TAG=""
BINARY_PATH=""
ALLOW_MISSING=false

while [ $# -gt 0 ]; do
  case "$1" in
    --tag) TAG="$2"; shift 2;;
    --binary) BINARY_PATH="$2"; shift 2;;
    --allow-missing) ALLOW_MISSING=true; shift;;
    *) SW_ERR "unknown argument: $1";;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# ── Read Cargo.toml version (canonical) ──────────────────────────────
CARGO_VER="$($SW_USR_SED -n '/^\[package\]/,/^\[/p' "${REPO_ROOT}/Cargo.toml" | $SW_USR_GREP -m1 '^version' | $SW_USR_SED 's/version = "\(.*\)"/\1/')"
[ -n "$CARGO_VER" ] || SW_ERR "could not read version from Cargo.toml"

# Validate strict semver
printf '%s' "$CARGO_VER" | $SW_USR_GREP -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' \
  || SW_ERR "Cargo.toml version '$CARGO_VER' is not strict semver X.Y.Z"

echo "Canonical version (Cargo.toml): $CARGO_VER"

ERRORS=0

# ── Validate Info.plist ──────────────────────────────────────────────
PLIST_PATH="${REPO_ROOT}/bundle/Info.plist"
if [ ! -f "$PLIST_PATH" ]; then
  if [ "$ALLOW_MISSING" = "true" ]; then
    echo "  SKIP: bundle/Info.plist not found (--allow-missing)"
  else
    echo "  FAIL: bundle/Info.plist not found"
    ERRORS=$((ERRORS + 1))
  fi
else
  PLIST_SHORT="$($SW_USR_PLUTIL -extract CFBundleShortVersionString raw -o - "$PLIST_PATH" 2>/dev/null || printf '')"
  PLIST_VER="$($SW_USR_PLUTIL -extract CFBundleVersion raw -o - "$PLIST_PATH" 2>/dev/null || printf '')"
  if [ "$PLIST_SHORT" != "$CARGO_VER" ]; then
    echo "  MISMATCH: Info.plist CFBundleShortVersionString='$PLIST_SHORT' != '$CARGO_VER'"
    ERRORS=$((ERRORS + 1))
  else
    echo "  OK: Info.plist CFBundleShortVersionString=$PLIST_SHORT"
  fi
  if [ "$PLIST_VER" != "$CARGO_VER" ]; then
    echo "  MISMATCH: Info.plist CFBundleVersion='$PLIST_VER' != '$CARGO_VER'"
    ERRORS=$((ERRORS + 1))
  else
    echo "  OK: Info.plist CFBundleVersion=$PLIST_VER"
  fi
fi

# ── Validate CHANGELOG.md ────────────────────────────────────────────
CHANGELOG_PATH="${REPO_ROOT}/CHANGELOG.md"
if [ ! -f "$CHANGELOG_PATH" ]; then
  if [ "$ALLOW_MISSING" = "true" ]; then
    echo "  SKIP: CHANGELOG.md not found (--allow-missing)"
  else
    echo "  FAIL: CHANGELOG.md not found"
    ERRORS=$((ERRORS + 1))
  fi
else
  CHANGELOG_VER="$($SW_USR_GREP -m1 -E '^## \[' "$CHANGELOG_PATH" | $SW_USR_SED -E 's/^## \[([0-9.]+)\].*/\1/' || true)"
  if [ -z "$CHANGELOG_VER" ]; then
    echo "  FAIL: CHANGELOG.md has no version entry"
    ERRORS=$((ERRORS + 1))
  elif [ "$CHANGELOG_VER" != "$CARGO_VER" ]; then
    echo "  MISMATCH: CHANGELOG.md top entry='$CHANGELOG_VER' != '$CARGO_VER'"
    ERRORS=$((ERRORS + 1))
  else
    echo "  OK: CHANGELOG.md top entry=$CHANGELOG_VER"
  fi
fi

# ── Validate tag (when provided) ─────────────────────────────────────
if [ -n "$TAG" ]; then
  if ! printf '%s' "$TAG" | $SW_USR_GREP -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "  MISMATCH: tag '$TAG' is not semver 'vX.Y.Z'"
    ERRORS=$((ERRORS + 1))
  else
    TAG_VER="${TAG#v}"
    if [ "$TAG_VER" != "$CARGO_VER" ]; then
      echo "  MISMATCH: tag version='$TAG_VER' != '$CARGO_VER'"
      ERRORS=$((ERRORS + 1))
    else
      echo "  OK: tag=$TAG"
    fi
  fi
fi

# ── Validate binary --version (when provided) ────────────────────────
if [ -n "$BINARY_PATH" ]; then
  if [ -x "$BINARY_PATH" ]; then
    BIN_VER_LINE="$("$BINARY_PATH" --version 2>/dev/null | $SW_USR_HEAD -1 || true)"
    BIN_VER="$(printf '%s' "$BIN_VER_LINE" | $SW_USR_SED 's/^switcheroo //' || true)"
    if [ -z "$BIN_VER" ]; then
      echo "  FAIL: binary at $BINARY_PATH did not produce --version output"
      ERRORS=$((ERRORS + 1))
    elif ! printf '%s' "$BIN_VER" | $SW_USR_GREP -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
      echo "  FAIL: binary --version not exact semver: '$BIN_VER_LINE'"
      ERRORS=$((ERRORS + 1))
    elif [ "$BIN_VER" != "$CARGO_VER" ]; then
      echo "  MISMATCH: binary --version='$BIN_VER' != '$CARGO_VER'"
      ERRORS=$((ERRORS + 1))
    else
      echo "  OK: binary --version=switcheroo $BIN_VER"
    fi
  else
    echo "  FAIL: binary at $BINARY_PATH not found or not executable"
    ERRORS=$((ERRORS + 1))
  fi
fi

# ── Result ───────────────────────────────────────────────────────────
if [ "$ERRORS" -gt 0 ]; then
  echo ""
  echo "FAIL: $ERRORS version mismatch(es) found"
  exit 1
fi

echo ""
echo "version sync OK"