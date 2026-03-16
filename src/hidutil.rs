//! Apply and clear keyboard remaps via `hidutil property --set`.
//!
//! This sets `UserKeyMapping` at the HID (kernel) level, which is the same
//! mechanism as System Settings → Keyboard → Modifier Keys. Switcheroo applies
//! these on startup and clears them on shutdown so the system is left clean.

use crate::config::ModifierRemap;
use log::info;
use std::process::Command;

/// Apply modifier remaps using hidutil. This sets the `UserKeyMapping` property
/// which remaps keys at the HID driver level (before `CGEventTap` sees them).
pub fn apply_modifier_remaps(remaps: &[ModifierRemap]) -> Result<(), String> {
    if remaps.is_empty() {
        return Ok(());
    }

    let mappings: Vec<String> = remaps
        .iter()
        .map(|r| {
            format!(
                "{{\"HIDKeyboardModifierMappingSrc\":{},\"HIDKeyboardModifierMappingDst\":{}}}",
                r.from_hid, r.to_hid
            )
        })
        .collect();

    let json = format!("{{\"UserKeyMapping\":[{}]}}", mappings.join(","));

    run_hidutil_set(&json)?;

    for r in remaps {
        info!("hidutil: applied {} → {}", r.from, r.to);
    }

    Ok(())
}

/// Clear all `UserKeyMapping` entries, restoring the keyboard to its default state.
///
/// This is called on shutdown (signal or panic) so Switcheroo doesn't leave
/// stale modifier remaps in the kernel when it's not running.
#[allow(clippy::print_stderr)] // logger may be torn down during panic/shutdown
pub fn clear_modifier_remaps() {
    info!("hidutil: clearing UserKeyMapping");
    if let Err(e) = run_hidutil_set("{\"UserKeyMapping\":[]}") {
        // Best-effort — we're shutting down, so just log it.
        eprintln!("Warning: failed to clear hidutil remaps on shutdown: {e}");
    }
}

/// Run `hidutil property --set <json>`.
fn run_hidutil_set(json: &str) -> Result<(), String> {
    let output = Command::new("hidutil")
        .arg("property")
        .arg("--set")
        .arg(json)
        .output()
        .map_err(|e| format!("Failed to run hidutil: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("hidutil failed: {stderr}"));
    }

    Ok(())
}
