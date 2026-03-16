use crate::keycode::{self, KeyCode};
use serde::Deserialize;
use std::path::Path;

/// Raw TOML config as deserialized
#[derive(Debug, Deserialize)]
pub struct RawConfig {
    #[serde(default)]
    pub modifier_remap: Vec<RawModifierRemap>,
    #[serde(default)]
    pub remap: Vec<RawRemap>,
    #[serde(default)]
    pub tap_hold: Vec<RawTapHold>,
    #[serde(default)]
    pub conditional_remap: Vec<RawConditionalRemap>,
    #[serde(default)]
    pub chord: Vec<RawChord>,
}

#[derive(Debug, Deserialize)]
pub struct RawModifierRemap {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct RawRemap {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct RawTapHold {
    pub key: String,
    pub tap: String,
    pub hold: String,
    #[serde(default = "default_tap_hold_timeout")]
    pub timeout_ms: u64,
}

fn default_tap_hold_timeout() -> u64 {
    200
}

#[derive(Debug, Deserialize)]
pub struct RawConditionalRemap {
    pub modifier: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct RawChord {
    pub keys: Vec<String>,
    pub emit: String,
    #[serde(default = "default_chord_window")]
    pub window_ms: u64,
}

fn default_chord_window() -> u64 {
    100
}

/// Parsed config with resolved keycodes
#[derive(Debug)]
pub struct Config {
    pub modifier_remaps: Vec<ModifierRemap>,
    pub remaps: Vec<Remap>,
    pub tap_holds: Vec<TapHold>,
    pub conditional_remaps: Vec<ConditionalRemap>,
    pub chords: Vec<Chord>,
}

#[derive(Debug)]
pub struct ModifierRemap {
    pub from: String,
    pub from_hid: u64,
    pub to: String,
    pub to_hid: u64,
}

#[derive(Debug)]
pub struct Remap {
    pub from: KeyCode,
    pub to: KeyCode,
}

#[derive(Debug)]
pub struct TapHold {
    pub key: KeyCode,
    pub tap: KeyCode,
    pub hold: KeyCode,
    pub timeout_ms: u64,
}

#[derive(Debug)]
pub struct ConditionalRemap {
    pub modifier: Modifier,
    pub from: KeyCode,
    pub to: KeyCode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Shift,
    Option,
    Cmd,
}

impl std::fmt::Display for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ctrl => "ctrl",
            Self::Shift => "shift",
            Self::Option => "option",
            Self::Cmd => "cmd",
        })
    }
}

#[derive(Debug)]
pub struct Chord {
    pub keys: Vec<KeyCode>,
    pub emit: KeyCode,
    pub window_ms: u64,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read config: {e}"))?;
        let raw: RawConfig =
            toml::from_str(&content).map_err(|e| format!("Failed to parse config: {e}"))?;
        Self::from_raw(raw)
    }

    pub(crate) fn from_raw(raw: RawConfig) -> Result<Self, String> {
        let modifier_remaps = raw
            .modifier_remap
            .into_iter()
            .map(|r| {
                let from_hid = keycode::hid_usage_id(&r.from)
                    .ok_or_else(|| format!("Unknown key for modifier_remap: {}", r.from))?;
                let to_hid = keycode::hid_usage_id(&r.to)
                    .ok_or_else(|| format!("Unknown key for modifier_remap: {}", r.to))?;
                Ok(ModifierRemap {
                    from: r.from,
                    from_hid,
                    to: r.to,
                    to_hid,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let remaps = raw
            .remap
            .into_iter()
            .map(|r| {
                let from = keycode::parse_key(&r.from)
                    .ok_or_else(|| format!("Unknown key for remap: {}", r.from))?;
                let to = keycode::parse_key(&r.to)
                    .ok_or_else(|| format!("Unknown key for remap: {}", r.to))?;
                Ok(Remap { from, to })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let tap_holds = raw
            .tap_hold
            .into_iter()
            .map(|t| {
                let key =
                    keycode::parse_key(&t.key).ok_or_else(|| format!("Unknown key: {}", t.key))?;
                let tap =
                    keycode::parse_key(&t.tap).ok_or_else(|| format!("Unknown key: {}", t.tap))?;
                let hold = keycode::parse_key(&t.hold)
                    .ok_or_else(|| format!("Unknown key: {}", t.hold))?;
                Ok(TapHold {
                    key,
                    tap,
                    hold,
                    timeout_ms: t.timeout_ms,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let conditional_remaps = raw
            .conditional_remap
            .into_iter()
            .map(|r| {
                let modifier = parse_modifier(&r.modifier)?;
                let from = keycode::parse_key(&r.from)
                    .ok_or_else(|| format!("Unknown key: {}", r.from))?;
                let to =
                    keycode::parse_key(&r.to).ok_or_else(|| format!("Unknown key: {}", r.to))?;
                Ok(ConditionalRemap { modifier, from, to })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let chords = raw
            .chord
            .into_iter()
            .map(|c| {
                let keys = c
                    .keys
                    .iter()
                    .map(|k| keycode::parse_key(k).ok_or_else(|| format!("Unknown key: {k}")))
                    .collect::<Result<Vec<_>, String>>()?;
                let emit = keycode::parse_key(&c.emit)
                    .ok_or_else(|| format!("Unknown key: {}", c.emit))?;
                Ok(Chord {
                    keys,
                    emit,
                    window_ms: c.window_ms,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        Ok(Config {
            modifier_remaps,
            remaps,
            tap_holds,
            conditional_remaps,
            chords,
        })
    }
}

pub(crate) fn parse_modifier(name: &str) -> Result<Modifier, String> {
    match name.to_lowercase().as_str() {
        "ctrl" | "control" => Ok(Modifier::Ctrl),
        "shift" => Ok(Modifier::Shift),
        "option" | "alt" => Ok(Modifier::Option),
        "cmd" | "command" => Ok(Modifier::Cmd),
        _ => Err(format!("Unknown modifier: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_config(toml: &str) -> RawConfig {
        toml::from_str(toml).expect("valid TOML")
    }

    // --- from_raw: valid full config ---

    #[test]
    fn from_raw_valid_full_config() {
        let raw = raw_config(
            r#"
[[modifier_remap]]
from = "caps_lock"
to = "left_ctrl"

[[remap]]
from = "a"
to = "b"

[[tap_hold]]
key = "left_ctrl"
tap = "escape"
hold = "left_ctrl"
timeout_ms = 150

[[conditional_remap]]
modifier = "ctrl"
from = "h"
to = "left_arrow"

[[chord]]
keys = ["left_shift", "right_shift"]
emit = "caps_lock"
window_ms = 80
"#,
        );

        let config = Config::from_raw(raw).expect("should parse successfully");

        assert_eq!(config.modifier_remaps.len(), 1);
        assert_eq!(config.modifier_remaps[0].from, "caps_lock");
        assert_eq!(config.modifier_remaps[0].to, "left_ctrl");

        assert_eq!(config.remaps.len(), 1);
        assert_eq!(config.remaps[0].from, KeyCode::A);
        assert_eq!(config.remaps[0].to, KeyCode::B);

        assert_eq!(config.tap_holds.len(), 1);
        assert_eq!(config.tap_holds[0].key, KeyCode::LEFT_CTRL);
        assert_eq!(config.tap_holds[0].tap, KeyCode::ESCAPE);
        assert_eq!(config.tap_holds[0].hold, KeyCode::LEFT_CTRL);
        assert_eq!(config.tap_holds[0].timeout_ms, 150);

        assert_eq!(config.conditional_remaps.len(), 1);
        assert_eq!(config.conditional_remaps[0].modifier, Modifier::Ctrl);
        assert_eq!(config.conditional_remaps[0].from, KeyCode::H);
        assert_eq!(config.conditional_remaps[0].to, KeyCode::LEFT_ARROW);

        assert_eq!(config.chords.len(), 1);
        assert_eq!(
            config.chords[0].keys,
            vec![KeyCode::LEFT_SHIFT, KeyCode::RIGHT_SHIFT]
        );
        assert_eq!(config.chords[0].emit, KeyCode::CAPS_LOCK);
        assert_eq!(config.chords[0].window_ms, 80);
    }

    // --- from_raw: unknown key names ---

    #[test]
    fn from_raw_unknown_remap_key_returns_error() {
        let raw = raw_config(
            r#"
[[remap]]
from = "not_a_key"
to = "a"
"#,
        );
        let result = Config::from_raw(raw);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("not_a_key"),
            "error should mention the bad key: {msg}"
        );
    }

    #[test]
    fn from_raw_unknown_remap_to_key_returns_error() {
        let raw = raw_config(
            r#"
[[remap]]
from = "a"
to = "also_not_a_key"
"#,
        );
        let result = Config::from_raw(raw);
        assert!(result.is_err());
    }

    #[test]
    fn from_raw_unknown_modifier_remap_key_returns_error() {
        let raw = raw_config(
            r#"
[[modifier_remap]]
from = "bad_key"
to = "left_ctrl"
"#,
        );
        let result = Config::from_raw(raw);
        assert!(result.is_err());
    }

    // --- from_raw: unknown modifier name ---

    #[test]
    fn from_raw_unknown_modifier_name_returns_error() {
        let raw = raw_config(
            r#"
[[conditional_remap]]
modifier = "super"
from = "h"
to = "left_arrow"
"#,
        );
        let result = Config::from_raw(raw);
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("super"),
            "error should mention the bad modifier: {msg}"
        );
    }

    // --- parse_modifier ---

    #[test]
    fn parse_modifier_all_valid_names() {
        assert_eq!(parse_modifier("ctrl").unwrap(), Modifier::Ctrl);
        assert_eq!(parse_modifier("control").unwrap(), Modifier::Ctrl);
        assert_eq!(parse_modifier("shift").unwrap(), Modifier::Shift);
        assert_eq!(parse_modifier("option").unwrap(), Modifier::Option);
        assert_eq!(parse_modifier("alt").unwrap(), Modifier::Option);
        assert_eq!(parse_modifier("cmd").unwrap(), Modifier::Cmd);
        assert_eq!(parse_modifier("command").unwrap(), Modifier::Cmd);
    }

    #[test]
    fn parse_modifier_case_insensitive() {
        assert_eq!(parse_modifier("CTRL").unwrap(), Modifier::Ctrl);
        assert_eq!(parse_modifier("Shift").unwrap(), Modifier::Shift);
        assert_eq!(parse_modifier("CMD").unwrap(), Modifier::Cmd);
    }

    #[test]
    fn parse_modifier_unknown_returns_error() {
        assert!(parse_modifier("super").is_err());
        assert!(parse_modifier("meta").is_err());
        assert!(parse_modifier("").is_err());
    }

    // --- empty sections ---

    #[test]
    fn from_raw_empty_config_is_valid() {
        let raw = raw_config("");
        let config = Config::from_raw(raw).expect("empty config should be valid");
        assert!(config.modifier_remaps.is_empty());
        assert!(config.remaps.is_empty());
        assert!(config.tap_holds.is_empty());
        assert!(config.conditional_remaps.is_empty());
        assert!(config.chords.is_empty());
    }

    #[test]
    fn from_raw_default_tap_hold_timeout() {
        let raw = raw_config(
            r#"
[[tap_hold]]
key = "left_ctrl"
tap = "escape"
hold = "left_ctrl"
"#,
        );
        let config = Config::from_raw(raw).unwrap();
        assert_eq!(config.tap_holds[0].timeout_ms, 200);
    }

    #[test]
    fn from_raw_default_chord_window() {
        let raw = raw_config(
            r#"
[[chord]]
keys = ["a", "b"]
emit = "c"
"#,
        );
        let config = Config::from_raw(raw).unwrap();
        assert_eq!(config.chords[0].window_ms, 100);
    }
}
