//! Apply keyboard remaps via `hidutil property --set`.
//!
//! This sets `UserKeyMapping` at the HID (kernel) level, which is the same
//! mechanism as System Settings → Keyboard → Modifier Keys, but persists
//! across reboots when applied on startup.

use crate::config::ModifierRemap;
use log::info;
use std::process::Command;

/// Apply modifier remaps using hidutil. This sets the `UserKeyMapping` property
/// which remaps keys at the HID driver level (before `CGEventTap` sees them).
///
/// If no remaps are configured, clears any existing `UserKeyMapping`.
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

    let output = Command::new("hidutil")
        .arg("property")
        .arg("--set")
        .arg(&json)
        .output()
        .map_err(|e| format!("Failed to run hidutil: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("hidutil failed: {stderr}"));
    }

    for r in remaps {
        info!("hidutil: {} → {}", r.from, r.to);
    }

    Ok(())
}
