# Distribution Plan

## Overview

Distribute Rebind via Homebrew with pre-signed, notarized universal binaries. Users install with `brew install` and never need to re-grant Accessibility permission after upgrades.

## Prerequisites

- [ ] Apple Developer Program enrollment ($99/yr) — [developer.apple.com/programs/enroll](https://developer.apple.com/programs/enroll/)
- [ ] GitHub repo created (e.g., `yourusername/rebind`)

## Phase 1: Apple Developer Account Setup

### 1. Enroll

1. Go to [developer.apple.com/programs/enroll](https://developer.apple.com/programs/enroll/)
2. Sign in with your Apple ID
3. Enroll as **Individual** ($99 USD/year)
4. Identity verification may take a few minutes to 48 hours

### 2. Create signing certificate

1. Go to [developer.apple.com/account/resources/certificates](https://developer.apple.com/account/resources/certificates)
2. Create a **Developer ID Application** certificate
3. Download and install it in your Keychain
4. Note the identity name — format: `Developer ID Application: Your Name (TEAMID)`

### 3. Export credentials for CI

For GitHub Actions, you'll need these stored as repository secrets:

| Secret | Description |
|--------|-------------|
| `APPLE_CERTIFICATE_BASE64` | `.p12` export of the Developer ID Application cert, base64-encoded |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` |
| `APPLE_ID` | Your Apple ID email |
| `APPLE_ID_PASSWORD` | App-specific password (generate at [appleid.apple.com](https://appleid.apple.com/) → Sign-In and Security → App-Specific Passwords) |
| `APPLE_TEAM_ID` | Your 10-character Team ID (visible at [developer.apple.com/account](https://developer.apple.com/account/)) |

To export the certificate:

```bash
# Find your identity
security find-identity -v -p codesigning

# Export to .p12 (you'll set a password)
security export -k ~/Library/Keychains/login.keychain-db \
  -t identities -f pkcs12 -o rebind-cert.p12

# Base64 encode for GitHub secret
base64 -i rebind-cert.p12 | pbcopy
# Paste into GitHub secret APPLE_CERTIFICATE_BASE64
```

## Phase 2: GitHub Actions CI

### Release workflow (`.github/workflows/release.yml`)

Triggered on version tags (`v*`). Should:

1. Build universal binary (arm64 + x86_64):
   ```bash
   cargo build --release --target aarch64-apple-darwin
   cargo build --release --target x86_64-apple-darwin
   lipo -create -output rebind \
     target/aarch64-apple-darwin/release/rebind \
     target/x86_64-apple-darwin/release/rebind
   ```

2. Create app bundle (`Rebind.app/Contents/MacOS/rebind` + `Info.plist`)

3. Sign with Developer ID:
   ```bash
   codesign --force --options runtime --sign "Developer ID Application: Your Name (TEAMID)" Rebind.app
   ```

4. Notarize with Apple:
   ```bash
   # Create zip for notarization
   ditto -c -k --keepParent Rebind.app Rebind.zip

   # Submit
   xcrun notarytool submit Rebind.zip \
     --apple-id "$APPLE_ID" \
     --password "$APPLE_ID_PASSWORD" \
     --team-id "$APPLE_TEAM_ID" \
     --wait

   # Staple the ticket
   xcrun stapler staple Rebind.app
   ```

5. Create release tarball and attach to GitHub release:
   ```bash
   tar -czf rebind-v0.1.0-macos-universal.tar.gz \
     Rebind.app/ config.toml com.local.rebind.plist install.sh
   ```

### What the CI runner needs

- macOS runner (`runs-on: macos-latest`)
- Rust toolchain with both targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`
- Import the signing certificate into a temporary keychain during the build

## Phase 3: Homebrew Tap

### Create the tap repo

Create `yourusername/homebrew-rebind` on GitHub with a single formula:

### Formula (`Formula/rebind.rb`)

```ruby
class Rebind < Formula
  desc "Lightweight macOS keyboard remapper using CGEventTap"
  homepage "https://github.com/yourusername/rebind"
  url "https://github.com/yourusername/rebind/releases/download/v0.1.0/rebind-v0.1.0-macos-universal.tar.gz"
  sha256 "PLACEHOLDER"
  license "MIT"

  def install
    prefix.install "Rebind.app"
    bin.write_exec_script prefix/"Rebind.app/Contents/MacOS/rebind"

    # Install default config if not present
    (etc/"rebind").mkpath
    etc.install "config.toml" => "rebind/config.toml" unless (etc/"rebind/config.toml").exist?
  end

  service do
    run [opt_prefix/"Rebind.app/Contents/MacOS/rebind"]
    keep_alive true
    log_path var/"log/rebind.log"
    error_log_path var/"log/rebind.err"
    environment_variables PATH: std_service_path_env
  end

  def caveats
    <<~EOS
      Rebind requires Accessibility permission:
        System Settings → Privacy & Security → Accessibility
        Add: #{opt_prefix}/Rebind.app

      To start rebind now and on login:
        brew services start rebind

      Config: ~/.config/rebind/config.toml
    EOS
  end
end
```

### User install flow

```bash
brew tap yourusername/rebind
brew install rebind
brew services start rebind
# Grant Accessibility once — persists across upgrades thanks to Developer ID signing
```

## Phase 4: Homebrew Core (later)

Once the project has ~75+ GitHub stars:

1. Submit formula to `homebrew/homebrew-core`
2. Users get `brew install rebind` without a tap
3. Requires passing `brew audit --strict`

## Phase 5: Raycast Store (optional)

Publish the Raycast extension separately:

```bash
cd raycast-extension
npx @raycast/api@latest publish
```

Goes through Raycast's review process. Extension must meet their guidelines.

## Notes

- Config always lives at `~/.config/rebind/config.toml` regardless of install method
- The Homebrew `service` block replaces our custom plist — `brew services start/stop` handles it
- `install.sh` remains for non-Homebrew users (standalone install with its own LaunchAgent)
- Developer ID signing means Accessibility permission persists across `brew upgrade`
