#!/bin/bash
# ─────────────────────────────────────────────────────────────────────
# scripts/package.sh — deterministic release packaging for Switcheroo.
#
# RELEASE MODE (--tag vX.Y.Z --commit-sha <40-hex>):
#   Exports the validated commit into a fresh private source tree for
#   compilation, and archives the source via `git archive <COMMIT_SHA>`
#   (the exact validated commit SHA) — not the exported build tree.
#   Builds only from that tree under a minimal environment. No
#   caller-controlled --cargo; toolchain is validated against
#   rust-toolchain.toml. Output is a fresh-private mktemp directory.
#   Artifacts are release-named (no -rehearsal suffix). Manifest is
#   deterministic (commit timestamp, no wall clock).
#
# REHEARSAL MODE (default):
#   Uses the working tree. Produces -rehearsal-named artifacts. Native
#   fallback permitted with --allow-native-fallback. Never looks like a
#   release.
#
# Usage:
#   scripts/package.sh [--dry-run] [--out-parent <path>]
#                       [--tag vX.Y.Z --commit-sha <40-hex>]
#                       [--cargo /path/to/cargo]  (rehearsal only)
#                       [--verify-reproducibility]
#                       [--allow-native-fallback]
#
# Safety:
#   - Never touches live LaunchAgents, user home, or git state.
#   - PATH=/usr/bin:/bin:/usr/sbin:/sbin; umask 077.
#   - All system tools invoked by absolute path via SW_* variables or
#     shell builtins.
#   - Output is a fresh-private mktemp directory under a validated parent.
#     Never overwrites caller-controlled paths. Never rm -rf caller paths.
#   - All payload inputs validated as regular non-symlink files.
#   - Cleanup only removes script-owned temp directories.
# ─────────────────────────────────────────────────────────────────────
set -euo pipefail

PATH=/usr/bin:/bin:/usr/sbin:/sbin
export PATH
umask 077

# ── Absolute tool paths ──────────────────────────────────────────────
SW_BIN_MKDIR=/bin/mkdir
SW_BIN_CP=/bin/cp
SW_BIN_RM=/bin/rm
SW_BIN_CHMOD=/bin/chmod
SW_USR_TAR=/usr/bin/tar
SW_USR_GZIP=/usr/bin/gzip
SW_USR_SIPS=/usr/bin/sips
SW_USR_ICONUTIL=/usr/bin/iconutil
SW_USR_CODESIGN=/usr/bin/codesign
SW_USR_LIPO=/usr/bin/lipo
SW_USR_FILE=/usr/bin/file
SW_USR_PLUTIL=/usr/bin/plutil
SW_USR_SHASUM=/usr/bin/shasum
SW_USR_GREP=/usr/bin/grep
SW_USR_SED=/usr/bin/sed
SW_USR_HEAD=/usr/bin/head
SW_USR_MKTEMP=/usr/bin/mktemp
SW_USR_GIT=/usr/bin/git
SW_USR_FIND=/usr/bin/find
SW_USR_TOUCH=/usr/bin/touch
SW_USR_STAT=/usr/bin/stat
SW_USR_ID=/usr/bin/id
SW_USR_DIRNAME=/usr/bin/dirname
SW_USR_BASENAME=/usr/bin/basename
SW_USR_UNAME=/usr/bin/uname
SW_USR_CHOWN=/usr/sbin/chown

SW_ERR() { printf 'Error: %s\n' "$*" >&2; exit 1; }

sw_shasum256_field1() {
  $SW_USR_SHASUM -a 256 "$1" | { read -r h _; printf '%s' "$h"; }
}

# Validate a path is a regular file, not a symlink.
sw_assert_regular_file() {
  local f="$1"
  [ -L "$f" ] && SW_ERR "path is a symlink — refusing: $f"
  [ -f "$f" ] || SW_ERR "not a regular file: $f"
}

# Walk every ancestor of a path, rejecting symlinks, wrong owner, or
# group/world-writable directories. System temp roots ($TMPDIR, /var/folders)
# are allowed to have group/world-writable ancestors because macOS manages
# them with restricted permissions and sticky bits.
sw_validate_ancestors() {
  local path="$1" current_uid
  current_uid="$($SW_USR_ID -u 2>/dev/null || printf 0)"
  path="$($SW_USR_DIRNAME "$path")"
  while [ "$path" != "/" ] && [ -n "$path" ]; do
    if [ -L "$path" ]; then
      # macOS has known system symlinks (/var -> /private/var, /tmp -> /private/tmp)
      # Resolve and check the target instead of rejecting
      local resolved
      resolved="$(readlink "$path" 2>/dev/null || true)"
      case "$path:$resolved" in
        /var:private/var|/tmp:private/tmp|/etc:private/etc)
          # Known macOS system symlink — safe
          ;;
        *)
          SW_ERR "ancestor is a symlink — refusing: $path (→ ${resolved:-unknown})"
          ;;
      esac
    fi
    if [ -d "$path" ]; then
      local owner mode
      owner="$($SW_USR_STAT -f '%u' "$path" 2>/dev/null || printf 'err')"
      mode="$($SW_USR_STAT -f '%Lp' "$path" 2>/dev/null || printf '000')"
      if [ $(( 8#$mode & 0022 )) -ne 0 ]; then
        case "$path" in
          /tmp|/var/tmp|/var/folders|/private/tmp|/private/var/tmp|/private/var/folders|"${TMPDIR:-/nonexistent}"|"${TMPDIR%/}"*)
            ;;
          *)
            SW_ERR "ancestor is group/world-writable (mode $mode) — refusing: $path"
            ;;
        esac
      fi
      if [ $(( 8#$mode & 0200 )) -ne 0 ] && [ "$owner" != "$current_uid" ] && [ "$owner" != "0" ]; then
        SW_ERR "writable ancestor owned by uid $owner (not current/root) — refusing: $path"
      fi
    fi
    path="$($SW_USR_DIRNAME "$path")"
  done
}

# ── Parse arguments ───────────────────────────────────────────────────
DRY_RUN=false
OUT_PARENT=""
TAG=""
COMMIT_SHA=""
CARGO_BIN=""
VERIFY_REPRODUCIBILITY=false
ALLOW_NATIVE_FALLBACK=false

while [ $# -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=true; shift;;
    --out-parent) OUT_PARENT="$2"; shift 2;;
    --out-dir) SW_ERR "--out-dir removed: use --out-parent for a validated parent directory";;
    --tag) TAG="$2"; shift 2;;
    --commit-sha) COMMIT_SHA="$2"; shift 2;;
    --cargo) CARGO_BIN="$2"; shift 2;;
    --verify-reproducibility) VERIFY_REPRODUCIBILITY=true; shift;;
    --allow-native-fallback) ALLOW_NATIVE_FALLBACK=true; shift;;
    *) SW_ERR "unknown argument: $1";;
  esac
done

# ── Determine mode ────────────────────────────────────────────────────
RELEASE_MODE=false
if [ -n "$COMMIT_SHA" ]; then
  RELEASE_MODE=true
  [ -n "$TAG" ] || SW_ERR "--commit-sha requires --tag"
  # Release mode: no --cargo allowed (toolchain is validated)
  [ -z "$CARGO_BIN" ] || SW_ERR "--cargo is not allowed in release mode (toolchain is validated automatically)"
elif [ -n "$TAG" ]; then
  SW_ERR "--tag requires --commit-sha for release mode"
fi

if [ "$VERIFY_REPRODUCIBILITY" = "true" ] && [ "$RELEASE_MODE" = "true" ]; then
  SW_ERR "--verify-reproducibility is only valid in rehearsal mode"
fi

# ── Resolve repo root ─────────────────────────────────────────────────
SW_SCRIPT_PATH="$0"
case "$SW_SCRIPT_PATH" in
  /*) ;;
  *) SW_SCRIPT_PATH="${PWD}/${SW_SCRIPT_PATH}";;
esac
SCRIPT_DIR="$($SW_USR_DIRNAME "$SW_SCRIPT_PATH")"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

# ── Resolve output parent and create fresh-private output dir ────────
if [ -z "$OUT_PARENT" ]; then
  OUT_PARENT="${REPO_ROOT}/dist-parent"
fi

# Resolve to absolute
case "$OUT_PARENT" in
  /*) ;;
  *) OUT_PARENT="${REPO_ROOT}/${OUT_PARENT}";;
esac

# Validate output parent ancestors (no symlinks in chain)
sw_validate_ancestors "$OUT_PARENT"

# If parent exists, validate it's a safe directory
if [ -e "$OUT_PARENT" ]; then
  [ -L "$OUT_PARENT" ] && SW_ERR "out-parent is a symlink — refusing: $OUT_PARENT"
  [ -d "$OUT_PARENT" ] || SW_ERR "out-parent exists but is not a directory — refusing: $OUT_PARENT"
  current_uid="$($SW_USR_ID -u 2>/dev/null || printf 0)"
  owner="$($SW_USR_STAT -f '%u' "$OUT_PARENT" 2>/dev/null || printf 'err')"
  [ "$owner" = "$current_uid" ] || \
    SW_ERR "out-parent owned by uid $owner (not $current_uid) — refusing: $OUT_PARENT"
  mode="$($SW_USR_STAT -f '%Lp' "$OUT_PARENT" 2>/dev/null || printf '000')"
  if [ $(( 8#$mode & 0022 )) -ne 0 ]; then
    SW_ERR "out-parent is group/world-writable (mode $mode) — refusing: $OUT_PARENT"
  fi
fi

$SW_BIN_MKDIR -p "$OUT_PARENT"

# Create a fresh-private unique output directory (0700 via umask)
OUT_DIR="$($SW_USR_MKTEMP -d -p "$OUT_PARENT" -t switcheroo-out.XXXXXX)"
$SW_BIN_CHMOD 700 "$OUT_DIR"

# ── Temp directories and cleanup ──────────────────────────────────────
# _CARGO_TARGET_DIR: a script-owned isolated target dir so every packaging
# invocation (including the Run 2 subprocess of --verify-reproducibility)
# builds cold into its own clean target. This makes the two rehearsal runs
# symmetric and eliminates cold-vs-warm shared-target-cache nondeterminism.
# Release mode already builds in a fresh exported tree; routing it through
# the same isolated target keeps binary-copy paths unified.
_TEMP_ICONSET=""
_VERIFY_DIR=""
_REPRO_TOP=""
_BUILD_TREE=""
_CARGO_TARGET_DIR=""
cleanup() {
  for d in "$_TEMP_ICONSET" "$_VERIFY_DIR" "$_REPRO_TOP" "$_BUILD_TREE" "$_CARGO_TARGET_DIR"; do
    if [ -n "$d" ] && [ -d "$d" ]; then
      $SW_BIN_RM -rf "$d" 2>/dev/null || true
    fi
  done
}
trap cleanup EXIT

# ── Read canonical version from Cargo.toml ───────────────────────────
CARGO_VERSION="$($SW_USR_SED -n '/^\[package\]/,/^\[/p' Cargo.toml | $SW_USR_GREP -m1 '^version' | $SW_USR_SED 's/version = "\(.*\)"/\1/')"
[ -n "$CARGO_VERSION" ] || SW_ERR "could not read version from Cargo.toml"
printf '%s' "$CARGO_VERSION" | $SW_USR_GREP -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' \
  || SW_ERR "Cargo.toml version '$CARGO_VERSION' is not strict semver X.Y.Z"

# ── Tag/SHA validation (release mode) ────────────────────────────────
if [ "$RELEASE_MODE" = "true" ]; then
  printf '%s' "$TAG" | $SW_USR_GREP -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' \
    || SW_ERR "tag '$TAG' is not semver 'vX.Y.Z'"
  TAG_VERSION="${TAG#v}"
  [ "$TAG_VERSION" = "$CARGO_VERSION" ] \
    || SW_ERR "tag version '$TAG_VERSION' != Cargo.toml version '$CARGO_VERSION'"
  printf '%s' "$COMMIT_SHA" | $SW_USR_GREP -Eq '^[0-9a-f]{40}$' \
    || SW_ERR "--commit-sha must be a 40-char hex SHA: '$COMMIT_SHA'"
  # Assert HEAD == COMMIT_SHA
  HEAD_SHA="$($SW_USR_GIT rev-parse HEAD 2>/dev/null || true)"
  [ "$HEAD_SHA" = "$COMMIT_SHA" ] \
    || SW_ERR "release mode requires HEAD == commit-sha. HEAD=$HEAD_SHA, requested=$COMMIT_SHA"
  # Assert completely clean worktree (tracked + untracked)
  if ! $SW_USR_GIT diff --quiet HEAD 2>/dev/null; then
    SW_ERR "release mode requires a clean worktree (unstaged tracked changes detected)"
  fi
  if ! $SW_USR_GIT diff --cached --quiet HEAD 2>/dev/null; then
    SW_ERR "release mode requires a clean worktree (staged changes detected)"
  fi
  # Check for untracked files (excluding .githooks which are GitButler-managed)
  UNTRACKED="$($SW_USR_GIT ls-files --others --exclude-standard 2>/dev/null | $SW_USR_GREP -v '^\.' || true)"
  if [ -n "$UNTRACKED" ]; then
    # Allow .githooks and .opencode (session files) as untracked
    REAL_UNTRACKED="$(printf '%s\n' "$UNTRACKED" | $SW_USR_GREP -v '^\.githooks/' | $SW_USR_GREP -v '^\.opencode/' || true)"
    if [ -n "$REAL_UNTRACKED" ]; then
      SW_ERR "release mode requires a clean worktree (untracked files detected): $REAL_UNTRACKED"
    fi
  fi
  VERSION="$TAG_VERSION"
  ARCHIVE_TAG="$TAG"
  ARCHIVE_SUFFIX=""
  MODE_LABEL="RELEASE"
else
  VERSION="$CARGO_VERSION"
  ARCHIVE_TAG="v${VERSION}"
  ARCHIVE_SUFFIX="-rehearsal"
  MODE_LABEL="REHEARSAL"
fi

echo "==> Switcheroo packaging [$MODE_LABEL]"
echo "    Version:  $VERSION"
echo "    Tag:      ${TAG:-<none>}"
echo "    Commit:   ${COMMIT_SHA:-<none>}"
echo "    Dry-run:  $DRY_RUN"
echo "    Out-dir:  $OUT_DIR"

# ── For release mode: export commit into a fresh private source tree ─
# This ensures the build is hermetic — no untracked files, no .cargo/config,
# no inherited workspace state. We build only from the exported tree.
BUILD_ROOT="$REPO_ROOT"  # default: build from current workspace
if [ "$RELEASE_MODE" = "true" ]; then
  echo "==> Exporting commit $COMMIT_SHA into private source tree"
  _BUILD_TREE="$($SW_USR_MKTEMP -d -t switcheroo.build-tree.XXXXXX)"
  $SW_USR_GIT archive --format=tar "$COMMIT_SHA" | $SW_USR_TAR -x -C "$_BUILD_TREE"
  BUILD_ROOT="$_BUILD_TREE"
  echo "    Build tree: $_BUILD_TREE"
fi

# ── Isolated cargo target directory ────────────────────────────────────
# Every packaging invocation builds into its own script-owned, fresh-private
# target dir. This guarantees cold, symmetric builds for both runs of
# --verify-reproducibility (Run 1 and the Run 2 subprocess each get a clean
# target instead of sharing the workspace target/ cache), and keeps release
# mode hermetic. Cleaned by the EXIT trap.
_CARGO_TARGET_DIR="$($SW_USR_MKTEMP -d -t switcheroo.target.XXXXXX)"
CARGO_TARGET_DIR="$_CARGO_TARGET_DIR"
export CARGO_TARGET_DIR
echo "    Target dir: $CARGO_TARGET_DIR"

# ── Discover/validate cargo ───────────────────────────────────────────
if [ "$RELEASE_MODE" = "true" ]; then
  # Release: use validated cargo from known paths only
  for c in "${HOME}/.cargo/bin/cargo" /opt/homebrew/bin/cargo /usr/local/bin/cargo; do
    if [ -x "$c" ]; then CARGO_BIN="$c"; break; fi
  done
  [ -n "$CARGO_BIN" ] || SW_ERR "cargo not found for release build"
  # Validate cargo: must exist, be executable, and report as cargo.
  # (rustup proxies are symlinks to rustup — that's expected; we verify
  # the proxy runs cargo correctly via --version, not the symlink target.)
  [ -L "$CARGO_BIN" ] || [ -f "$CARGO_BIN" ] || SW_ERR "cargo at $CARGO_BIN is not a file or symlink"
  CARGO_OWNER="$($SW_USR_STAT -L -f '%u' "$CARGO_BIN" 2>/dev/null || printf 'err')"
  [ "$CARGO_OWNER" = "$($SW_USR_ID -u)" ] || \
    SW_ERR "cargo at $CARGO_BIN owned by uid $CARGO_OWNER — refusing"
  CARGO_VER_OUT="$("$CARGO_BIN" --version 2>/dev/null || true)"
  printf '%s' "$CARGO_VER_OUT" | $SW_USR_GREP -q '^cargo ' \
    || SW_ERR "cargo at $CARGO_BIN does not report as cargo: '$CARGO_VER_OUT'"
  # Verify rustc version matches rust-toolchain.toml pin
  RUSTC_BIN="$($SW_USR_DIRNAME "$CARGO_BIN")/rustc"
  [ -x "$RUSTC_BIN" ] || RUSTC_BIN="${HOME}/.cargo/bin/rustc"
  RUSTC_VER="$("$RUSTC_BIN" --version 2>/dev/null || true)"
  EXPECTED_RUSTC="rustc 1.95.0"
  printf '%s' "$RUSTC_VER" | $SW_USR_GREP -q "^${EXPECTED_RUSTC}" \
    || SW_ERR "rustc version '$RUSTC_VER' does not match pinned '${EXPECTED_RUSTC}' (rust-toolchain.toml)"
else
  # Rehearsal: discover cargo with optional --cargo
  if [ -z "$CARGO_BIN" ]; then
    for c in "${HOME}/.cargo/bin/cargo" /opt/homebrew/bin/cargo /usr/local/bin/cargo; do
      if [ -x "$c" ]; then CARGO_BIN="$c"; break; fi
    done
  fi
  [ -n "$CARGO_BIN" ] || SW_ERR "cargo not found. Pass --cargo /absolute/path."
fi

# Sanitize build environment
unset RUSTC_WRAPPER 2>/dev/null || true
export CARGO_BUILD_RUSTC_WRAPPER=""

# ── Validate Info.plist version ───────────────────────────────────────
PLIST_PATH="${BUILD_ROOT}/bundle/Info.plist"
sw_assert_regular_file "$PLIST_PATH"
PLIST_SHORT_VER="$($SW_USR_PLUTIL -extract CFBundleShortVersionString raw -o - "$PLIST_PATH" 2>/dev/null || printf '')"
PLIST_VER="$($SW_USR_PLUTIL -extract CFBundleVersion raw -o - "$PLIST_PATH" 2>/dev/null || printf '')"
[ "$PLIST_SHORT_VER" = "$CARGO_VERSION" ] \
  || SW_ERR "Info.plist CFBundleShortVersionString='$PLIST_SHORT_VER' != Cargo='$CARGO_VERSION'"
[ "$PLIST_VER" = "$CARGO_VERSION" ] \
  || SW_ERR "Info.plist CFBundleVersion='$PLIST_VER' != Cargo='$CARGO_VERSION'"

# ── Validate CHANGELOG.md ─────────────────────────────────────────────
CHANGELOG_PATH="${BUILD_ROOT}/CHANGELOG.md"
if [ ! -f "$CHANGELOG_PATH" ]; then
  [ "$RELEASE_MODE" = "false" ] || SW_ERR "CHANGELOG.md required in release mode"
  echo "    WARNING: CHANGELOG.md not found (rehearsal — skipping)"
else
  CHANGELOG_VER="$($SW_USR_GREP -m1 -E '^## \[' "$CHANGELOG_PATH" | $SW_USR_SED -E 's/^## \[([0-9.]+)\].*/\1/' || true)"
  if [ -z "$CHANGELOG_VER" ]; then
    [ "$RELEASE_MODE" = "false" ] || SW_ERR "CHANGELOG.md has no version entry (required in release mode)"
    echo "    WARNING: CHANGELOG.md has no version entry (rehearsal — skipping)"
  elif [ "$CHANGELOG_VER" != "$VERSION" ]; then
    SW_ERR "CHANGELOG.md top entry '$CHANGELOG_VER' != expected '$VERSION'"
  fi
fi

# ── Validate icon master ──────────────────────────────────────────────
ICON_MASTER="${BUILD_ROOT}/bundle/AppIcon-1024.png"
sw_assert_regular_file "$ICON_MASTER"

# ── Build universal binary ────────────────────────────────────────────
echo "==> Building universal binary (aarch64 + x86_64)"

build_native_only=false

if [ "$RELEASE_MODE" = "true" ]; then
  # Release: both targets MUST succeed — no fallback
  # Hermetic build: use a private empty CARGO_HOME seeded with locked
  # dependencies to avoid consuming mutable user Cargo config, credentials,
  # or proxies. Fetch with the user's CARGO_HOME (which has the registry
  # index), then build with --frozen under a sanitized private environment.
  PRIVATE_HOME="$($SW_USR_MKTEMP -d -t switcheroo.home.XXXXXX)"
  PRIVATE_CARGO_HOME="$PRIVATE_HOME/.cargo"
  $SW_BIN_MKDIR -p "$PRIVATE_CARGO_HOME"

  # Fetch locked dependencies using the user's CARGO_HOME (has registry index).
  # This populates the user's cache with the exact locked versions.
  env -i \
    HOME="$HOME" \
    PATH="/usr/bin:/bin:/usr/sbin:/sbin" \
    CARGO_HOME="$HOME/.cargo" \
    RUSTUP_HOME="$HOME/.rustup" \
    CARGO_BUILD_RUSTC_WRAPPER="" \
    "$CARGO_BIN" fetch --locked --manifest-path "$BUILD_ROOT/Cargo.toml" 2>/dev/null \
    || SW_ERR "release: cargo fetch --locked failed"

  # Copy the fetched registry cache to the private CARGO_HOME
  if [ -d "$HOME/.cargo/registry" ]; then
    $SW_BIN_CP -R "$HOME/.cargo/registry" "$PRIVATE_CARGO_HOME/registry"
  fi
  if [ -d "$HOME/.cargo/git" ]; then
    $SW_BIN_CP -R "$HOME/.cargo/git" "$PRIVATE_CARGO_HOME/git"
  fi

  # Build both targets under --frozen (offline, no network, no user config)
  # Include cargo bin dir in PATH so rustup proxies can find rustc
  CARGO_BIN_DIR="$($SW_USR_DIRNAME "$CARGO_BIN")"
  env -i \
    HOME="$PRIVATE_HOME" \
    PATH="${CARGO_BIN_DIR}:/usr/bin:/bin:/usr/sbin:/sbin" \
    CARGO_HOME="$PRIVATE_CARGO_HOME" \
    CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
    RUSTUP_HOME="$HOME/.rustup" \
    CARGO_BUILD_RUSTC_WRAPPER="" \
    "$CARGO_BIN" build --release --frozen --manifest-path "$BUILD_ROOT/Cargo.toml" --target aarch64-apple-darwin \
    || SW_ERR "release: aarch64 build failed"
  env -i \
    HOME="$PRIVATE_HOME" \
    PATH="${CARGO_BIN_DIR}:/usr/bin:/bin:/usr/sbin:/sbin" \
    CARGO_HOME="$PRIVATE_CARGO_HOME" \
    CARGO_TARGET_DIR="$CARGO_TARGET_DIR" \
    RUSTUP_HOME="$HOME/.rustup" \
    CARGO_BUILD_RUSTC_WRAPPER="" \
    "$CARGO_BIN" build --release --frozen --manifest-path "$BUILD_ROOT/Cargo.toml" --target x86_64-apple-darwin \
    || SW_ERR "release: x86_64 build failed"
  # Clean up private HOME
  $SW_BIN_RM -rf "$PRIVATE_HOME"
else
  # Rehearsal: allow native fallback
  if ! "$CARGO_BIN" build --release --locked --target aarch64-apple-darwin 2>/dev/null; then
    echo "    (aarch64 target not available — falling back to native-only)"
    build_native_only=true
  fi
  if [ "$build_native_only" = "false" ]; then
    if ! "$CARGO_BIN" build --release --locked --target x86_64-apple-darwin 2>/dev/null; then
      echo "    (x86_64 target not available — falling back to native-only)"
      build_native_only=true
    fi
  fi
fi

UNIVERSAL_BINARY="$OUT_DIR/switcheroo-universal"

if [ "$build_native_only" = "true" ]; then
  [ "$RELEASE_MODE" = "false" ] || SW_ERR "release mode requires universal build"
  echo "==> Building native binary only (cross targets not installed)"
  "$CARGO_BIN" build --release --locked --manifest-path "$BUILD_ROOT/Cargo.toml"
  $SW_BIN_CP "$CARGO_TARGET_DIR/release/switcheroo" "$UNIVERSAL_BINARY"
  ARCH_LABEL="native"
  ARCH_NAME="$($SW_USR_UNAME -m)"
else
  echo "==> Creating universal binary via lipo"
  $SW_USR_LIPO -create -output "$UNIVERSAL_BINARY" \
    "$CARGO_TARGET_DIR/aarch64-apple-darwin/release/switcheroo" \
    "$CARGO_TARGET_DIR/x86_64-apple-darwin/release/switcheroo"
  ARCHS="$($SW_USR_LIPO -archs "$UNIVERSAL_BINARY" 2>/dev/null || true)"
  if [ "$ARCHS" != "x86_64 arm64" ] && [ "$ARCHS" != "arm64 x86_64" ]; then
    SW_ERR "universal binary architecture mismatch: '$ARCHS'"
  fi
  ARCH_LABEL="universal"
  ARCH_NAME="universal"
fi

$SW_USR_FILE "$UNIVERSAL_BINARY" | $SW_USR_GREP -q 'Mach-O' \
  || SW_ERR "built binary is not a Mach-O executable"

echo "==> Ad-hoc signing universal binary"
$SW_USR_CODESIGN --force --sign - "$UNIVERSAL_BINARY"

# Verify --version (exact match)
echo "==> Verifying binary --version"
BIN_VERSION_LINE="$("$UNIVERSAL_BINARY" --version 2>/dev/null | $SW_USR_HEAD -1 || true)"
BIN_VERSION="$(printf '%s' "$BIN_VERSION_LINE" | $SW_USR_SED 's/^switcheroo //' || true)"
printf '%s' "$BIN_VERSION" | $SW_USR_GREP -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' \
  || SW_ERR "binary --version not exact 'switcheroo X.Y.Z': got '$BIN_VERSION_LINE'"
[ "$BIN_VERSION" = "$VERSION" ] \
  || SW_ERR "binary --version '$BIN_VERSION' != expected '$VERSION'"
echo "    OK: switcheroo $BIN_VERSION"

# ── Generate AppIcon.icns ─────────────────────────────────────────────
echo "==> Generating AppIcon.icns from bundle/AppIcon-1024.png"
_TEMP_ICONSET="$($SW_USR_MKTEMP -d -t switcheroo.iconset.XXXXXX)"
ICONSET_DIR="${_TEMP_ICONSET}/AppIcon.iconset"
$SW_BIN_MKDIR -p "$ICONSET_DIR"

generate_icon() {
  local size="$1" out_name="$2"
  $SW_USR_SIPS -z "$size" "$size" "$ICON_MASTER" --out "$ICONSET_DIR/$out_name" >/dev/null 2>&1 \
    || SW_ERR "sips failed for $out_name"
}

generate_icon 16   "icon_16x16.png"
generate_icon 32   "icon_16x16@2x.png"
generate_icon 32   "icon_32x32.png"
generate_icon 64   "icon_32x32@2x.png"
generate_icon 64   "icon_64x64.png"
generate_icon 128  "icon_64x64@2x.png"
generate_icon 128  "icon_128x128.png"
generate_icon 256  "icon_128x128@2x.png"
generate_icon 256  "icon_256x256.png"
generate_icon 512  "icon_256x256@2x.png"
generate_icon 512  "icon_512x512.png"
generate_icon 1024 "icon_512x512@2x.png"

ICNS_OUTPUT="$OUT_DIR/AppIcon.icns"
$SW_USR_ICONUTIL -c icns "$ICONSET_DIR" -o "$ICNS_OUTPUT" \
  || SW_ERR "iconutil -c icns failed"
[ -f "$ICNS_OUTPUT" ] || SW_ERR "iconutil did not produce AppIcon.icns"
$SW_BIN_RM -rf "$_TEMP_ICONSET"
_TEMP_ICONSET=""

# ── Assemble Switcheroo.app ───────────────────────────────────────────
echo "==> Assembling Switcheroo.app"
APP_DIR="$OUT_DIR/Switcheroo.app"
$SW_BIN_MKDIR -p "$APP_DIR/Contents/MacOS"
$SW_BIN_MKDIR -p "$APP_DIR/Contents/Resources"

$SW_BIN_CP "$UNIVERSAL_BINARY" "$APP_DIR/Contents/MacOS/switcheroo"
$SW_BIN_CHMOD 755 "$APP_DIR/Contents/MacOS/switcheroo"

$SW_BIN_CP "$PLIST_PATH" "$APP_DIR/Contents/Info.plist"
$SW_USR_PLUTIL -replace CFBundleVersion -string "$VERSION" "$APP_DIR/Contents/Info.plist"
$SW_USR_PLUTIL -replace CFBundleShortVersionString -string "$VERSION" "$APP_DIR/Contents/Info.plist"
$SW_USR_PLUTIL -lint "$APP_DIR/Contents/Info.plist" >/dev/null

$SW_BIN_CP "$ICNS_OUTPUT" "$APP_DIR/Contents/Resources/AppIcon.icns"

echo "==> Ad-hoc signing Switcheroo.app"
$SW_USR_CODESIGN --force --sign - "$APP_DIR"
$SW_USR_CODESIGN --verify --strict "$APP_DIR" 2>/dev/null \
  || SW_ERR "codesign --verify failed on assembled .app"

BUNDLE_ID="$($SW_USR_PLUTIL -extract CFBundleIdentifier raw -o - "$APP_DIR/Contents/Info.plist" 2>/dev/null || printf '')"
[ "$BUNDLE_ID" = "com.mitchelljphayes.switcheroo" ] \
  || SW_ERR "wrong CFBundleIdentifier: '$BUNDLE_ID'"
[ -f "$APP_DIR/Contents/Resources/AppIcon.icns" ] \
  || SW_ERR "AppIcon.icns missing from assembled .app"
echo "    OK: .app assembled, signed, icon present"

# ── Dry-run stops here ────────────────────────────────────────────────
if [ "$DRY_RUN" = "true" ]; then
  echo "==> Dry-run complete (archives skipped) [$MODE_LABEL]"
  echo "    Binary: $UNIVERSAL_BINARY"
  echo "    App:    $APP_DIR"
  echo "    Icon:   $ICNS_OUTPUT"
  exit 0
fi

if [ "$build_native_only" = "true" ] && [ "$ALLOW_NATIVE_FALLBACK" = "false" ]; then
  echo "==> Rehearsal with native fallback — not producing archives."
  echo "    Use --allow-native-fallback to produce rehearsal archives."
  exit 0
fi

# ── Build source archive ──────────────────────────────────────────────
echo "==> Building source archive"
SOURCE_ARCHIVE="$OUT_DIR/switcheroo-${ARCHIVE_TAG}-source${ARCHIVE_SUFFIX}.tar.gz"
SOURCE_TAR="${SOURCE_ARCHIVE%.gz}"

if [ "$RELEASE_MODE" = "true" ]; then
  # Release: archive directly from the validated commit SHA via git archive.
  # This is deterministic and never includes target/ or build artifacts.
  # The exported build tree is used for compilation only, not for archiving.
  # Use -C to run from the original repo (we may be cd'd into the build tree).
  $SW_USR_GIT -C "$REPO_ROOT" archive --format=tar --prefix="switcheroo-${ARCHIVE_TAG}/" \
    --output "$SOURCE_TAR" "$COMMIT_SHA"
else
  # Rehearsal: archive HEAD
  $SW_USR_GIT -C "$REPO_ROOT" archive --format=tar --prefix="switcheroo-${ARCHIVE_TAG}/" \
    --output "$SOURCE_TAR" HEAD
fi
[ -f "$SOURCE_TAR" ] || SW_ERR "source tar was not created"

$SW_USR_GZIP -n -c "$SOURCE_TAR" > "$SOURCE_ARCHIVE"
$SW_BIN_RM -f "$SOURCE_TAR"
[ -f "$SOURCE_ARCHIVE" ] || SW_ERR "source archive was not created"

# ── Build binary archive ──────────────────────────────────────────────
echo "==> Building binary archive"
if [ "$ARCH_LABEL" = "universal" ]; then
  BINARY_ARCHIVE="$OUT_DIR/switcheroo-${ARCHIVE_TAG}-macos-universal${ARCHIVE_SUFFIX}.tar.gz"
else
  BINARY_ARCHIVE="$OUT_DIR/switcheroo-${ARCHIVE_TAG}-macos-${ARCH_NAME}${ARCHIVE_SUFFIX}.tar.gz"
fi
BINARY_TAR="${BINARY_ARCHIVE%.gz}"

# Set file mtimes to epoch for deterministic tar
$SW_USR_FIND "$OUT_DIR/Switcheroo.app" -exec $SW_USR_TOUCH -t 197001010000.00 {} + 2>/dev/null || true

# Normalize ownership for deterministic tar headers. macOS bsdtar records
# the actual uid/gid from the filesystem, and temp dirs under different
# filesystems (/tmp → wheel, /var/folders → staff) get different gids.
# chown to the current user's uid:gid so both --verify-reproducibility
# runs (whose OUT_DIRs may live on different filesystems) produce
# identical tar headers. chown to our own uid:gid needs no sudo and does
# not modify file contents or git state.
CURRENT_UIDGID="$($SW_USR_ID -u):$($SW_USR_ID -g)"
$SW_USR_FIND "$OUT_DIR/Switcheroo.app" -exec "$SW_USR_CHOWN" "$CURRENT_UIDGID" {} + 2>/dev/null || true

# Validate payload inputs are regular non-symlink files
for f in com.mitchelljphayes.switcheroo.plist install-binary.sh uninstall.sh scripts/lib.sh config.toml README.md LICENSE; do
  sw_assert_regular_file "${BUILD_ROOT}/${f}"
done

# Copy repo payload files into the OUT_DIR and normalize their ownership
# so all tar entries have a consistent uid:gid regardless of which
# filesystem BUILD_ROOT lives on. This avoids touching the working tree.
PAYLOAD_STAGING="$OUT_DIR/.payload"
$SW_BIN_MKDIR -p "$PAYLOAD_STAGING/scripts"
for f in com.mitchelljphayes.switcheroo.plist install-binary.sh uninstall.sh scripts/lib.sh config.toml README.md LICENSE; do
  $SW_BIN_CP "${BUILD_ROOT}/${f}" "$PAYLOAD_STAGING/${f}"
done
# Normalize ownership and mtimes for deterministic tar headers
$SW_USR_FIND "$PAYLOAD_STAGING" -exec "$SW_USR_CHOWN" "$CURRENT_UIDGID" {} + 2>/dev/null || true
$SW_USR_FIND "$PAYLOAD_STAGING" -exec $SW_USR_TOUCH -t 197001010000.00 {} + 2>/dev/null || true

$SW_USR_TAR -cf "$BINARY_TAR" \
  -C "$OUT_DIR" Switcheroo.app \
  -C "$PAYLOAD_STAGING" \
  com.mitchelljphayes.switcheroo.plist \
  install-binary.sh \
  uninstall.sh \
  scripts/lib.sh \
  config.toml \
  README.md \
  LICENSE

# Clean up the payload staging dir (no longer needed after tar is written)
$SW_BIN_RM -rf "$PAYLOAD_STAGING"

[ -f "$BINARY_TAR" ] || SW_ERR "binary tar was not created"
$SW_USR_GZIP -n -c "$BINARY_TAR" > "$BINARY_ARCHIVE"
$SW_BIN_RM -f "$BINARY_TAR"
[ -f "$BINARY_ARCHIVE" ] || SW_ERR "binary archive was not created"

# ── Generate and verify SHA-256 checksums ────────────────────────────
echo "==> Generating SHA-256 checksums"
CHECKSUMS_FILE="$OUT_DIR/checksums-sha256${ARCHIVE_SUFFIX}.txt"
SOURCE_ARCHIVE_NAME="$($SW_USR_BASENAME "$SOURCE_ARCHIVE")"
BINARY_ARCHIVE_NAME="$($SW_USR_BASENAME "$BINARY_ARCHIVE")"

(
  cd "$OUT_DIR" && \
  $SW_USR_SHASUM -a 256 "$SOURCE_ARCHIVE_NAME" "$BINARY_ARCHIVE_NAME" \
  > "$($SW_USR_BASENAME "$CHECKSUMS_FILE")"
)
while IFS= read -r line; do printf '%s\n' "$line"; done < "$CHECKSUMS_FILE"
( cd "$OUT_DIR" && $SW_USR_SHASUM -a 256 -c "$($SW_USR_BASENAME "$CHECKSUMS_FILE")" )

# ── Extract and self-verify binary archive ───────────────────────────
echo "==> Self-verifying extracted binary archive"
_VERIFY_DIR="$($SW_USR_MKTEMP -d -t switcheroo.verify.XXXXXX)"
$SW_USR_TAR -xzf "$BINARY_ARCHIVE" -C "$_VERIFY_DIR"

[ -d "$_VERIFY_DIR/Switcheroo.app" ] || SW_ERR "missing Switcheroo.app"
for f in com.mitchelljphayes.switcheroo.plist install-binary.sh uninstall.sh scripts/lib.sh config.toml README.md LICENSE; do
  [ -f "$_VERIFY_DIR/$f" ] || SW_ERR "missing $f in archive"
done

$SW_USR_PLUTIL -lint "$_VERIFY_DIR/Switcheroo.app/Contents/Info.plist" >/dev/null \
  || SW_ERR "extracted Info.plist fails plutil -lint"
$SW_USR_CODESIGN --verify --strict "$_VERIFY_DIR/Switcheroo.app" 2>/dev/null \
  || SW_ERR "extracted .app fails codesign --verify"

EXTRACTED_BIN="$_VERIFY_DIR/Switcheroo.app/Contents/MacOS/switcheroo"
EXTRACTED_VER_LINE="$("$EXTRACTED_BIN" --version 2>/dev/null | $SW_USR_HEAD -1 || true)"
EXTRACTED_VER="$(printf '%s' "$EXTRACTED_VER_LINE" | $SW_USR_SED 's/^switcheroo //' || true)"
printf '%s' "$EXTRACTED_VER" | $SW_USR_GREP -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' \
  || SW_ERR "extracted binary --version not exact semver: '$EXTRACTED_VER_LINE'"
[ "$EXTRACTED_VER" = "$VERSION" ] \
  || SW_ERR "extracted binary --version '$EXTRACTED_VER' != '$VERSION'"
[ -f "$_VERIFY_DIR/Switcheroo.app/Contents/Resources/AppIcon.icns" ] \
  || SW_ERR "extracted .app missing AppIcon.icns"

if [ "$ARCH_LABEL" = "universal" ]; then
  $SW_USR_FILE "$EXTRACTED_BIN" | $SW_USR_GREP -q 'universal' \
    || SW_ERR "extracted binary is not universal"
fi

EXTRACTED_PLIST_VER="$($SW_USR_PLUTIL -extract CFBundleShortVersionString raw -o - "$_VERIFY_DIR/Switcheroo.app/Contents/Info.plist" 2>/dev/null || printf '')"
[ "$EXTRACTED_PLIST_VER" = "$VERSION" ] \
  || SW_ERR "extracted plist version '$EXTRACTED_PLIST_VER' != '$VERSION'"

$SW_BIN_RM -rf "$_VERIFY_DIR"
_VERIFY_DIR=""
echo "    OK: extracted archive verified"

# ── Release manifest (deterministic) ──────────────────────────────────
if [ "$RELEASE_MODE" = "true" ]; then
  MANIFEST_FILE="$OUT_DIR/release-manifest.txt"
  # Deterministic timestamp from commit (not wall clock)
  COMMIT_TIMESTAMP="$($SW_USR_GIT -C "$REPO_ROOT" show -s --format='%ci' "$COMMIT_SHA" 2>/dev/null | $SW_USR_SED 's/ +0000/Z/' | $SW_USR_SED 's/ +[0-9]*$//' || printf 'unknown')"
  CARGO_VER_FULL="$("$CARGO_BIN" --version 2>/dev/null || printf 'unknown')"
  RUSTC_VER_FULL="$("$RUSTC_BIN" --version 2>/dev/null || printf 'unknown')"
  SCRIPT_SHA="$(sw_shasum256_field1 "$0")"
  SOURCE_HASH="$(sw_shasum256_field1 "$SOURCE_ARCHIVE")"
  BINARY_HASH="$(sw_shasum256_field1 "$BINARY_ARCHIVE")"
  # Validate hashes
  printf '%s' "$SOURCE_HASH" | $SW_USR_GREP -Eq '^[0-9a-f]{64}$' \
    || SW_ERR "manifest: source hash invalid: '$SOURCE_HASH'"
  printf '%s' "$BINARY_HASH" | $SW_USR_GREP -Eq '^[0-9a-f]{64}$' \
    || SW_ERR "manifest: binary hash invalid: '$BINARY_HASH'"

  {
    printf 'switcheroo release manifest\n'
    printf 'version: %s\n' "$VERSION"
    printf 'tag: %s\n' "$TAG"
    printf 'commit: %s\n' "$COMMIT_SHA"
    printf 'commit_timestamp: %s\n' "$COMMIT_TIMESTAMP"
    printf 'cargo: %s\n' "$CARGO_VER_FULL"
    printf 'rustc: %s\n' "$RUSTC_VER_FULL"
    printf 'targets: aarch64-apple-darwin,x86_64-apple-darwin\n'
    printf 'packaging_script_sha: %s\n' "$SCRIPT_SHA"
    printf '\n'
    printf 'artifacts:\n'
    printf '  %s: %s\n' "$SOURCE_ARCHIVE_NAME" "$SOURCE_HASH"
    printf '  %s: %s\n' "$BINARY_ARCHIVE_NAME" "$BINARY_HASH"
  } > "$MANIFEST_FILE"
  echo "==> Release manifest: $MANIFEST_FILE"
fi

# ── Reproducibility verification ──────────────────────────────────────
if [ "$VERIFY_REPRODUCIBILITY" = "true" ]; then
  echo "==> Verifying reproducibility (two-run hash comparison)"
  HASH1_SOURCE="$(sw_shasum256_field1 "$SOURCE_ARCHIVE")"
  HASH1_BINARY="$(sw_shasum256_field1 "$BINARY_ARCHIVE")"
  echo "    Run 1: source=$HASH1_SOURCE binary=$HASH1_BINARY"

  # Create one script-owned private top-level temp directory with two
  # dedicated run parents. Never enumerate or delete caller directories.
  _REPRO_TOP="$($SW_USR_MKTEMP -d -t switcheroo.repro-top.XXXXXX)"
  REPRO_PARENT2="$_REPRO_TOP/run2-parent"
  $SW_BIN_MKDIR -p "$REPRO_PARENT2"

  echo "    Running packaging again..."
  # Capture run 2's exact output dir via a machine-readable OUTPUT_DIR line
  if [ "$ALLOW_NATIVE_FALLBACK" = "true" ]; then
    R2_OUTPUT_LINE="$("$0" --out-parent "$REPRO_PARENT2" --cargo "$CARGO_BIN" --allow-native-fallback 2>&1)"
  else
    R2_OUTPUT_LINE="$("$0" --out-parent "$REPRO_PARENT2" --cargo "$CARGO_BIN" 2>&1)"
  fi
  R2_EXIT=$?
  if [ "$R2_EXIT" -ne 0 ]; then
    $SW_BIN_RM -rf "$_REPRO_TOP"
    _REPRO_TOP=""
    SW_ERR "reproducibility run 2 failed (exit $R2_EXIT)"
  fi

  # Extract the exact output dir from the machine-readable line
  R2_DIR="$(printf '%s\n' "$R2_OUTPUT_LINE" | $SW_USR_GREP '^OUTPUT_DIR=' | $SW_USR_SED 's/^OUTPUT_DIR=//')"
  [ -n "$R2_DIR" ] || {
    $SW_BIN_RM -rf "$_REPRO_TOP"
    _REPRO_TOP=""
    SW_ERR "reproducibility: could not capture run 2 output dir"
  }

  # Validate R2_DIR is a direct child of REPRO_PARENT2
  R2_PARENT="$($SW_USR_DIRNAME "$R2_DIR")"
  [ "$R2_PARENT" = "$REPRO_PARENT2" ] || {
    $SW_BIN_RM -rf "$_REPRO_TOP"
    _REPRO_TOP=""
    SW_ERR "reproducibility: run 2 output dir is not under dedicated parent"
  }

  if [ "$ARCH_LABEL" = "universal" ]; then
    R2_SOURCE="$R2_DIR/switcheroo-${ARCHIVE_TAG}-source${ARCHIVE_SUFFIX}.tar.gz"
    R2_BINARY="$R2_DIR/switcheroo-${ARCHIVE_TAG}-macos-universal${ARCHIVE_SUFFIX}.tar.gz"
  else
    R2_SOURCE="$R2_DIR/switcheroo-${ARCHIVE_TAG}-source${ARCHIVE_SUFFIX}.tar.gz"
    R2_BINARY="$R2_DIR/switcheroo-${ARCHIVE_TAG}-macos-${ARCH_NAME}${ARCHIVE_SUFFIX}.tar.gz"
  fi

  HASH2_SOURCE="$(sw_shasum256_field1 "$R2_SOURCE")"
  HASH2_BINARY="$(sw_shasum256_field1 "$R2_BINARY")"
  echo "    Run 2: source=$HASH2_SOURCE binary=$HASH2_BINARY"

  [ "$HASH1_SOURCE" = "$HASH2_SOURCE" ] || {
    $SW_BIN_RM -rf "$_REPRO_TOP"
    _REPRO_TOP=""
    SW_ERR "source NOT reproducible: $HASH1_SOURCE != $HASH2_SOURCE"
  }
  [ "$HASH1_BINARY" = "$HASH2_BINARY" ] || {
    $SW_BIN_RM -rf "$_REPRO_TOP"
    _REPRO_TOP=""
    SW_ERR "binary NOT reproducible: $HASH1_BINARY != $HASH2_BINARY"
  }
  echo "    OK: both archives byte-for-byte reproducible"
  # Delete only the script-owned top-level directory
  $SW_BIN_RM -rf "$_REPRO_TOP"
  _REPRO_TOP=""
fi

# ── Done ──────────────────────────────────────────────────────────────
echo ""
echo "==> Packaging complete [$MODE_LABEL]"
echo "    Source archive:  $SOURCE_ARCHIVE"
echo "    Binary archive:  $BINARY_ARCHIVE"
echo "    Checksums:       $CHECKSUMS_FILE"
if [ "$RELEASE_MODE" = "true" ]; then
  echo "    Manifest:        $MANIFEST_FILE"
fi
echo "    Universal binary: $UNIVERSAL_BINARY"
echo "    App bundle:      $APP_DIR"
if [ "$RELEASE_MODE" = "false" ]; then
  echo ""
  echo "    NOTE: These are REHEARSAL artifacts, not release artifacts."
  echo "    Do not publish. Use --tag vX.Y.Z --commit-sha <SHA> for release mode."
fi
# Machine-readable output for callers (e.g. reproducibility verification)
echo "OUTPUT_DIR=$OUT_DIR"