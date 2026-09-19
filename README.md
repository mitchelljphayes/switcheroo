# Switcheroo 🦘

Lightweight macOS keyboard remapper using `CGEventTap`. No kernel extensions, no DriverKit, no Karabiner dependency.

Switcheroo has two parts:

- **The daemon** performs the keyboard remapping. Install it with Homebrew and run it as a background service.
- **The optional Raycast extension** lets you view and edit remaps. It does not install or replace the daemon. [Store submission is awaiting review](https://github.com/raycast/extensions/pull/31064).

You can use Switcheroo without Raycast by editing `~/.config/switcheroo/config.toml` directly. It supports modifier remaps, key swaps, tap-hold, conditional remaps, and chords. Modifier mappings are reapplied at startup and after wake; they do not persist across reboot by themselves.

## Install

### Homebrew quick start (recommended)

Requires macOS and [Homebrew](https://brew.sh/). Homebrew builds the daemon from source, so the first installation may take a few minutes. No Apple Developer account is needed.

**Already using a standalone installation?** Follow the [migration instructions](https://github.com/mitchelljphayes/homebrew-switcheroo#migrating-from-the-standalone-install-caution) before starting the Homebrew service. Do not run both providers at once; `brew services stop` does not stop a standalone LaunchAgent.

#### 1. Install the daemon

```bash
brew tap mitchelljphayes/switcheroo
brew install switcheroo
```

Do not start the service until you have configured it and granted Accessibility permission.

#### 2. Create your config

Copy the sample only if you do not already have a config file or symlink:

```bash
mkdir -p "$HOME/.config/switcheroo"
config="$HOME/.config/switcheroo/config.toml"
if [ ! -e "$config" ] && [ ! -L "$config" ]; then
  cp "$(brew --prefix)/etc/switcheroo/config.toml" "$config"
fi
open -e "$config"
```

Review the mappings before starting the daemon: the sample changes keyboard behavior. See the [sample config](config.toml) and [config reference](#config-reference). An existing config is left untouched.

#### 3. Grant Accessibility permission

1. Print the installed app path:

   ```bash
   echo "$(brew --prefix switcheroo)/Switcheroo.app"
   ```

2. Open **System Settings → Privacy & Security → Accessibility**.
3. Click **+**, use **⌘⇧G** to navigate to the printed path, and add `Switcheroo.app`.
4. Enable its permission. Grant access to **Switcheroo**, not just Raycast.

The locally built app is ad-hoc signed. After an upgrade or rebuild, you may need to remove and re-add its Accessibility entry if remapping stops working.

#### 4. Start and check the service

```bash
brew services start switcheroo
brew services info switcheroo
switcheroo --version
```

The service runs in the background and starts at login. Test a mapping from your config. If it does not work, check [troubleshooting](#troubleshooting) below.

#### 5. Optional: add the Raycast UI

Homebrew does **not** install the Raycast extension. See [Raycast Extension](#raycast-extension) for its current availability and local installation instructions.

## Usage

For a Homebrew installation:

```bash
brew services info switcheroo     # check service status
brew services restart switcheroo  # apply manually edited config
brew services stop switcheroo     # stop remapping and disable start-at-login
brew services start switcheroo    # start again and enable start-at-login
```

Keep the daemon running while using remaps. A normal shutdown removes Switcheroo-owned modifier mappings and restores the previous mappings where safe.

### Raycast Extension

**Store status:** the extension is [submitted for review](https://github.com/raycast/extensions/pull/31064), not yet available as a Store install. You can use the daemon now; Raycast is optional.

The extension provides **View Remaps**, **Add Remap**, **Restart Switcheroo**, **View Logs**, and **Edit Config**. For a local installation while review is pending, follow the [extension guide](raycast-extension/README.md#local-install-before-store-approval). The latest Store candidate is on the submission branch, which may differ from the extension source in this repository.

After the initial local build/import, stop `npm run dev` with **Ctrl+C**. The built commands remain available; the watcher is only needed while developing. Stopping it does not stop the remapping daemon.

Once the Store version is approved, install it from Raycast and remove any duplicate local development copy in **Raycast Settings → Extensions**.

### Upgrade

```bash
brew update
brew upgrade switcheroo
brew services restart switcheroo
```

Recheck Accessibility permission if needed. Your active config is separate from Homebrew's installed files; upgrades do not replace it.

## Troubleshooting

- **Installed, but keys are unchanged:** check `brew services info switcheroo`, the active config path, and Accessibility permission for the Homebrew app path printed above.
- **Changed the TOML manually:** restart the service to load it. In the submitted Raycast extension, add/edit/delete actions attempt a restart after saving and report restart failures separately.
- **Raycast says “Missing executable”:** this is an extension build/import error, not necessarily a missing daemon. Rebuild the local extension using its [local-install instructions](raycast-extension/README.md#local-install-before-store-approval).
- **Two sets of Raycast commands:** stop extra `npm run dev` watchers and check for duplicate local extension registrations before removing anything.
- **Old standalone install detected:** use the [migration guide](https://github.com/mitchelljphayes/homebrew-switcheroo#migrating-from-the-standalone-install-caution). Do not run a second daemon or use blanket `hidutil` clearing commands.

Daemon logs are under `~/Library/Logs/com.mitchelljphayes.switcheroo/`:

```bash
tail -f "$HOME/Library/Logs/com.mitchelljphayes.switcheroo/daemon.err"
```

For Homebrew launchd stdout/stderr paths and more service commands, see the [tap README](https://github.com/mitchelljphayes/homebrew-switcheroo#readme).

## Uninstall

### Homebrew installation

Stop the service before removing the executable:

```bash
brew services stop switcheroo
brew uninstall switcheroo
brew untap mitchelljphayes/switcheroo
```

Keep `~/.config/switcheroo/config.toml` if you may reinstall. If the daemon was hard-killed or cleanup reports an error, seek recovery guidance before deleting its binary or recovery state; a normal uninstall is not a guarantee of crash recovery.

### Standalone installation

From the source checkout used to build the current standalone version:

```bash
cargo build --release --locked
./uninstall.sh
```

The build supplies the recovery helper required by the uninstaller. It handles the current and legacy standalone labels and preserves your config. Do not use `uninstall.sh` to remove a Homebrew installation.

## Build from source (standalone alternative)

Use this instead of Homebrew, not alongside it. Requires Git, Rust via [rustup](https://rustup.rs/), and the Xcode Command Line Tools. Install the latter with `xcode-select --install` if needed.

```bash
git clone --branch v0.1.1 --depth 1 https://github.com/mitchelljphayes/switcheroo.git
cd switcheroo
./install.sh
```

This will:
1. Build the release binary with `cargo`
2. Stage + ad-hoc sign the `.app` bundle and atomically swap it into `~/.local/bin/Switcheroo.app`
3. Create a sample config at `~/.config/switcheroo/config.toml` if one does not exist
4. Install and start a LaunchAgent, migrating from the old `com.local.switcheroo` label if present

The standalone installer:
- Stop any existing Switcheroo agent before overwriting the bundle
- Validate `~`, paths, and plist ownership/permissions (rejecting hostile symlinks)
- Migrate the old `com.local.switcheroo` label safely (only if its plist points at Switcheroo)
- Verify the agent is registered after bootstrap, rolling back on failure

**Standalone permission path:** the installer starts its LaunchAgent; after first install, grant Accessibility access:
- System Settings → Privacy & Security → Accessibility
- Add `~/.local/bin/Switcheroo.app`

> **Bundle-id migration (v0.1.x):** The LaunchAgent identity changed from
> `com.local.switcheroo` to `com.mitchelljphayes.switcheroo`. An existing
> Accessibility grant is tied to the old bundle id and must be **re-issued
> once** after upgrading. `install.sh` detects and cleanly stops the old
> label; `uninstall.sh` cleans up both. Unrelated `hidutil` mappings are
> preserved across the migration.

The source installer rejects some symlinked installation/config paths. If you manage config through dotfiles symlinks, prefer the Homebrew path rather than removing or overwriting those symlinks to satisfy the installer.

### Prebuilt archives

[Public releases](https://github.com/mitchelljphayes/switcheroo/releases) currently distribute source. Prebuilt app archives produced by CI are validation artifacts, **not a supported public install path**. Ad-hoc signatures and co-hosted checksums do not authenticate the publisher; signed or attested binary distribution is separate future work.

## Config reference

### `[[modifier_remap]]`

Kernel-level key remap applied via `hidutil` on startup, before the event tap processes keys. Mappings are automatically re-applied about two seconds after wake. They are not inherently persistent across reboot: the login service reapplies them. Normal shutdown removes owned mappings and restores prior mappings where safe. If wake reapplication fails, a warning is logged and the daemon keeps running.

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
# Map semicolon to equal (; to =)
[[remap]]
from = "semicolon"
to = "equal"

# Remap Caps Lock to Escape (alternative to modifier_remap if you
# want it handled in userspace rather than at the kernel level)
[[remap]]
from = "caps_lock"
to = "escape"
```

> **`[[remap]]` vs `[[modifier_remap]]`**: Use `modifier_remap` for modifier changes such as Caps Lock → Ctrl at the HID level. Use `remap` for unconditional key changes handled by the event tap. Keep Switcheroo running for normal operation; stopping it normally also cleans up its owned HID mappings.

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

The Homebrew formula copies the tracked `bundle/AppIcon.icns` into the app bundle. It does **not** run `iconutil` inside Homebrew's build sandbox; that was fixed in v0.1.1.

Maintainers can regenerate the asset from `bundle/AppIcon-1024.png` with `scripts/generate_icns.sh` outside the sandbox. The Raycast extension uses the separate 512×512 `raycast-extension/assets/command-icon.png` asset.

## License

MIT
