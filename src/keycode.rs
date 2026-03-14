/// macOS CGEvent keycodes (from Events.h / Carbon HIToolbox)
/// These map to the physical key positions on the keyboard.
use std::fmt;

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
        write!(f, "{}", name)
    }
}

/// Parse a key name from the TOML config into a KeyCode
pub fn parse_key(name: &str) -> Option<KeyCode> {
    match name.to_lowercase().as_str() {
        "a" => Some(KeyCode::A),
        "b" => Some(KeyCode::B),
        "c" => Some(KeyCode::C),
        "d" => Some(KeyCode::D),
        "e" => Some(KeyCode::E),
        "f" => Some(KeyCode::F),
        "g" => Some(KeyCode::G),
        "h" => Some(KeyCode::H),
        "i" => Some(KeyCode::I),
        "j" => Some(KeyCode::J),
        "k" => Some(KeyCode::K),
        "l" => Some(KeyCode::L),
        "m" => Some(KeyCode::M),
        "n" => Some(KeyCode::N),
        "o" => Some(KeyCode::O),
        "p" => Some(KeyCode::P),
        "q" => Some(KeyCode::Q),
        "r" => Some(KeyCode::R),
        "s" => Some(KeyCode::S),
        "t" => Some(KeyCode::T),
        "u" => Some(KeyCode::U),
        "v" => Some(KeyCode::V),
        "w" => Some(KeyCode::W),
        "x" => Some(KeyCode::X),
        "y" => Some(KeyCode::Y),
        "z" => Some(KeyCode::Z),
        "left_arrow" | "left" => Some(KeyCode::LEFT_ARROW),
        "right_arrow" | "right" => Some(KeyCode::RIGHT_ARROW),
        "down_arrow" | "down" => Some(KeyCode::DOWN_ARROW),
        "up_arrow" | "up" => Some(KeyCode::UP_ARROW),
        "left_shift" | "lshift" => Some(KeyCode::LEFT_SHIFT),
        "right_shift" | "rshift" => Some(KeyCode::RIGHT_SHIFT),
        "left_ctrl" | "lctrl" | "left_control" => Some(KeyCode::LEFT_CTRL),
        "right_ctrl" | "rctrl" | "right_control" => Some(KeyCode::RIGHT_CTRL),
        "left_option" | "loption" | "left_alt" | "lalt" => Some(KeyCode::LEFT_OPTION),
        "right_option" | "roption" | "right_alt" | "ralt" => Some(KeyCode::RIGHT_OPTION),
        "left_cmd" | "lcmd" | "left_command" => Some(KeyCode::LEFT_CMD),
        "right_cmd" | "rcmd" | "right_command" => Some(KeyCode::RIGHT_CMD),
        "caps_lock" | "capslock" | "caps" => Some(KeyCode::CAPS_LOCK),
        "escape" | "esc" => Some(KeyCode::ESCAPE),
        "tab" => Some(KeyCode::TAB),
        "space" => Some(KeyCode::SPACE),
        "return" | "enter" => Some(KeyCode::RETURN),
        "delete" | "backspace" => Some(KeyCode::DELETE),
        "forward_delete" => Some(KeyCode::FORWARD_DELETE),
        "f1" => Some(KeyCode::F1),
        "f2" => Some(KeyCode::F2),
        "f3" => Some(KeyCode::F3),
        "f4" => Some(KeyCode::F4),
        "f5" => Some(KeyCode::F5),
        "f6" => Some(KeyCode::F6),
        "f7" => Some(KeyCode::F7),
        "f8" => Some(KeyCode::F8),
        "f9" => Some(KeyCode::F9),
        "f10" => Some(KeyCode::F10),
        "f11" => Some(KeyCode::F11),
        "f12" => Some(KeyCode::F12),
        _ => None,
    }
}

/// Map a key name to its USB HID Usage ID (page 0x07).
/// These are the values used by `hidutil property --set` for UserKeyMapping.
/// Reference: USB HID Usage Tables, Section 10 (Keyboard/Keypad Page)
pub fn hid_usage_id(name: &str) -> Option<u64> {
    // HID usage IDs are prefixed with 0x700000000 for the keyboard/keypad page
    let base: u64 = 0x700000000;
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

/// Modifier flags as reported by CGEvent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
            cmd: flags & 0x100000 != 0,      // kCGEventFlagMaskCommand
            caps_lock: flags & 0x10000 != 0, // kCGEventFlagMaskAlphaShift
        }
    }
}
