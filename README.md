# Switcheroo 🦘

Lightweight macOS keyboard remapper using `CGEventTap`. No kernel extensions, no DriverKit, no Karabiner dependency.

## What it does

Switcheroo intercepts keyboard events at the Quartz event level and applies remapping rules defined in a TOML config. It handles the stateful remaps that macOS can't do natively (conditional modifier remaps, tap-hold, chords).

Switcheroo also applies kernel-level modifier remaps via `hidutil` on startup, so settings like Caps Lock → Ctrl persist across reboots without depending on System Settings.

## Default config

```toml
# Kernel-level modifier remaps (applied via hidutil on startup)
[[modifier_remap]]
from = "caps_lock"
to = "left_ctrl"

# Simple key remaps (unconditional, applied via CGEventTap)
# [[remap]]
# from = "a"
# to = "b"

# Ctrl + HJKL → Arrow keys (vim-style navigation everywhere)
[[conditional_remap]]
modifier = "ctrl"
from = "h"
to = "left_arrow"

[[conditional_remap]]
modifier = "ctrl"
from = "j"
to = "down_arrow"

[[conditional_remap]]
modifier = "ctrl"
from = "k"
to = "up_arrow"

[[conditional_remap]]
modifier = "ctrl"
from = "l"
to = "right_arrow"

# Both shifts pressed together → Caps Lock
[[chord]]
keys = ["left_shift", "right_shift"]
emit = "caps_lock"
window_ms = 100
```

## Install

### Option 0 — Homebrew (recommended)

```bash
brew tap mitchelljphayes/switcheroo
brew install switcheroo
brew services start switcheroo
```

Homebrew builds from source via `cargo --release --locked`, so the
binary is compiled locally and ad-hoc signed on your machine — no
Developer ID, no notarization, no Gatekeeper friction. Grant
Accessibility permission after first install (see below).

> **Note:** ad-hoc signing means Accessibility permission may need
> re-granting after each `brew upgrade` (the binary is recompiled with
> a new signature). A sample config is installed at
> `$(brew --prefix)/etc/switcheroo/config.toml`; create or symlink your
> active config at `~/.config/switcheroo/config.toml`.

### Option A — Prebuilt binary archive (not yet a public distribution path)

> **Not yet available for public distribution.** Prebuilt binary archives
> are produced by CI for testing and rehearsal only. They are **unnotarized**
> and signed with an **ad-hoc signature** (self-consistency only — this does
> NOT authenticate the publisher). A co-hosted checksum file provides
> **integrity** (detection of accidental corruption), not **authenticity**
> (proof of publisher identity). Until a signed manifest or trusted
> attestation is added, the **Homebrew source-build path (Option 0) is the
> only public distribution method**. The binary archive will become a public
> option only when artifact authenticity is implemented (signed tag +
> attested manifest bound to the immutable commit).

### Option B — Build from source (fallback)

```bash
./install.sh
```

This will:
1. Build the release binary with `cargo`
2. Stage + ad-hoc sign the `.app` bundle and atomically swap it into `~/.local/bin/Switcheroo.app`
3. Copy config to `~/.config/switcheroo/config.toml`
4. Install and start a LaunchAgent, migrating from the old `com.local.switcheroo` label if present

Both installers:
- Stop any existing Switcheroo agent before overwriting the bundle
- Validate `~`, paths, and plist ownership/permissions (rejecting hostile symlinks)
- Migrate the old `com.local.switcheroo` label safely (only if its plist points at Switcheroo)
- Verify the agent is registered after bootstrap, rolling back on failure

**Important**: Grant Accessibility access after first install:
- System Settings → Privacy & Security → Accessibility
- Add `~/.local/bin/Switcheroo.app`

> **Bundle-id migration (v0.1.x):** The LaunchAgent identity changed from
> `com.local.switcheroo` to `com.mitchelljphayes.switcheroo`. An existing
> Accessibility grant is tied to the old bundle id and must be **re-issued
> once** after upgrading. `install.sh` detects and cleanly stops the old
> label; `uninstall.sh` cleans up both. Unrelated `hidutil` mappings are
> preserved across the migration.

## Usage

```bash
# Run directly (for testing)
switcheroo                              # uses ~/.config/switcheroo/config.toml
switcheroo /path/to/config.toml        # explicit config path

# With debug logging
RUST_LOG=debug switcheroo

# As a service (managed by install.sh)
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.mitchelljphayes.switcheroo.plist
launchctl bootout gui/$(id -u)/com.mitchelljphayes.switcheroo
tail -f ~/Library/Logs/com.mitchelljphayes.switcheroo/daemon.err
```

## Raycast Extension

A Raycast extension is included for managing the config via UI:

```bash
cd raycast-extension && npm install && npm run dev
```

Commands: View Remaps, Add Remap, Restart Switcheroo, View Logs, Edit Config.

## Uninstall

```bash
./uninstall.sh
```

## Config reference

### `[[modifier_remap]]`

Kernel-level key remap applied via `hidutil` on startup. Equivalent to System Settings → Keyboard → Modifier Keys, but persistent. These are applied at the HID driver level (before any event tap sees them) and survive app restarts. Mappings are automatically re-applied ~2 seconds after the system wakes from sleep (via an IOKit power notification); if reapplication fails, a warning is logged and the daemon keeps running.

| Field | Values |
|-------|--------|
| `from` | Any key name (see below) |
| `to` | Any key name (see below) |

### `[[remap]]`

Simple unconditional key remap. Every press of `from` becomes `to`, regardless of which modifiers are held. Applied at the `CGEventTap` level (userspace), so these require Switcheroo to be running.

Use this for straightforward key swaps that aren't modifier-specific.

| Field | Values |
|-------|--------|
| `from` | Any key name (see below) |
| `to` | Any key name (see below) |

**Examples:**

```toml
# Swap semicolon and colon (remap ; to =)
[[remap]]
from = "semicolon"
to = "equal"

# Remap Caps Lock to Escape (alternative to modifier_remap if you
# want it handled in userspace rather than at the kernel level)
[[remap]]
from = "caps_lock"
to = "escape"
```

> **`[[remap]]` vs `[[modifier_remap]]`**: Use `modifier_remap` for modifier key swaps (e.g. Caps Lock → Ctrl) — it's applied at the kernel level via `hidutil`, so it works even if Switcheroo isn't running. Use `remap` for everything else, or when you want remaps that can be toggled by stopping/starting Switcheroo.

### `[[tap_hold]]`

Tap a key for one action, hold it for another.

| Field | Description |
|-------|-------------|
| `key` | The key to intercept |
| `tap` | Key to emit on quick press+release |
| `hold` | Key to emit when held with other keys |
| `timeout_ms` | Time window in ms (default: 200) |

### `[[conditional_remap]]`

Remap a key when a modifier is held. The modifier is stripped from the output event.

| Field | Values |
|-------|--------|
| `modifier` | `ctrl`, `shift`, `option`/`alt`, `cmd`/`command` |
| `from` | Any key name (see below) |
| `to` | Any key name (see below) |

### `[[chord]]`

Emit a key when multiple keys are pressed simultaneously.

| Field | Description |
|-------|-------------|
| `keys` | Array of key names that must be pressed together |
| `emit` | Key to emit when chord triggers |
| `window_ms` | Time window in ms for chord detection (default: 100) |

### Key names

Letters: `a`-`z`  
Arrows: `left_arrow`, `right_arrow`, `up_arrow`, `down_arrow`  
Modifiers: `left_shift`, `right_shift`, `left_ctrl`, `right_ctrl`, `left_option`, `right_option`, `left_cmd`, `right_cmd`, `caps_lock`  
Special: `escape`, `tab`, `space`, `return`, `delete`, `forward_delete`  
Function: `f1`-`f12`

## How it works

1. Applies `[[modifier_remap]]` rules via `hidutil` (kernel-level, instant) — on startup **and on wake from sleep** (via an IOKit power notification; debounced ~2 s)
2. Registers a `CGEventTap` at `kCGHIDEventTap` (earliest interception point in userspace)
3. Receives `keyDown`, `keyUp`, and `flagsChanged` events
4. Runs them through the remap engine (tap-hold, conditional remaps, chords)
5. Returns modified events (or suppresses them)

This is the same mechanism used by macOS accessibility tools, screenshot apps, and remote desktop software. It requires Accessibility permission but no special entitlements, kernel extensions, or virtual HID devices.

## Why not Karabiner/kanata?

Both depend on `Karabiner-DriverKit-VirtualHIDDevice`, which:
- Requires a DriverKit system extension
- Has recurring permission issues on macOS updates
- Was broken in macOS 26.4 beta (internal keyboard stopped working)
- Apple is pushing developers away from DriverKit virtual HID toward CoreHID

Switcheroo uses `CGEventTap`, which has been stable since macOS 10.4 (2005) and is Apple's supported userspace event interception API. For kernel-level modifier remaps, it uses `hidutil`, which has been stable since macOS 10.12.

## Icons

The app bundle icon (`AppIcon.icns`) is generated from the tracked
master `bundle/AppIcon-1024.png` (1024×1024). The packaging script
(`scripts/package.sh`) produces all required sizes via `sips` +
`iconutil` at build time. The Raycast extension icon
(`raycast-extension/assets/command-icon.png`) is a 512×512 derivative
for Raycast Store requirements.

## License

MIT
