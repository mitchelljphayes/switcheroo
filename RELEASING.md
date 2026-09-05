# Releasing Switcheroo

This is the maintainer release runbook. Follow it step by step.

## Prerequisites

Before any release, verify the external prerequisites documented in
[`RELEASE_SETUP.md`](RELEASE_SETUP.md). The release workflow remains
read-only (no `contents: write`, no automated publication) until all
six prerequisites are verified.

**You do NOT need all six prerequisites to build artifacts.** The
current workflow builds and validates artifacts with `contents: read`
only. The prerequisites are needed only for automated publication.

## Version Bump Checklist

The canonical version source is `Cargo.toml` `[package] version`. All
other version surfaces are validated against it:

1. **`Cargo.toml`** — set `[package] version = "X.Y.Z"` (canonical).
2. **`bundle/Info.plist`** — update `CFBundleShortVersionString` and
   `CFBundleVersion` to `X.Y.Z`. (The release workflow overrides these
   via `plutil -replace`, but the committed values must match for the
   `validate-tag` check to pass. `install.sh` now stamps them from the
   built binary's `--version` output for source builds.)
3. **`CHANGELOG.md`** — add a `## [X.Y.Z] - YYYY-MM-DD` entry at the
   top. The `validate-tag` check refuses tags whose top changelog entry
   doesn't match.
4. **Tag** — `git tag -a vX.Y.Z -m "Release vX.Y.Z"` (after the commit
   is merged to `main`; the tag must point at a main-ancestor commit).
5. **Archives** — archive names embed the tag:
   `switcheroo-vX.Y.Z-source.tar.gz`,
   `switcheroo-vX.Y.Z-macos-universal.tar.gz`.
6. **Formula checksum** — computed from the actual source archive after
   the build; rendered into the Formula via
   `packaging/homebrew/render_formula.sh`.

### Version sync validation

Run this locally before tagging:

```bash
./scripts/version-sync.sh
# With a binary and tag:
./scripts/version-sync.sh --tag vX.Y.Z --binary ./dist/switcheroo-universal
```

All surfaces must report OK.

## Acceptance Commands

Run locally from the repo root (no commit, no push needed):

### Rust gates
```bash
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo test --locked
cargo build --locked --all-targets
cargo build --locked --all-features
```

### Shell gates
```bash
./scripts/test_lib.sh
shellcheck scripts/package.sh scripts/lib.sh scripts/version-sync.sh \
  install.sh install-binary.sh uninstall.sh \
  packaging/homebrew/render_formula.sh
```

### Workflow gates (if installed locally)
```bash
actionlint
zizmor .
```

### Packaging dry run + full run
```bash
# Dry run (no archive write):
./scripts/package.sh --dry-run --out-parent ./dist-parent

# Full run (writes archives + checksums to a fresh-private mktemp dir):
./scripts/package.sh --out-parent ./dist-parent
# The unique output directory is printed as "Out-dir:" in the output.

# Reproducibility check (two-run hash comparison):
./scripts/package.sh --verify-reproducibility --out-parent ./dist-parent --allow-native-fallback

# Verify checksums (from the unique output dir printed by package.sh):
# OUT_DIR=./dist-parent/switcheroo-out.XXXXXX  # from package.sh output
# shasum -a 256 -c "$OUT_DIR/checksums-sha256-rehearsal.txt"

# Extract and verify binary archive:
# mkdir -p /tmp/sw-accept && tar -xzf "$OUT_DIR/switcheroo-v0.1.0-macos-universal-rehearsal.tar.gz" -C /tmp/sw-accept
# /usr/bin/plutil -lint /tmp/sw-accept/Switcheroo.app/Contents/Info.plist
# /usr/bin/codesign --verify --strict /tmp/sw-accept/Switcheroo.app
# /tmp/sw-accept/Switcheroo.app/Contents/MacOS/switcheroo --version
# ls /tmp/sw-accept/Switcheroo.app/Contents/Resources/AppIcon.icns
```

### Version sync gate
```bash
CARGO_VER=$(/usr/bin/sed -n '/^\[package\]/,/^\[/p' Cargo.toml | /usr/bin/grep -m1 '^version' | /usr/bin/sed 's/version = "\(.*\)"/\1/')
PLIST_VER=$(/usr/bin/plutil -extract CFBundleShortVersionString raw -o - bundle/Info.plist)
CHANGELOG_VER=$(/usr/bin/grep -m1 -E '^## \[' CHANGELOG.md | /usr/bin/sed -E 's/^## \[([0-9.]+)\].*/\1/')
test "$CARGO_VER" = "$PLIST_VER" && test "$CARGO_VER" = "$CHANGELOG_VER" && echo "version sync OK"
# Binary version can be checked with:
# ./scripts/version-sync.sh --binary <path-to-built-binary>
```

### Homebrew formula rehearsal (if `brew` is available)

**REHEARSAL ONLY** — these artifacts are not for publication. The
rehearsal uses a local `file://` archive with a checksum computed from
that exact file. The production formula must use the public GitHub
archive URL with a checksum downloaded from the actual public archive
bytes (see step 9 in "Later Authorized Actions" below).

```bash
# Build the source archive (rehearsal mode):
./scripts/package.sh --out-parent ./dist-parent --allow-native-fallback
# Find the unique output directory:
OUT_DIR=$(ls -d ./dist-parent/switcheroo-out.* | tail -1)
# Render the formula using the local archive (rehearsal mode):
packaging/homebrew/render_formula.sh \
  --version 0.1.0 \
  --local-archive "$OUT_DIR/switcheroo-v0.1.0-source-rehearsal.tar.gz" \
  --rehearsal
brew style packaging/homebrew/switcheroo.rb

# For a full local install/test rehearsal, create a temporary tap:
TAP_DIR=$(mktemp -d)
mkdir -p "$TAP_DIR/Formula"
cp packaging/homebrew/switcheroo.rb "$TAP_DIR/Formula/switcheroo.rb"
cd "$TAP_DIR" && git init --quiet && git add -A && git commit -m test --quiet
cd -
brew tap local/sw-test "$TAP_DIR"
brew audit --strict --online local/sw-test/switcheroo
brew install --build-from-source local/sw-test/switcheroo
brew test local/sw-test/switcheroo
brew uninstall local/sw-test/switcheroo
brew untap local/sw-test
rm -rf "$TAP_DIR" packaging/homebrew/switcheroo.rb
```

### Raycast extension gates
```bash
cd raycast-extension
npm ci
npm run lint
npx tsc --noEmit
npm run build
node --test src/lib/service.test.mjs
npm audit
cd ..
```

## Release Workflow (GitHub Actions)

Once the version bump commit is merged to `main` and the tag exists:

```bash
gh workflow run release.yml -f tag=vX.Y.Z --ref main
```

The workflow:
1. Asserts dispatch from `main`.
2. Validates the tag (semver, exists, merged to main, Cargo.toml +
   Info.plist + CHANGELOG.md + rust-toolchain.toml version match).
3. Passes the validated 40-hex commit SHA to all downstream jobs.
4. Runs `fmt`, `clippy`, `test` at the tag commit SHA.
5. Calls `scripts/package.sh --tag vX.Y.Z --commit-sha <SHA>` in release
   mode: asserts HEAD == SHA, requires completely clean worktree, exports
   commit into a private source tree, builds hermetically under `env -i`
   with private CARGO_HOME and `--frozen`, archives from the exact SHA.
6. Uploads `private-validation-artifacts` (7-day retention).

**No GitHub Release is created.** Maintainers download validation artifacts
from the Actions run for verification only.

### Publication Allowlist

**The following may be attached to a public GitHub Release:**
- Release notes (text)
- The git tag itself (immutable, signed if tag ruleset configured)

**The following must NOT be attached to a public GitHub Release:**
- `switcheroo-vX.Y.Z-macos-universal.tar.gz` (prebuilt binary — unnotarized,
  unauthenticated; do not publish until notarized or independently
  signed/attested)
- `checksums-sha256.txt` (co-hosted with binary — not an authenticity anchor)
- `release-manifest.txt` (unsigned — integrity only, not authenticity)

**The only public distribution path is the Homebrew source-build Formula.**
The Formula's checksum must be computed from the exact public GitHub tag
archive URL bytes (`https://github.com/mitchelljphayes/switcheroo/archive/refs/tags/vX.Y.Z.tar.gz`),
downloaded after the tag is public and immutable.

## Later Authorized Actions

These are **NOT** done by the packaging layer or the release workflow.
They require explicit user authorization and external GitHub settings:

1. **Commit** the packaging layer (and the wake layer, separately or
   together — user decides).
2. **Push** the commit to a remote branch.
3. **Open a PR** to `main`.
4. **Make the GitHub repo public** (if not already).
5. **Configure GitHub protections:** `release` environment, env-scoped
   `RELEASE_AUTHORIZATION` secret, `main` branch protection, immutable
   `v*` tag ruleset, Actions SHA-pin policy, resolution of the
   credential incident (the 6 `RELEASE_SETUP.md` prerequisites).
6. **Create the immutable `vX.Y.Z` tag** at the merged commit.
7. **Dispatch `release.yml`** with `tag=vX.Y.Z` from `main`; download
   the build artifacts.
8. **Create the GitHub Release** manually — attach release notes only.
   Do NOT attach the binary archive, checksums, or manifest (see
   Publication Allowlist above). The prebuilt binary must not be publicly
   distributed until notarized or independently signed/attested.
9. **Compute the real SHA-256** of the public source archive; render
   `packaging/homebrew/switcheroo.rb` with the real checksum via
   `packaging/homebrew/render_formula.sh`.
10. **Create the tap repo** `mitchelljphayes/homebrew-switcheroo` on
    GitHub; push `Formula/switcheroo.rb` (note: `Formula/`, not
    `Formula/homebrew/`).
11. **Verify** `brew tap mitchelljphayes/switcheroo && brew install
    switcheroo` end-to-end on a clean account.

## Known Gaps

- **No automated publication** — by design until `RELEASE_SETUP.md`
  prerequisites are met.
- **No attestation/SBOM** — deferred per approved defaults
  (`id-token: write` / `attestations: write` not added in this pass).
- **Rust advisory scanning** — CI uses a SHA-pinned `cargo audit` action.
  The scanner database is network-fetched and may not cover CVSS 4.0
  advisories on all local toolchain versions; failures are fail-closed.
- **Ad-hoc signing only** — Accessibility permission may need
  re-granting after each rebuild.
- **Homebrew Rust build dependency** — The Formula uses Homebrew's `rust`
  Formula (not the repo's pinned `rust-toolchain.toml`). This is
  intentional: Homebrew bottles are platform/toolchain-specific and
  Homebrew manages its own compiler versions. The repo pin ensures CI
  and local release builds use a specific reviewed toolchain.
- **Prebuilt binary not for public distribution** — Until notarization
  or independent signing/attestation is added, the binary archive is a
  CI validation artifact only. The sole public path is the Homebrew
  source-build Formula.