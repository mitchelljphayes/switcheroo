# Rebind

Lightweight macOS keyboard remapper using `CGEventTap`. No kernel extensions, no DriverKit, no Karabiner dependency.

## What it does

Rebind intercepts keyboard events at the Quartz event level and applies remapping rules defined in a TOML config. It handles the stateful remaps that macOS can't do natively (conditional modifier remaps, tap-hold, chords).

Rebind also applies kernel-level modifier remaps via `hidutil` on startup, so settings like Caps Lock → Ctrl persist across reboots without depending on System Settings.

## Default config

```toml
# Kernel-level modifier remaps (applied via hidutil on startup)
[[modifier_remap]]
from = "caps_lock"
to = "left_ctrl"

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

```bash
./install.sh
```

This will:
1. Build the release binary
2. Install app bundle to `~/.local/bin/Rebind.app`
3. Code sign with your local certificate (preserves Accessibility permission across rebuilds)
4. Copy config to `~/.config/rebind/config.toml`
5. Install and start a LaunchAgent

**Important**: Grant Accessibility access after first install:
- System Settings → Privacy & Security → Accessibility
- Add `~/.local/bin/Rebind.app`

## Usage

```bash
# Run directly (for testing)
rebind                              # uses ~/.config/rebind/config.toml
rebind /path/to/config.toml        # explicit config path

# With debug logging
RUST_LOG=debug rebind

# As a service (managed by install.sh)
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.local.rebind.plist
launchctl bootout gui/$(id -u)/com.local.rebind
tail -f /tmp/rebind.err
```

## Raycast Extension

A Raycast extension is included for managing the config via UI:

```bash
cd raycast-extension && npm install && npm run dev
```

Commands: View Remaps, Add Remap, Restart Rebind, View Logs, Edit Config.

## Uninstall

```bash
./uninstall.sh
```

## Config reference

### `[[modifier_remap]]`

Kernel-level key remap applied via `hidutil` on startup. Equivalent to System Settings → Keyboard → Modifier Keys, but persistent.

| Field | Values |
|-------|--------|
| `from` | Any key name (see below) |
| `to` | Any key name (see below) |

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

1. Applies `[[modifier_remap]]` rules via `hidutil` (kernel-level, instant)
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

Rebind uses `CGEventTap`, which has been stable since macOS 10.4 (2005) and is Apple's supported userspace event interception API. For kernel-level modifier remaps, it uses `hidutil`, which has been stable since macOS 10.12.

## License

MIT
