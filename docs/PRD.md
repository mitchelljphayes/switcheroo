# Switcheroo — Simple Key Remapping

## Overview

A minimal, reliable key remapper that doesn't require kernel extensions, DriverKit, or system reboots. A tiny Rust daemon reads a YAML config and intercepts keyboard events at the OS level. A Raycast extension provides the UI for configuration and management. macOS first, with a cross-platform architecture that can extend to Linux and Windows.

## Problem

Key remapping on macOS is unreasonably difficult:

- **Karabiner-Elements** is the de facto standard but requires a DriverKit system extension, multiple privacy permission grants, reboots after install, reboots after macOS updates, and regularly breaks on new macOS versions. It configures via JSON. The UX is hostile to non-technical users.
- **Kanata** has a niche config language (s-expressions) and depends on Karabiner's DriverKit driver on macOS — so when Karabiner breaks, Kanata breaks too.
- **Hammerspoon** works via CGEventTap but is a general-purpose Lua scripting environment, not a key remapper. No GUI, no config format — you write code.
- **macOS System Settings** can only do single-key modifier remaps (e.g., Caps Lock → Control). No tap/hold, no key combos, no layers.

As of macOS 26 (Tahoe), the Karabiner DriverKit driver is broken on beta releases, leaving Hammerspoon as the only working option. This is a recurring pattern with every major macOS release.

The most common remapping needs are simple:
1. Caps Lock → Escape on tap, Control on hold
2. Vim-style arrow keys (Ctrl+HJKL)
3. Modifier key swaps

These should not require a kernel driver, 6 permission dialogs, and 2 reboots.

## Solution

Two components:

### 1. `switcheroo` daemon (Rust)

A lightweight background process that:
- Reads a YAML config file from `~/.config/switcheroo/config.yaml`
- Intercepts and remaps keyboard events using OS-native APIs
  - **macOS**: CGEventTap (userspace, no driver)
  - **Linux** (future): evdev + uinput
  - **Windows** (future): Low-level keyboard hook (LLHOOK)
- Runs as a background service (launchd on macOS, systemd on Linux)
- Requires only Accessibility permission on macOS (one toggle in System Settings)
- Has no UI, no menu bar icon, no dock presence
- Watches the config file for changes and hot-reloads
- Cross-platform Rust core with platform-specific input backends

### 2. Raycast extension

A Raycast extension that provides:
- **Status** — is the daemon running? what config is active?
- **Quick toggles** — enable/disable specific remaps without editing YAML
- **Preset library** — common remaps users can enable with one click
- **Config editor** — open config in $EDITOR or edit inline
- **Daemon control** — start, stop, restart, reload

## User Stories

### First-time setup
> As a new user, I want to install switcheroo and have Caps Lock → Escape working in under 2 minutes, so I don't waste time fighting macOS permissions.

1. `brew install switcheroo`
2. Grant Accessibility permission when prompted (one dialog)
3. Open Raycast → "Switcheroo" → Enable "Caps to Ctrl/Esc" preset
4. Done. Survives reboots, sleep, macOS updates.

### Power user config
> As a developer, I want to define my remaps in a YAML file and have them hot-reload when I save, so I can iterate quickly.

Edit `~/.config/switcheroo/config.yaml`, save, remaps update instantly.

### Non-technical user
> As a non-technical professional, I want to remap keys without learning a config language, so I can customize my keyboard without help.

Open Raycast → "Switcheroo" → browse presets → toggle on what you want. No config file needed.

### macOS update
> As a user who just updated macOS, I want my key remaps to still work without reinstalling or rebooting, so I'm not disrupted.

CGEventTap is a stable userspace API. No driver to break.

## Config Format

```yaml
# ~/.config/switcheroo/config.yaml

# Tap/hold remaps
- name: Caps to Ctrl/Esc
  from: caps_lock
  tap: escape
  hold: left_control
  hold_threshold_ms: 200

# Modifier + key → key
- name: Vim arrows
  modifier: control
  map:
    h: left
    j: down
    k: up
    l: right

# Simple key swap
- name: Swap Command and Option (left)
  swap: [left_command, left_option]

# Modifier swap
- name: Right Command → Hyper
  from: right_command
  to: [control, option, command, shift]

# App-specific remap (macOS only)
- name: Terminal Ctrl+N → Down
  app: com.mitchellh.ghostty
  modifier: control
  map:
    n: down
    p: up
```

### Key names

Platform-agnostic key names that map to OS-specific keycodes internally:
- Modifiers: `caps_lock`, `left_shift`, `right_shift`, `left_control`, `right_control`, `left_alt`, `right_alt`, `left_meta`, `right_meta`, `fn`
  - Aliases: `left_option`/`right_option` → `left_alt`/`right_alt`, `left_command`/`right_command` → `left_meta`/`right_meta`
- Letters: `a`–`z`
- Numbers: `0`–`9`
- Arrows: `left`, `right`, `up`, `down`
- Special: `escape`, `return`, `tab`, `space`, `backspace`, `delete`
- Function keys: `f1`–`f20`

## Requirements

### Functional

1. **Tap/hold detection** — distinguish between a quick tap and a held modifier, with configurable threshold
2. **Modifier + key remapping** — remap key combos (e.g., Ctrl+H → Left Arrow)
3. **Simple key swaps** — swap two keys bidirectionally
4. **Modifier remapping** — map one modifier to another or to a combo (Hyper key)
5. **App-specific remaps** — apply remaps only when a specific app is frontmost (macOS only in v1)
6. **Hot reload** — watch config file, apply changes without restart
7. **Modifier passthrough** — Ctrl+Shift+H → Shift+Left (preserve non-remapped modifiers)
8. **Preset library** — ship common remaps as named presets in the Raycast extension

### Non-functional

1. **Latency** — < 1ms added latency per keystroke
2. **Reliability** — survive sleep/wake, screen lock/unlock, display changes without dying
3. **Startup** — daemon starts in < 100ms
4. **Memory** — < 5MB resident memory
5. **CPU** — 0% CPU when idle, negligible during typing
6. **Permissions** — only Accessibility permission required on macOS (no Full Disk Access, no Input Monitoring, no system extensions)
7. **Compatibility** — macOS 14+ (Sonoma and later). Linux and Windows support in future versions.

### Installation

1. Homebrew formula: `brew install switcheroo`
2. Installs the Rust daemon binary to `/usr/local/bin/switcheroo`
3. Installs a launchd plist to `~/Library/LaunchAgents/`
4. On first run, prompts for Accessibility permission
5. Creates default config at `~/.config/switcheroo/config.yaml` if none exists
6. Raycast extension installable from Raycast Store

## Architecture

```
┌─────────────────┐     ┌────────────────────────┐
│  Raycast ext     │────▶│ ~/.config/switcheroo/  │
│  (TypeScript)    │     │   config.yaml          │
└─────────────────┘     └──────────┬─────────────┘
                                   │ file watch
                        ┌──────────▼─────────────┐
                        │  switcheroo daemon      │
                        │  (Rust)                 │
                        │                         │
                        │  ┌───────────────────┐  │
                        │  │ Config (YAML)     │  │
                        │  ├───────────────────┤  │
                        │  │ Remap engine      │  │
                        │  │ (tap/hold state   │  │
                        │  │  machine, combos) │  │
                        │  ├───────────────────┤  │
                        │  │ Platform backend  │  │
                        │  │ ┌───────────────┐ │  │
                        │  │ │macOS:CGEvent  │ │  │
                        │  │ │linux:evdev    │ │  │
                        │  │ │win:LLHOOK     │ │  │
                        │  │ └───────────────┘ │  │
                        │  ├───────────────────┤  │
                        │  │ App watcher       │  │
                        │  │ (NSWorkspace/etc) │  │
                        │  └───────────────────┘  │
                        └─────────────────────────┘
                              ▲           │
                    intercept │           │ emit
                        ┌─────┴───────────▼───────┐
                        │     OS input layer       │
                        └─────────────────────────┘
```

### Rust crate structure

```
switcheroo/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point, daemonization
│   ├── config.rs            # YAML parsing, validation
│   ├── engine.rs            # Platform-agnostic remap logic + state machine
│   ├── keys.rs              # Key name → keycode mapping
│   ├── platform/
│   │   ├── mod.rs           # Platform trait definition
│   │   ├── macos.rs         # CGEventTap backend (core-foundation crate)
│   │   ├── linux.rs         # evdev/uinput backend (future)
│   │   └── windows.rs       # LLHOOK backend (future)
│   └── health.rs            # Eventtap health monitoring, auto-recovery
├── presets/                  # Built-in preset YAML files
│   ├── caps-ctrl-esc.yaml
│   ├── vim-arrows.yaml
│   └── hyper-key.yaml
└── raycast-extension/        # Raycast extension (TypeScript)
```

### Daemon lifecycle

1. Parse config YAML
2. Initialize platform backend (CGEventTap on macOS)
3. Build remap table from config
4. Start file watcher on config
5. Enter event loop
6. On config change: re-parse YAML, rebuild remap table (no restart)
7. Health check: if event tap gets disabled by OS, re-register it

### Daemon management

- `switcheroo start` — start daemon (or enable launchd agent)
- `switcheroo stop` — stop daemon
- `switcheroo reload` — send SIGHUP to reload config
- `switcheroo status` — print running state and active remaps
- `switcheroo validate` — check config syntax without starting
- `switcheroo list-keys` — print all valid key names

## CGEventTap Limitations (Out of Scope)

These are inherent to the CGEventTap API on macOS and cannot be fixed without a DriverKit driver:

1. **Cannot distinguish between keyboards** — all keyboards are treated as one. Device-specific remaps are not possible.
2. **Cannot intercept certain system shortcuts** — e.g., Cmd+Tab, Ctrl+Power, some Touch Bar keys
3. **Cannot remap mouse buttons** — out of scope for v1
4. **Cannot work at the login screen** — CGEventTap requires a user session
5. **Cannot intercept keys before other CGEventTaps** — if another app (e.g., Hammerspoon) also has a tap, ordering is not guaranteed

Note: Linux (evdev) and Windows (Interception driver) backends would not have some of these limitations.

## Open Questions

1. ~~**Name**~~ — **Switcheroo** ✓
2. **Raycast vs standalone menu bar** — Raycast dependency OK, or should there be a fallback standalone UI?
3. **Preset distribution** — ship presets in the Raycast extension, in the daemon, or both?
4. **Multiple configs / profiles** — worth supporting in v1, or just one config file?
5. **Notarization** — required for Homebrew cask distribution. Means we need an Apple Developer account.
6. **Crate name** — `switcheroo` is taken on crates.io (unrelated context-switching lib). Use `switcheroo-keys` or just don't publish to crates.io?

## Milestones

### v0.1 — Proof of concept
- Rust daemon with CGEventTap (via `core-foundation` / `core-graphics` crates)
- Tap/hold detection (caps → ctrl/esc)
- Modifier+key remapping (ctrl+hjkl → arrows)
- YAML config parsing (via `serde` + `serde_yaml`)
- CLI: start, stop, status

### v0.2 — Reliable daily driver
- launchd integration (auto-start)
- File watcher config hot-reload (via `notify` crate)
- Eventtap health monitoring (auto-recover after sleep)
- Homebrew formula
- Simple key swaps
- Modifier passthrough

### v0.3 — Raycast extension
- Show status
- Toggle presets
- Start/stop/reload daemon
- Edit config

### v1.0 — Public release
- App-specific remaps
- Preset library (10+ common remaps)
- Notarized binary
- Documentation site
- Raycast Store listing

### Future
- Linux backend (evdev + uinput)
- Windows backend (LLHOOK or Interception)
- Per-device remaps on platforms that support it

## Prior Art

| Tool | Mechanism | Driver needed | Config format | GUI | Cross-platform |
|------|-----------|---------------|---------------|-----|----------------|
| Karabiner-Elements | DriverKit + IOKit | Yes | JSON | Yes (complex) | No |
| Kanata | Karabiner driver (macOS) / evdev (Linux) | Yes (macOS) | S-expressions | No | Yes |
| Hammerspoon | CGEventTap | No | Lua code | Lua console | No |
| hidutil | IOKit | No | CLI args | No | No |
| BetterTouchTool | CGEventTap + driver | Optional | Proprietary | Yes | No |
| xremap | evdev | No | YAML | No | Linux only |
| **Switcheroo** | **CGEventTap / evdev / LLHOOK** | **No** | **YAML** | **Raycast** | **Planned** |
