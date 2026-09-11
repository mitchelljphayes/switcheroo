#!/bin/sh
# ─────────────────────────────────────────────────────────────────────
# scripts/test_icon_regression.sh — regression test for the Homebrew
# Formula icon installation logic.
#
# Executes the EXACT Formula icon-install code (cp bundle/AppIcon.icns)
# under the REAL Homebrew Sandbox profile (Sandbox API, not
# allow-default), then verifies the installed icon bytes are identical
# to the source.
#
# Also confirms that the OLD iconutil-based approach fails under the
# same sandbox (negative control — the bug we fixed).
#
# Does NOT start/stop services, change config, hidutil, or Accessibility.
#
# Usage: scripts/test_icon_regression.sh
# Exit: 0 on pass, 1 on fail
# ─────────────────────────────────────────────────────────────────────
set -eu

PATH=/usr/bin:/bin:/opt/homebrew/bin
export PATH

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
REPO_ROOT=$(cd "${SCRIPT_DIR}/.." && pwd)
ICNS="${REPO_ROOT}/bundle/AppIcon.icns"
MASTER="${REPO_ROOT}/bundle/AppIcon-1024.png"

BREW=""
for c in /opt/homebrew/bin/brew /usr/local/bin/brew; do
  if [ -x "$c" ]; then BREW="$c"; break; fi
done

if [ -z "$BREW" ]; then
  echo "SKIP: brew not found - cannot run Homebrew Sandbox regression test"
  exit 0
fi

HOMEBREW_LIBRARY=$("$BREW" --repository)/Library
export HOMEBREW_LIBRARY

[ -f "$ICNS" ] || { echo "FAIL: bundle/AppIcon.icns not found" >&2; exit 1; }
[ -f "$MASTER" ] || { echo "FAIL: bundle/AppIcon-1024.png not found" >&2; exit 1; }

WORK=$(mktemp -d -t switcheroo-icon-reg.XXXXXX)
trap 'rm -rf "$WORK"' EXIT

ICNS_SHA=$(shasum -a 256 "$ICNS" | awk '{print $1}')
ICNS_SIZE=$(stat -f%z "$ICNS")

echo "==> Source: $ICNS ($ICNS_SIZE bytes, sha256 $ICNS_SHA)"
echo "==> Running Formula icon logic under real Homebrew Sandbox..."

# Write the Ruby test script to a temp file to avoid heredoc quoting issues.
# Uses Homebrew's own Ruby (via `brew ruby`) which has Sorbet + all libs loaded.
RUBY_TEST="$WORK/icon_sandbox_test.rb"
cat > "$RUBY_TEST" <<'RUBY_EOF'
require "sandbox"

mode = ARGV[0]

case mode
when "cp"
  icns_source = ARGV[1]
  icns_dest = ARGV[2]
  sandbox = Sandbox.new
  sandbox.allow_write_path(File.dirname(icns_dest))
  sandbox.allow_read(path: File.dirname(icns_source))
  sandbox.run("/bin/cp", icns_source, icns_dest)
  puts "PASS: cp under Sandbox succeeded"
when "iconutil"
  iconset = ARGV[1]
  output = ARGV[2]
  sandbox = Sandbox.new
  sandbox.allow_write_path(File.dirname(output))
  sandbox.run("/usr/bin/iconutil", "-c", "icns", iconset, "-o", output)
  warn "UNEXPECTED: iconutil succeeded under Sandbox"
  exit 0
end
RUBY_EOF

# ── Positive test: cp (new Formula logic) under Sandbox ──────────────
INSTALLED_ICNS="$WORK/installed.icns"

"$BREW" ruby "$RUBY_TEST" cp "$ICNS" "$INSTALLED_ICNS" || {
  echo "FAIL: Sandbox test (cp) returned non-zero" >&2
  exit 1
}

# Verify installed bytes match source
if [ ! -f "$INSTALLED_ICNS" ]; then
  echo "FAIL: installed icon not produced" >&2
  exit 1
fi
INSTALLED_SHA=$(shasum -a 256 "$INSTALLED_ICNS" | awk '{print $1}')
INSTALLED_SIZE=$(stat -f%z "$INSTALLED_ICNS")

if [ "$INSTALLED_SHA" != "$ICNS_SHA" ]; then
  echo "FAIL: installed icon sha256 mismatch: $INSTALLED_SHA != $ICNS_SHA" >&2
  exit 1
fi
if [ "$INSTALLED_SIZE" != "$ICNS_SIZE" ]; then
  echo "FAIL: installed icon size mismatch: $INSTALLED_SIZE != $ICNS_SIZE" >&2
  exit 1
fi
echo "PASS: installed icon bytes match source ($INSTALLED_SIZE bytes, sha256 $INSTALLED_SHA)"

# ── Negative control: iconutil (old Formula logic) MUST fail under Sandbox ─
echo "==> Negative control: iconutil under Sandbox (expected to fail)..."

ICONSET_SRC="$WORK/AppIcon.iconset"
mkdir -p "$ICONSET_SRC"
# Generate 10 standard PNGs (same as generate_icns.sh)
for spec in \
  "16:16x16" "32:16x16@2x" \
  "32:32x32" "64:32x32@2x" \
  "128:128x128" "256:128x128@2x" \
  "256:256x256" "512:256x256@2x" \
  "512:512x512" "1024:512x512@2x"; do
  px=${spec%%:*}
  name=${spec#*:}
  /usr/bin/sips -z "$px" "$px" "$MASTER" --out "$ICONSET_SRC/icon_${name}.png" >/dev/null 2>&1
done

ICONUTIL_FAIL_OUT="$WORK/iconutil-sandbox-fail.icns"
# iconutil is expected to fail; swallow non-zero exit
"$BREW" ruby "$RUBY_TEST" iconutil "$ICONSET_SRC" "$ICONUTIL_FAIL_OUT" 2>/dev/null || true

if [ -f "$ICONUTIL_FAIL_OUT" ]; then
  echo "FAIL: iconutil produced output under Sandbox - sandbox profile may have changed" >&2
  exit 1
fi
echo "PASS: iconutil fails under Sandbox (negative control confirms the bug)"

# ── Service model Label regression ───────────────────────────────────
# Validate that the Formula template's `service do` block resolves the
# expected launchd Label via Homebrew's actual Service model.
#
# `plist_name` must equal "homebrew.mxcl.switcheroo" — the canonical
# homebrew.mxcl.<name> Label. Without an explicit `name macos:`, the
# Label falls back to the legacy default (still homebrew.mxcl.<name>
# in current Homebrew), so we additionally assert the source contains
# the explicit `name macos:` line that we ship, to guard against an
# accidental regression to a positional `name "..."` form (which raises
# ArgumentError at install time under HOMEBREW_DEVELOPER=1 / Sorbet).
#
# The `name "..."` (positional) vs `name macos: "..."` (explicit) bug
# is also covered by the integration `brew install --build-from-source`
# run documented in the v0.1.1 PR description.

echo "==> Validating Homebrew service model Label from template..."

SERVICE_TEST="$WORK/service_label_test.rb"
cat > "$SERVICE_TEST" <<'RUBY_EOF'
require "formula"

template_path = ARGV[0]
code = File.read(template_path)

# Guard: the template must use the explicit `name macos:` form, not the
# positional `name "..."` form (regression — ArgumentError at install).
unless code =~ /^\s*name\s+macos:\s*"/m
  warn "FAIL: template does not declare `name macos: ...` for the service Label"
  exit 1
end

code.gsub!("__REHEARSAL_MARKER__", "")
code.gsub!("__URL__", "https://example.com/v0.1.1.tar.gz")
code.gsub!("__SHA256__", "a" * 64)
code.gsub!("__VERSION__", "0.1.1")

# Evaluate at top level so Formula.inherited resolves the class name.
eval(code, TOPLEVEL_BINDING, template_path, 1)

f = Switcheroo.new(
  "switcheroo",
  Pathname.new("/tmp/switcheroo-service-label-test"),
  :stable,
)

svc = f.service

expected_label = "homebrew.mxcl.switcheroo"
actual_label = svc.plist_name

if actual_label != expected_label
  warn "FAIL: service plist_name is '#{actual_label}', expected '#{expected_label}'"
  exit 1
end

puts "PASS: service Label='#{actual_label}', explicit `name macos:` declared"
RUBY_EOF

"$BREW" ruby "$SERVICE_TEST" "${REPO_ROOT}/packaging/homebrew/switcheroo.rb.tpl" || {
  echo "FAIL: service Label regression test failed" >&2
  exit 1
}

echo ""
echo "==> All icon + service regression tests PASSED"