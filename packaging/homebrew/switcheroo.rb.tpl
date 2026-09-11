# typed: strict
# frozen_string_literal: true
__REHEARSAL_MARKER__
# ─────────────────────────────────────────────────────────────────────
# packaging/homebrew/switcheroo.rb.tpl — Homebrew Formula template.
#
# Placeholders (rendered by packaging/homebrew/render_formula.sh):
#   __URL__     → immutable GitHub source archive URL (constructed internally)
#   __SHA256__  → SHA-256 of the source archive (64 lowercase hex)
#   __VERSION__ → version string (used only in rehearsal comments; the
#                 formula body has no explicit version stanza — Homebrew
#                 derives it from the URL tag).
#
# The template is tracked in-repo. The rendered switcheroo.rb (with a
# real sha256) is produced AFTER a public tag/archive exists — a later
# authorized action. Until then, only this template ships.
#
# Structural rules (approved defaults, see plan.md §5):
#   - builds native from source via `cargo install *std_cargo_args --locked`
#   - macOS-only, rust build dependency
#   - service do block (standard homebrew.mxcl.switcheroo label)
#   - does NOT call install.sh / install-binary.sh
#   - does NOT write ~/.local/bin or ~/Library/LaunchAgents
#   - installs sample config under Homebrew-managed etc/ only
#   - copies pre-built bundle/AppIcon.icns (iconutil cannot run in
#     the Homebrew build sandbox; no sips/iconutil/Dir.mktmpdir at
#     install time)
#   - explicit service name "homebrew.mxcl.switcheroo" for compatibility
#     with the published tap
#   - caveats document Accessibility + config location + re-grant note
#   - test block runs --version
# ─────────────────────────────────────────────────────────────────────
class Switcheroo < Formula
  desc "Lightweight macOS keyboard remapper using CGEventTap"
  homepage "https://github.com/mitchelljphayes/switcheroo"
  url "__URL__"
  sha256 "__SHA256__"
  license "MIT"

  depends_on "rust" => :build

  def install
    # Build and install via Homebrew's standard cargo args (includes
    # --locked, --root, --path, --jobs). The binary installs to bin/,
    # then we assemble the .app bundle from it.
    system "cargo", "install", *std_cargo_args

    # Assemble the .app bundle under the Homebrew cellar.
    app = prefix / "Switcheroo.app"
    (app / "Contents/MacOS").mkpath
    (app / "Contents/Resources").mkpath
    cp bin / "switcheroo", app / "Contents/MacOS/switcheroo"
    cp "bundle/Info.plist", app / "Contents/Info.plist"

    # Install the pre-built AppIcon.icns from the source archive.
    #
    # iconutil -c icns cannot run inside Homebrew's build sandbox
    # (seatbelt profile denies a mach-lookup iconutil needs), so the
    # .icns is generated OUTSIDE the sandbox by scripts/generate_icns.sh
    # and shipped as a tracked asset. This also removes the sips/iconutil
    # `out:`/`err:` kwargs that are not accepted by Formula#system's
    # Sorbet sig and raised TypeError under HOMEBREW_SORBET_RUNTIME=1.
    icns_source = buildpath / "bundle/AppIcon.icns"
    if File.exist?(icns_source)
      cp icns_source, app / "Contents/Resources/AppIcon.icns"
    end

    # Ad-hoc sign the locally-compiled bundle (no Developer ID needed).
    system "/usr/bin/codesign", "--force", "--sign", "-", app.to_s

    # Install sample config to Homebrew etc/ (documentation sample only;
    # the daemon reads from ~/.config/switcheroo/config.toml).
    (etc / "switcheroo").mkpath
    etc.install "config.toml" => "switcheroo/config.toml" unless (etc / "switcheroo/config.toml").exist?
  end

  service do
    name macos: "homebrew.mxcl.switcheroo"
    run [opt_prefix / "Switcheroo.app/Contents/MacOS/switcheroo"]
    keep_alive true
    run_at_load true
    environment_variables PATH: std_service_path_env
    log_path var / "log/switcheroo.log"
    error_log_path var / "log/switcheroo.err"
  end

  def caveats
    <<~EOS
      Switcheroo requires macOS Accessibility permission:
        System Settings → Privacy & Security → Accessibility
        Add: #{opt_prefix}/Switcheroo.app

      To start switcheroo now and on login:
        brew services start switcheroo

      Config: the daemon reads from ~/.config/switcheroo/config.toml
      A sample config is installed at:
        #{etc}/switcheroo/config.toml
      Create or symlink your active config at ~/.config/switcheroo/config.toml

      Note: ad-hoc signing means Accessibility permission may need
      re-granting after each `brew upgrade` (the binary is recompiled
      locally with a new ad-hoc signature).
    EOS
  end

  test do
    assert_match "switcheroo", shell_output("#{opt_prefix}/Switcheroo.app/Contents/MacOS/switcheroo --version")
  end
end
