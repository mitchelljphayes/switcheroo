# Changelog

All notable changes to Switcheroo are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-09-02

### Added

- First public tagged release.
- CGEventTap-based keyboard remapper daemon for macOS.
- Kernel-level modifier remaps via `hidutil` (applied on startup and
  re-applied ~2 seconds after wake from sleep via an IOKit power
  notification; debounced to avoid race conditions with panic/signal
  cleanup).
- Simple key remaps (`[[remap]]`), conditional remaps
  (`[[conditional_remap]]`), tap-hold (`[[tap_hold]]`), and chords
  (`[[chord]]`).
- Standalone installer (`install.sh`) with transactional swap,
  backup/rollback, and safe migration from the old
  `com.local.switcheroo` label.
- Binary installer (`install-binary.sh`) for prebuilt universal `.app`
  bundles with bundle-id, version, and signature verification.
- Idempotent uninstaller (`uninstall.sh`) that preserves unrelated
  `hidutil` mappings and user config.
- Raycast extension for managing config via UI (View Remaps, Add Remap,
  Restart Switcheroo, View Logs, Edit Config).
- Homebrew tap install path (`brew tap mitchelljphayes/switcheroo &&
  brew install switcheroo`) with a source-building Formula template.
- Ad-hoc universal binary archive for prebuilt distribution
  (secondary path; Gatekeeper quarantine applies).
- Deterministic packaging script (`scripts/package.sh`) producing
  source + binary archives with SHA-256 checksums.
- Version synchronization validation across `Cargo.toml`,
  `bundle/Info.plist`, `CHANGELOG.md`, and built binary.
- Read-only GitHub Actions release build pipeline with immutable tag
  validation, version-matched checks, and SHA-pinned actions.
- CI hardening with `--locked` clippy/test, all-targets/all-features
  build gates, shellcheck, and packaging dry-run.

### Security

- All system tools in packaging scripts invoked by absolute path with
  fixed minimal `PATH`.
- Safe directory validation rejects symlinks, group/world-writable
  paths, and wrong-owner directories in installer scripts.
- Transactional app/plist swap with rollback on failure.
- Foreign job collision refusal: bootout/bootstrap only proceeds when
  the loaded job's `program =` exactly matches the expected Switcheroo
  executable.
- `hidutil` mappings snapshotted and restored around legacy daemon
  shutdown to preserve foreign (non-Switcheroo) mappings.
- Raycast extension uses absolute `execFileSync` tool arrays, `lstat`
  (not `stat`) to detect symlinked plists, exact Label +
  ProgramArguments[0] + owner validation, and exact (not prefix-based)
  Homebrew executable path matching against two official paths only.
  `launchctl kickstart -k` for Homebrew is guarded by a re-verification
  of the loaded job identity immediately before invocation.
- Release artifacts are bound to a validated 40-hex commit SHA: the
  workflow passes the SHA through all jobs, `package.sh` asserts
  HEAD == SHA, requires a completely clean worktree (tracked + untracked),
  exports the commit into a fresh private source tree, and builds
  hermetically under `env -i` with a validated toolchain. The source
  archive is created via `git archive <COMMIT_SHA>` (the exact validated
  commit SHA), not the exported build tree (which is used for compilation
  only). The manifest is deterministic
  (commit timestamp, cargo/rustc/toolchain/script SHA — no wall clock).
  Rehearsal artifacts are clearly named `-rehearsal` and never look like
  release artifacts.
- Formula renderer constructs the production URL internally from a
  strict-semver version — no arbitrary URL input accepted. SHA-256
  must be exactly 64 lowercase hex. Rehearsal mode copies the local
  archive to a script-created safe filename and derives the file:// URL
  and checksum from that copy — the caller-supplied path never enters
  the sed expression directly.
- Packaging output uses fresh-private `mktemp` directories with `umask 077`.
  Ancestor paths are validated (no symlinks, correct owner, not
  group/world-writable). Payload inputs are validated as regular
  non-symlink files. No caller-controlled paths are overwritten or removed.
- GitHub Actions workflows use `contents: read` only — no write scopes,
  no publication, no attestations in this pass.

### Known Limitations

- Ad-hoc signing only (no Apple Developer ID, no notarization).
  Ad-hoc signing provides self-consistency only — it does NOT
  authenticate the publisher. Checksums provide integrity (corruption
  detection), not authenticity (publisher proof). The prebuilt binary
  archive is a CI/rehearsal output, not a public distribution option,
  until a signed manifest or trusted attestation is added. The primary
  public path is the Homebrew source-build (Option 0).
- Accessibility permission may need re-granting after each rebuild
  (ad-hoc signature changes). The "Switcheroo Dev" self-signed cert
  path in `install.sh` mitigates this on the developer's own machine.
- No automated GitHub Release publication — maintainers download
  build artifacts and create releases manually until
  `RELEASE_SETUP.md` prerequisites are met.