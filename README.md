# keytap

Lightweight macOS keyboard remapper using `CGEventTap`. No kernel extensions, no DriverKit, no Karabiner dependency.

## What it does

keytap intercepts keyboard events at the Quartz event level and applies remapping rules defined in a TOML config. It handles the stateful remaps that macOS can't do natively (conditional modifier remaps, chords).

**For simple modifier swaps** (e.g., Caps Lock → Ctrl), use macOS System Settings → Keyboard → Modifier Keys. That's kernel-level and rock solid. keytap handles everything else.

## Default config

```toml
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
2. Install to `/usr/local/bin/keytap`
3. Copy config to `~/.config/keytap/config.toml`
4. Install and start a LaunchAgent

**Important**: Grant Accessibility access after installing:
- System Settings → Privacy & Security → Accessibility
- Add `/usr/local/bin/keytap`

## Usage

```bash
# Run directly (for testing)
keytap                              # uses ~/.config/keytap/config.toml
keytap /path/to/config.toml        # explicit config path

# With debug logging
RUST_LOG=debug keytap

# As a service (managed by install.sh)
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.local.keytap.plist
launchctl bootout gui/$(id -u)/com.local.keytap
tail -f /tmp/keytap.log
```

## Uninstall

```bash
./uninstall.sh
```

## Config reference

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

1. Registers a `CGEventTap` at `kCGHIDEventTap` (earliest interception point in userspace)
2. Receives `keyDown`, `keyUp`, and `flagsChanged` events
3. Runs them through the remap engine
4. Returns modified events (or suppresses them)

This is the same mechanism used by macOS accessibility tools, screenshot apps, and remote desktop software. It requires Accessibility permission but no special entitlements, kernel extensions, or virtual HID devices.

## Why not Karabiner/kanata?

Both depend on `Karabiner-DriverKit-VirtualHIDDevice`, which:
- Requires a DriverKit system extension
- Has recurring permission issues on macOS updates
- Was broken in macOS 26.4 beta (internal keyboard stopped working)
- Apple is pushing developers away from DriverKit virtual HID toward CoreHID

keytap uses `CGEventTap`, which has been stable since macOS 10.4 (2005) and is Apple's supported userspace event interception API.

## License

MIT
