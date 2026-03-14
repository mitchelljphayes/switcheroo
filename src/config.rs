use crate::keycode::{self, KeyCode};
use serde::Deserialize;
use std::path::PathBuf;

/// Raw TOML config as deserialized
#[derive(Debug, Deserialize)]
pub struct RawConfig {
    #[serde(default)]
    pub modifier_remap: Vec<RawModifierRemap>,
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

#[derive(Debug)]
pub struct Chord {
    pub keys: Vec<KeyCode>,
    pub emit: KeyCode,
    pub window_ms: u64,
}

impl Config {
    pub fn load(path: &PathBuf) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read config: {e}"))?;
        let raw: RawConfig =
            toml::from_str(&content).map_err(|e| format!("Failed to parse config: {e}"))?;
        Self::from_raw(raw)
    }

    fn from_raw(raw: RawConfig) -> Result<Self, String> {
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
            tap_holds,
            conditional_remaps,
            chords,
        })
    }
}

fn parse_modifier(name: &str) -> Result<Modifier, String> {
    match name.to_lowercase().as_str() {
        "ctrl" | "control" => Ok(Modifier::Ctrl),
        "shift" => Ok(Modifier::Shift),
        "option" | "alt" => Ok(Modifier::Option),
        "cmd" | "command" => Ok(Modifier::Cmd),
        _ => Err(format!("Unknown modifier: {name}")),
    }
}
