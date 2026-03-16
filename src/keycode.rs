/// macOS `CGEvent` keycodes (from `Events.h` / Carbon `HIToolbox`)
/// These map to the physical key positions on the keyboard.
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyCode(pub u16);

impl KeyCode {
    // Letters
    pub const A: KeyCode = KeyCode(0x00);
    pub const S: KeyCode = KeyCode(0x01);
    pub const D: KeyCode = KeyCode(0x02);
    pub const F: KeyCode = KeyCode(0x03);
    pub const H: KeyCode = KeyCode(0x04);
    pub const G: KeyCode = KeyCode(0x05);
    pub const Z: KeyCode = KeyCode(0x06);
    pub const X: KeyCode = KeyCode(0x07);
    pub const C: KeyCode = KeyCode(0x08);
    pub const V: KeyCode = KeyCode(0x09);
    pub const B: KeyCode = KeyCode(0x0B);
    pub const Q: KeyCode = KeyCode(0x0C);
    pub const W: KeyCode = KeyCode(0x0D);
    pub const E: KeyCode = KeyCode(0x0E);
    pub const R: KeyCode = KeyCode(0x0F);
    pub const Y: KeyCode = KeyCode(0x10);
    pub const T: KeyCode = KeyCode(0x11);
    pub const O: KeyCode = KeyCode(0x19);
    pub const U: KeyCode = KeyCode(0x20);
    pub const I: KeyCode = KeyCode(0x22);
    pub const P: KeyCode = KeyCode(0x23);
    pub const L: KeyCode = KeyCode(0x25);
    pub const J: KeyCode = KeyCode(0x26);
    pub const K: KeyCode = KeyCode(0x28);
    pub const N: KeyCode = KeyCode(0x2D);
    pub const M: KeyCode = KeyCode(0x2E);

    // Arrows
    pub const LEFT_ARROW: KeyCode = KeyCode(0x7B);
    pub const RIGHT_ARROW: KeyCode = KeyCode(0x7C);
    pub const DOWN_ARROW: KeyCode = KeyCode(0x7D);
    pub const UP_ARROW: KeyCode = KeyCode(0x7E);

    // Modifiers
    pub const LEFT_SHIFT: KeyCode = KeyCode(0x38);
    pub const RIGHT_SHIFT: KeyCode = KeyCode(0x3C);
    pub const LEFT_CTRL: KeyCode = KeyCode(0x3B);
    pub const RIGHT_CTRL: KeyCode = KeyCode(0x3E);
    pub const LEFT_OPTION: KeyCode = KeyCode(0x3A);
    pub const RIGHT_OPTION: KeyCode = KeyCode(0x3D);
    pub const LEFT_CMD: KeyCode = KeyCode(0x37);
    pub const RIGHT_CMD: KeyCode = KeyCode(0x36);
    pub const CAPS_LOCK: KeyCode = KeyCode(0x39);

    // Special
    pub const ESCAPE: KeyCode = KeyCode(0x35);
    pub const TAB: KeyCode = KeyCode(0x30);
    pub const SPACE: KeyCode = KeyCode(0x31);
    pub const RETURN: KeyCode = KeyCode(0x24);
    pub const DELETE: KeyCode = KeyCode(0x33);
    pub const FORWARD_DELETE: KeyCode = KeyCode(0x75);

    // Function keys
    pub const F1: KeyCode = KeyCode(0x7A);
    pub const F2: KeyCode = KeyCode(0x78);
    pub const F3: KeyCode = KeyCode(0x63);
    pub const F4: KeyCode = KeyCode(0x76);
    pub const F5: KeyCode = KeyCode(0x60);
    pub const F6: KeyCode = KeyCode(0x61);
    pub const F7: KeyCode = KeyCode(0x62);
    pub const F8: KeyCode = KeyCode(0x64);
    pub const F9: KeyCode = KeyCode(0x65);
    pub const F10: KeyCode = KeyCode(0x6D);
    pub const F11: KeyCode = KeyCode(0x67);
    pub const F12: KeyCode = KeyCode(0x6F);
}

impl fmt::Display for KeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match *self {
            Self::A => "a",
            Self::S => "s",
            Self::D => "d",
            Self::F => "f",
            Self::H => "h",
            Self::G => "g",
            Self::Z => "z",
            Self::X => "x",
            Self::C => "c",
            Self::V => "v",
            Self::B => "b",
            Self::Q => "q",
            Self::W => "w",
            Self::E => "e",
            Self::R => "r",
            Self::Y => "y",
            Self::T => "t",
            Self::O => "o",
            Self::U => "u",
            Self::I => "i",
            Self::P => "p",
            Self::L => "l",
            Self::J => "j",
            Self::K => "k",
            Self::N => "n",
            Self::M => "m",
            Self::LEFT_ARROW => "left_arrow",
            Self::RIGHT_ARROW => "right_arrow",
            Self::DOWN_ARROW => "down_arrow",
            Self::UP_ARROW => "up_arrow",
            Self::LEFT_SHIFT => "left_shift",
            Self::RIGHT_SHIFT => "right_shift",
            Self::LEFT_CTRL => "left_ctrl",
            Self::RIGHT_CTRL => "right_ctrl",
            Self::LEFT_OPTION => "left_option",
            Self::RIGHT_OPTION => "right_option",
            Self::LEFT_CMD => "left_cmd",
            Self::RIGHT_CMD => "right_cmd",
            Self::CAPS_LOCK => "caps_lock",
            Self::ESCAPE => "escape",
            Self::TAB => "tab",
            Self::SPACE => "space",
            Self::RETURN => "return",
            Self::DELETE => "delete",
            Self::FORWARD_DELETE => "forward_delete",
            Self::F1 => "f1",
            Self::F2 => "f2",
            Self::F3 => "f3",
            Self::F4 => "f4",
            Self::F5 => "f5",
            Self::F6 => "f6",
            Self::F7 => "f7",
            Self::F8 => "f8",
            Self::F9 => "f9",
            Self::F10 => "f10",
            Self::F11 => "f11",
            Self::F12 => "f12",
            _ => return write!(f, "0x{:02X}", self.0),
        };
        write!(f, "{name}")
    }
}

impl FromStr for KeyCode {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name.to_lowercase().as_str() {
            "a" => Ok(Self::A),
            "b" => Ok(Self::B),
            "c" => Ok(Self::C),
            "d" => Ok(Self::D),
            "e" => Ok(Self::E),
            "f" => Ok(Self::F),
            "g" => Ok(Self::G),
            "h" => Ok(Self::H),
            "i" => Ok(Self::I),
            "j" => Ok(Self::J),
            "k" => Ok(Self::K),
            "l" => Ok(Self::L),
            "m" => Ok(Self::M),
            "n" => Ok(Self::N),
            "o" => Ok(Self::O),
            "p" => Ok(Self::P),
            "q" => Ok(Self::Q),
            "r" => Ok(Self::R),
            "s" => Ok(Self::S),
            "t" => Ok(Self::T),
            "u" => Ok(Self::U),
            "v" => Ok(Self::V),
            "w" => Ok(Self::W),
            "x" => Ok(Self::X),
            "y" => Ok(Self::Y),
            "z" => Ok(Self::Z),
            "left_arrow" | "left" => Ok(Self::LEFT_ARROW),
            "right_arrow" | "right" => Ok(Self::RIGHT_ARROW),
            "down_arrow" | "down" => Ok(Self::DOWN_ARROW),
            "up_arrow" | "up" => Ok(Self::UP_ARROW),
            "left_shift" | "lshift" => Ok(Self::LEFT_SHIFT),
            "right_shift" | "rshift" => Ok(Self::RIGHT_SHIFT),
            "left_ctrl" | "lctrl" | "left_control" => Ok(Self::LEFT_CTRL),
            "right_ctrl" | "rctrl" | "right_control" => Ok(Self::RIGHT_CTRL),
            "left_option" | "loption" | "left_alt" | "lalt" => Ok(Self::LEFT_OPTION),
            "right_option" | "roption" | "right_alt" | "ralt" => Ok(Self::RIGHT_OPTION),
            "left_cmd" | "lcmd" | "left_command" => Ok(Self::LEFT_CMD),
            "right_cmd" | "rcmd" | "right_command" => Ok(Self::RIGHT_CMD),
            "caps_lock" | "capslock" | "caps" => Ok(Self::CAPS_LOCK),
            "escape" | "esc" => Ok(Self::ESCAPE),
            "tab" => Ok(Self::TAB),
            "space" => Ok(Self::SPACE),
            "return" | "enter" => Ok(Self::RETURN),
            "delete" | "backspace" => Ok(Self::DELETE),
            "forward_delete" => Ok(Self::FORWARD_DELETE),
            "f1" => Ok(Self::F1),
            "f2" => Ok(Self::F2),
            "f3" => Ok(Self::F3),
            "f4" => Ok(Self::F4),
            "f5" => Ok(Self::F5),
            "f6" => Ok(Self::F6),
            "f7" => Ok(Self::F7),
            "f8" => Ok(Self::F8),
            "f9" => Ok(Self::F9),
            "f10" => Ok(Self::F10),
            "f11" => Ok(Self::F11),
            "f12" => Ok(Self::F12),
            _ => Err(format!("Unknown key: {name}")),
        }
    }
}

/// Parse a key name from the TOML config into a `KeyCode`.
///
/// Convenience wrapper around `KeyCode::from_str` that returns `Option`.
pub fn parse_key(name: &str) -> Option<KeyCode> {
    name.parse().ok()
}

/// Map a key name to its USB HID Usage ID (page 0x07).
/// These are the values used by `hidutil property --set` for `UserKeyMapping`.
/// Reference: USB HID Usage Tables, Section 10 (Keyboard/Keypad Page)
///
/// Note: this takes a key *name* (not a `KeyCode`) because HID usage IDs
/// map to the name-level aliases, and modifier remaps work with names
/// before they're resolved to `KeyCode` values.
pub fn hid_usage_id(name: &str) -> Option<u64> {
    // HID usage IDs are prefixed with 0x700000000 for the keyboard/keypad page
    let base: u64 = 0x0007_0000_0000;
    let id: u64 = match name.to_lowercase().as_str() {
        "a" => 0x04,
        "b" => 0x05,
        "c" => 0x06,
        "d" => 0x07,
        "e" => 0x08,
        "f" => 0x09,
        "g" => 0x0A,
        "h" => 0x0B,
        "i" => 0x0C,
        "j" => 0x0D,
        "k" => 0x0E,
        "l" => 0x0F,
        "m" => 0x10,
        "n" => 0x11,
        "o" => 0x12,
        "p" => 0x13,
        "q" => 0x14,
        "r" => 0x15,
        "s" => 0x16,
        "t" => 0x17,
        "u" => 0x18,
        "v" => 0x19,
        "w" => 0x1A,
        "x" => 0x1B,
        "y" => 0x1C,
        "z" => 0x1D,
        "escape" | "esc" => 0x29,
        "tab" => 0x2B,
        "space" => 0x2C,
        "return" | "enter" => 0x28,
        "delete" | "backspace" => 0x2A,
        "forward_delete" => 0x4C,
        "caps_lock" | "capslock" | "caps" => 0x39,
        "left_shift" | "lshift" => 0xE1,
        "right_shift" | "rshift" => 0xE5,
        "left_ctrl" | "lctrl" | "left_control" => 0xE0,
        "right_ctrl" | "rctrl" | "right_control" => 0xE4,
        "left_option" | "loption" | "left_alt" | "lalt" => 0xE2,
        "right_option" | "roption" | "right_alt" | "ralt" => 0xE6,
        "left_cmd" | "lcmd" | "left_command" => 0xE3,
        "right_cmd" | "rcmd" | "right_command" => 0xE7,
        "left_arrow" | "left" => 0x50,
        "right_arrow" | "right" => 0x4F,
        "down_arrow" | "down" => 0x51,
        "up_arrow" | "up" => 0x52,
        "f1" => 0x3A,
        "f2" => 0x3B,
        "f3" => 0x3C,
        "f4" => 0x3D,
        "f5" => 0x3E,
        "f6" => 0x3F,
        "f7" => 0x40,
        "f8" => 0x41,
        "f9" => 0x42,
        "f10" => 0x43,
        "f11" => 0x44,
        "f12" => 0x45,
        _ => return None,
    };
    Some(base | id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_key: letters ---

    #[test]
    fn parse_key_all_letters() {
        let cases = [
            ("a", KeyCode::A),
            ("b", KeyCode::B),
            ("c", KeyCode::C),
            ("d", KeyCode::D),
            ("e", KeyCode::E),
            ("f", KeyCode::F),
            ("g", KeyCode::G),
            ("h", KeyCode::H),
            ("i", KeyCode::I),
            ("j", KeyCode::J),
            ("k", KeyCode::K),
            ("l", KeyCode::L),
            ("m", KeyCode::M),
            ("n", KeyCode::N),
            ("o", KeyCode::O),
            ("p", KeyCode::P),
            ("q", KeyCode::Q),
            ("r", KeyCode::R),
            ("s", KeyCode::S),
            ("t", KeyCode::T),
            ("u", KeyCode::U),
            ("v", KeyCode::V),
            ("w", KeyCode::W),
            ("x", KeyCode::X),
            ("y", KeyCode::Y),
            ("z", KeyCode::Z),
        ];
        for (name, expected) in cases {
            assert_eq!(
                parse_key(name),
                Some(expected),
                "parse_key({name:?}) should be {expected:?}"
            );
        }
    }

    // --- parse_key: modifiers ---

    #[test]
    fn parse_key_modifiers() {
        let cases = [
            ("left_shift", KeyCode::LEFT_SHIFT),
            ("right_shift", KeyCode::RIGHT_SHIFT),
            ("left_ctrl", KeyCode::LEFT_CTRL),
            ("right_ctrl", KeyCode::RIGHT_CTRL),
            ("left_option", KeyCode::LEFT_OPTION),
            ("right_option", KeyCode::RIGHT_OPTION),
            ("left_cmd", KeyCode::LEFT_CMD),
            ("right_cmd", KeyCode::RIGHT_CMD),
            ("caps_lock", KeyCode::CAPS_LOCK),
        ];
        for (name, expected) in cases {
            assert_eq!(
                parse_key(name),
                Some(expected),
                "parse_key({name:?}) should be {expected:?}"
            );
        }
    }

    // --- parse_key: aliases ---

    #[test]
    fn parse_key_alias_esc_to_escape() {
        assert_eq!(parse_key("esc"), Some(KeyCode::ESCAPE));
    }

    #[test]
    fn parse_key_alias_enter_to_return() {
        assert_eq!(parse_key("enter"), Some(KeyCode::RETURN));
    }

    #[test]
    fn parse_key_alias_backspace_to_delete() {
        assert_eq!(parse_key("backspace"), Some(KeyCode::DELETE));
    }

    #[test]
    fn parse_key_alias_lshift_to_left_shift() {
        assert_eq!(parse_key("lshift"), Some(KeyCode::LEFT_SHIFT));
    }

    #[test]
    fn parse_key_alias_rshift_to_right_shift() {
        assert_eq!(parse_key("rshift"), Some(KeyCode::RIGHT_SHIFT));
    }

    #[test]
    fn parse_key_alias_lctrl() {
        assert_eq!(parse_key("lctrl"), Some(KeyCode::LEFT_CTRL));
        assert_eq!(parse_key("left_control"), Some(KeyCode::LEFT_CTRL));
    }

    #[test]
    fn parse_key_alias_lalt_loption() {
        assert_eq!(parse_key("lalt"), Some(KeyCode::LEFT_OPTION));
        assert_eq!(parse_key("left_alt"), Some(KeyCode::LEFT_OPTION));
        assert_eq!(parse_key("loption"), Some(KeyCode::LEFT_OPTION));
    }

    #[test]
    fn parse_key_alias_lcmd() {
        assert_eq!(parse_key("lcmd"), Some(KeyCode::LEFT_CMD));
        assert_eq!(parse_key("left_command"), Some(KeyCode::LEFT_CMD));
    }

    #[test]
    fn parse_key_alias_caps() {
        assert_eq!(parse_key("caps"), Some(KeyCode::CAPS_LOCK));
        assert_eq!(parse_key("capslock"), Some(KeyCode::CAPS_LOCK));
    }

    #[test]
    fn parse_key_alias_arrow_shortcuts() {
        assert_eq!(parse_key("left"), Some(KeyCode::LEFT_ARROW));
        assert_eq!(parse_key("right"), Some(KeyCode::RIGHT_ARROW));
        assert_eq!(parse_key("up"), Some(KeyCode::UP_ARROW));
        assert_eq!(parse_key("down"), Some(KeyCode::DOWN_ARROW));
    }

    // --- parse_key: unknown ---

    #[test]
    fn parse_key_unknown_returns_none() {
        assert_eq!(parse_key("not_a_key"), None);
        assert_eq!(parse_key(""), None);
        assert_eq!(parse_key("123"), None);
    }

    // --- parse_key: case insensitivity ---

    #[test]
    fn parse_key_case_insensitive() {
        assert_eq!(parse_key("A"), Some(KeyCode::A));
        assert_eq!(parse_key("ESC"), Some(KeyCode::ESCAPE));
        assert_eq!(parse_key("Left_Shift"), Some(KeyCode::LEFT_SHIFT));
        assert_eq!(parse_key("CAPS_LOCK"), Some(KeyCode::CAPS_LOCK));
        assert_eq!(parse_key("F1"), Some(KeyCode::F1));
    }

    // --- hid_usage_id ---

    #[test]
    fn hid_usage_id_known_keys() {
        let base: u64 = 0x0007_0000_0000;
        assert_eq!(hid_usage_id("a"), Some(base | 0x04));
        assert_eq!(hid_usage_id("z"), Some(base | 0x1D));
        assert_eq!(hid_usage_id("escape"), Some(base | 0x29));
        assert_eq!(hid_usage_id("space"), Some(base | 0x2C));
        assert_eq!(hid_usage_id("return"), Some(base | 0x28));
        assert_eq!(hid_usage_id("left_shift"), Some(base | 0xE1));
        assert_eq!(hid_usage_id("caps_lock"), Some(base | 0x39));
        assert_eq!(hid_usage_id("left_arrow"), Some(base | 0x50));
        assert_eq!(hid_usage_id("f1"), Some(base | 0x3A));
        assert_eq!(hid_usage_id("f12"), Some(base | 0x45));
    }

    #[test]
    fn hid_usage_id_aliases() {
        let base: u64 = 0x0007_0000_0000;
        assert_eq!(hid_usage_id("esc"), Some(base | 0x29));
        assert_eq!(hid_usage_id("enter"), Some(base | 0x28));
        assert_eq!(hid_usage_id("backspace"), Some(base | 0x2A));
        assert_eq!(hid_usage_id("lshift"), Some(base | 0xE1));
    }

    #[test]
    fn hid_usage_id_unknown_returns_none() {
        assert_eq!(hid_usage_id("not_a_key"), None);
        assert_eq!(hid_usage_id(""), None);
    }

    // --- Display ---

    #[test]
    fn display_known_keys() {
        assert_eq!(KeyCode::A.to_string(), "a");
        assert_eq!(KeyCode::Z.to_string(), "z");
        assert_eq!(KeyCode::ESCAPE.to_string(), "escape");
        assert_eq!(KeyCode::RETURN.to_string(), "return");
        assert_eq!(KeyCode::DELETE.to_string(), "delete");
        assert_eq!(KeyCode::LEFT_SHIFT.to_string(), "left_shift");
        assert_eq!(KeyCode::CAPS_LOCK.to_string(), "caps_lock");
        assert_eq!(KeyCode::LEFT_ARROW.to_string(), "left_arrow");
        assert_eq!(KeyCode::F1.to_string(), "f1");
        assert_eq!(KeyCode::F12.to_string(), "f12");
        assert_eq!(KeyCode::SPACE.to_string(), "space");
        assert_eq!(KeyCode::TAB.to_string(), "tab");
    }

    #[test]
    fn display_unknown_keycode_uses_hex() {
        // A keycode not in the match table should format as hex
        assert_eq!(KeyCode(0xFF).to_string(), "0xFF");
    }
}

/// Modifier flags as reported by `CGEvent`
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // these are genuine modifier flags, not config options
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub option: bool,
    pub cmd: bool,
    pub caps_lock: bool,
}

impl Modifiers {
    pub fn from_cg_flags(flags: u64) -> Self {
        Self {
            ctrl: flags & 0x40000 != 0,      // kCGEventFlagMaskControl
            shift: flags & 0x20000 != 0,     // kCGEventFlagMaskShift
            option: flags & 0x80000 != 0,    // kCGEventFlagMaskAlternate
            cmd: flags & 0x0010_0000 != 0,   // kCGEventFlagMaskCommand
            caps_lock: flags & 0x10000 != 0, // kCGEventFlagMaskAlphaShift
        }
    }

    /// Check whether a given modifier is currently active.
    pub fn is_active(self, modifier: crate::config::Modifier) -> bool {
        match modifier {
            crate::config::Modifier::Ctrl => self.ctrl,
            crate::config::Modifier::Shift => self.shift,
            crate::config::Modifier::Option => self.option,
            crate::config::Modifier::Cmd => self.cmd,
        }
    }
}
