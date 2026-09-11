#!/bin/sh
# ─────────────────────────────────────────────────────────────────────
# scripts/generate_icns.sh — generate bundle/AppIcon.icns from the
# tracked 1024×1024 master PNG.
#
# This runs OUTSIDE the Homebrew build sandbox. `iconutil -c icns`
# cannot run inside Homebrew's seatbelt profile (deny mach-lookup),
# so the Formula copies this pre-built file instead of generating at
# install time (see packaging/homebrew/switcheroo.rb.tpl).
#
# Produces the 10 standard iconutil-recognized PNGs (16, 32, 128, 256,
# 512 + @2x each) — NOT the 12-file set that includes the non-standard
# icon_64x64 pair that iconutil silently ignores.
#
# Usage: scripts/generate_icns.sh [master.png] [output.icns]
#   defaults: bundle/AppIcon-1024.png  bundle/AppIcon.icns
#
# Validates output: ICNS magic, declared length, chunk lengths, and
# round-trip decodability via `iconutil -c iconset`.
# ─────────────────────────────────────────────────────────────────────
set -eu

PATH=/usr/bin:/bin
export PATH

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
REPO_ROOT=$(cd "${SCRIPT_DIR}/.." && pwd)

MASTER="${1:-${REPO_ROOT}/bundle/AppIcon-1024.png}"
OUTPUT="${2:-${REPO_ROOT}/bundle/AppIcon.icns}"

[ -f "$MASTER" ] || { echo "Error: master PNG not found: $MASTER" >&2; exit 1; }
[ -L "$MASTER" ] && { echo "Error: master PNG is a symlink — refusing: $MASTER" >&2; exit 1; }

SIPS=/usr/bin/sips
ICONUTIL=/usr/bin/iconutil
MKTEMP=/usr/bin/mktemp
RM=/bin/rm
STAT=/usr/bin/stat
PYTHON3=/usr/bin/python3

WORK=$($MKTEMP -d -t switcheroo-icns.XXXXXX)
trap '$RM -rf "$WORK"' EXIT
ICONSET="$WORK/AppIcon.iconset"
mkdir "$ICONSET"

# 10 standard iconutil-recognized sizes: sips_px:basename
# (drops the non-standard icon_64x64.png pair iconutil ignores)
for spec in \
  "16:16x16" "32:16x16@2x" \
  "32:32x32" "64:32x32@2x" \
  "128:128x128" "256:128x128@2x" \
  "256:256x256" "512:256x256@2x" \
  "512:512x512" "1024:512x512@2x"; do
  px=${spec%%:*}
  name=${spec#*:}
  $SIPS -z "$px" "$px" "$MASTER" --out "$ICONSET/icon_${name}.png" >/dev/null 2>&1 \
    || { echo "Error: sips failed for icon_${name}.png" >&2; exit 1; }
  [ -f "$ICONSET/icon_${name}.png" ] || { echo "Error: sips did not produce icon_${name}.png" >&2; exit 1; }
done

# Verify exactly 10 PNGs
COUNT=$(find "$ICONSET" -name '*.png' -type f | wc -l | tr -d ' ')
[ "$COUNT" -eq 10 ] || { echo "Error: expected 10 PNGs, got $COUNT" >&2; exit 1; }

$ICONUTIL -c icns "$ICONSET" -o "$OUTPUT" \
  || { echo "Error: iconutil -c icns failed" >&2; exit 1; }
[ -f "$OUTPUT" ] || { echo "Error: iconutil did not produce $OUTPUT" >&2; exit 1; }

# ── Validate output ICNS ─────────────────────────────────────────────
$PYTHON3 - "$OUTPUT" <<'PY'
import struct, sys, subprocess, tempfile, os

path = sys.argv[1]
with open(path, "rb") as f:
    data = f.read()

assert data[0:4] == b"icns", "FAIL: bad ICNS magic"
declared = struct.unpack(">I", data[4:8])[0]
actual = len(data)
assert declared == actual, f"FAIL: declared {declared} != actual {actual}"

offset = 8
total = 8
while offset < len(data):
    cl = struct.unpack(">I", data[offset+4:offset+8])[0]
    assert cl >= 8, f"FAIL: chunk len {cl} < 8 at offset {offset}"
    total += cl
    offset += cl
assert total == actual, f"FAIL: chunk sum {total} != actual {actual}"

# Round-trip decode test
with tempfile.TemporaryDirectory() as td:
    out = os.path.join(td, "decoded.iconset")
    subprocess.run(["/usr/bin/iconutil", "-c", "iconset", path, "-o", out], check=True)
    pngs = sorted(os.listdir(out))
    assert len(pngs) == 10, f"FAIL: round-trip extracted {len(pngs)}, expected 10"

print(f"PASS: {actual} bytes, magic OK, lengths match, round-trip decodes (10 PNGs)")
PY

echo "Generated: $OUTPUT"
echo "Size: $($STAT -f%z "$OUTPUT") bytes"
echo "SHA-256: $(/usr/bin/shasum -a 256 "$OUTPUT" | awk '{print $1}')"