use crate::config::Config;
use crate::keycode::{KeyCode, Modifiers};
use log::debug;
use std::time::Instant;

/// The remap engine processes key events and decides what to do with them.
pub struct Engine {
    config: Config,
    chord_state: ChordState,
    tap_hold_state: Vec<TapHoldState>,
}

/// Tracks state for a single tap-hold key
struct TapHoldState {
    /// When the key was pressed (None if not currently held)
    pressed_at: Option<Instant>,
    /// Whether another key was pressed while this key was held
    used_as_hold: bool,
}

/// Tracks state for chord detection (e.g., double-shift → caps lock)
struct ChordState {
    /// For each chord rule, track when its constituent keys were last pressed
    pending: Vec<ChordPending>,
}

struct ChordPending {
    /// Which keys in this chord have been pressed (and when)
    pressed: Vec<(KeyCode, Instant)>,
}

/// What the engine decides to do with an event
pub enum Action {
    /// Let the original event pass through unmodified
    Pass,
    /// Suppress the original event entirely
    #[allow(dead_code)]
    Suppress,
    /// Suppress the original and emit a different key (with no modifiers forced)
    Replace(KeyCode),
    /// Emit a synthetic key tap (down + up) after suppressing the original
    EmitTap(KeyCode),
}

impl Engine {
    pub fn new(config: Config) -> Self {
        let chord_state = ChordState {
            pending: config
                .chords
                .iter()
                .map(|_| ChordPending { pressed: vec![] })
                .collect(),
        };
        let tap_hold_state = config
            .tap_holds
            .iter()
            .map(|_| TapHoldState {
                pressed_at: None,
                used_as_hold: false,
            })
            .collect();
        Engine {
            config,
            chord_state,
            tap_hold_state,
        }
    }

    /// Process a key-down event. Returns the action to take.
    pub fn on_key_down(&mut self, keycode: KeyCode, modifiers: Modifiers) -> Action {
        // Any key-down while a tap-hold key is held means it's being used as a hold
        self.mark_tap_holds_as_used(keycode);

        // Check simple remaps first (unconditional key swaps)
        if let Some(action) = self.check_remaps(keycode) {
            return action;
        }

        // Check conditional remaps (e.g., Ctrl+H → Left Arrow)
        if let Some(action) = self.check_conditional_remaps(keycode, modifiers) {
            return action;
        }

        // Track chord state (e.g., double-shift → caps lock)
        if let Some(action) = self.check_chords_down(keycode) {
            return action;
        }

        Action::Pass
    }

    /// Process a key-up event.
    pub fn on_key_up(&mut self, keycode: KeyCode, modifiers: Modifiers) -> Action {
        // Check simple remaps on key-up too
        if let Some(action) = self.check_remaps(keycode) {
            return action;
        }

        // If this key-up corresponds to a conditional remap, replace the key-up too
        if let Some(action) = self.check_conditional_remaps_up(keycode, modifiers) {
            return action;
        }

        Action::Pass
    }

    /// Process a flags-changed event (modifier key press/release).
    pub fn on_flags_changed(&mut self, keycode: KeyCode, modifiers: Modifiers) -> Action {
        let is_press = is_modifier_press(keycode, modifiers);

        // Check tap-hold (e.g., Ctrl tap → Escape, Ctrl hold → Ctrl)
        if let Some(action) = self.check_tap_hold(keycode, is_press) {
            return action;
        }

        if is_press {
            // Any modifier press while a tap-hold key is held → mark as used
            self.mark_tap_holds_as_used(keycode);

            if let Some(action) = self.check_chords_down(keycode) {
                return action;
            }
        }

        Action::Pass
    }

    /// Check and handle tap-hold keys on flagsChanged events.
    fn check_tap_hold(&mut self, keycode: KeyCode, is_press: bool) -> Option<Action> {
        for (th, state) in self.config.tap_holds.iter().zip(&mut self.tap_hold_state) {
            if keycode != th.key {
                continue;
            }

            if is_press {
                // Key pressed — start tracking
                debug!("Tap-hold key pressed: {keycode}");
                state.pressed_at = Some(Instant::now());
                state.used_as_hold = false;
                // Pass through — let the modifier take effect normally
                return Some(Action::Pass);
            }

            // Key released — was it a tap or a hold?
            let was_tap = if let Some(pressed_at) = state.pressed_at {
                let held_duration = pressed_at.elapsed();
                let within_timeout = held_duration.as_millis() < u128::from(th.timeout_ms);
                !state.used_as_hold && within_timeout
            } else {
                false
            };

            state.pressed_at = None;
            state.used_as_hold = false;

            if was_tap {
                debug!("Tap-hold: {keycode} tapped → emitting {}", th.tap);
                // Suppress the modifier key-up and emit the tap key instead
                return Some(Action::EmitTap(th.tap));
            }

            debug!("Tap-hold: {keycode} used as hold ({})", th.hold);
            // Normal modifier release — pass through
            return Some(Action::Pass);
        }
        None
    }

    /// Mark all currently-held tap-hold keys as "used as hold"
    /// because another key was pressed while they were down.
    fn mark_tap_holds_as_used(&mut self, _trigger_key: KeyCode) {
        for (th, state) in self.config.tap_holds.iter().zip(&mut self.tap_hold_state) {
            if state.pressed_at.is_some() {
                if !state.used_as_hold {
                    debug!(
                        "Tap-hold key {} now used as hold (another key pressed)",
                        th.key
                    );
                }
                state.used_as_hold = true;
            }
        }
    }

    fn check_remaps(&self, keycode: KeyCode) -> Option<Action> {
        for remap in &self.config.remaps {
            if keycode == remap.from {
                debug!("Remap: {} → {}", remap.from, remap.to);
                return Some(Action::Replace(remap.to));
            }
        }
        None
    }

    fn check_conditional_remaps(&self, keycode: KeyCode, modifiers: Modifiers) -> Option<Action> {
        for remap in &self.config.conditional_remaps {
            if keycode == remap.from && modifiers.is_active(remap.modifier) {
                debug!(
                    "Conditional remap: {} + {} → {}",
                    remap.modifier, remap.from, remap.to
                );
                return Some(Action::Replace(remap.to));
            }
        }
        None
    }

    fn check_conditional_remaps_up(
        &self,
        keycode: KeyCode,
        modifiers: Modifiers,
    ) -> Option<Action> {
        for remap in &self.config.conditional_remaps {
            if keycode == remap.from && modifiers.is_active(remap.modifier) {
                return Some(Action::Replace(remap.to));
            }
        }
        None
    }

    fn check_chords_down(&mut self, keycode: KeyCode) -> Option<Action> {
        let now = Instant::now();

        for (i, chord) in self.config.chords.iter().enumerate() {
            if !chord.keys.contains(&keycode) {
                continue;
            }

            let pending = &mut self.chord_state.pending[i];

            // Remove stale presses outside the window
            let window = std::time::Duration::from_millis(chord.window_ms);
            let before = pending.pressed.len();
            pending
                .pressed
                .retain(|(_, t)| now.duration_since(*t) < window);
            if pending.pressed.len() < before {
                debug!(
                    "Chord: expired {} stale keys (window {}ms)",
                    before - pending.pressed.len(),
                    chord.window_ms
                );
            }

            // Add this key press (avoid duplicates)
            if pending.pressed.iter().any(|(k, _)| *k == keycode) {
                // Update timestamp for existing key
                for (k, t) in &mut pending.pressed {
                    if *k == keycode {
                        *t = now;
                        break;
                    }
                }
            } else {
                pending.pressed.push((keycode, now));
            }

            debug!(
                "Chord state: {}/{} keys pressed",
                pending.pressed.len(),
                chord.keys.len()
            );

            // Check if all chord keys are pressed within the window
            if chord.keys.len() == pending.pressed.len()
                && chord
                    .keys
                    .iter()
                    .all(|k| pending.pressed.iter().any(|(pk, _)| pk == k))
            {
                debug!("Chord triggered: {:?} → {}", chord.keys, chord.emit);
                pending.pressed.clear();
                // Chords involving modifier keys arrive as flagsChanged events,
                // so we need to emit a synthetic key tap rather than replacing
                // the event (which would just change the keycode on a flagsChanged).
                return Some(Action::EmitTap(chord.emit));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Chord, ConditionalRemap, Config, Modifier, Remap, TapHold};
    use crate::keycode::KeyCode;

    fn no_modifiers() -> Modifiers {
        Modifiers::default()
    }

    fn ctrl_held() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        }
    }

    fn empty_config() -> Config {
        Config {
            modifier_remaps: vec![],
            remaps: vec![],
            tap_holds: vec![],
            conditional_remaps: vec![],
            chords: vec![],
        }
    }

    // --- Simple remaps ---

    #[test]
    fn simple_remap_a_to_b_on_key_down() {
        let config = Config {
            remaps: vec![Remap {
                from: KeyCode::A,
                to: KeyCode::B,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);
        let action = engine.on_key_down(KeyCode::A, no_modifiers());
        assert!(
            matches!(action, Action::Replace(k) if k == KeyCode::B),
            "expected Replace(B)"
        );
    }

    #[test]
    fn simple_remap_a_to_b_on_key_up() {
        let config = Config {
            remaps: vec![Remap {
                from: KeyCode::A,
                to: KeyCode::B,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);
        let action = engine.on_key_up(KeyCode::A, no_modifiers());
        assert!(
            matches!(action, Action::Replace(k) if k == KeyCode::B),
            "expected Replace(B) on key-up"
        );
    }

    #[test]
    fn simple_remap_non_mapped_key_passes_through() {
        let config = Config {
            remaps: vec![Remap {
                from: KeyCode::A,
                to: KeyCode::B,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);
        // C is not remapped — should pass through
        let action = engine.on_key_down(KeyCode::C, no_modifiers());
        assert!(
            matches!(action, Action::Pass),
            "expected Pass for unmapped key"
        );
    }

    // --- Conditional remaps ---

    #[test]
    fn conditional_remap_ctrl_h_to_left_arrow() {
        let config = Config {
            conditional_remaps: vec![ConditionalRemap {
                modifier: Modifier::Ctrl,
                from: KeyCode::H,
                to: KeyCode::LEFT_ARROW,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);
        let action = engine.on_key_down(KeyCode::H, ctrl_held());
        assert!(
            matches!(action, Action::Replace(k) if k == KeyCode::LEFT_ARROW),
            "expected Replace(LEFT_ARROW)"
        );
    }

    #[test]
    fn conditional_remap_h_without_modifier_passes_through() {
        let config = Config {
            conditional_remaps: vec![ConditionalRemap {
                modifier: Modifier::Ctrl,
                from: KeyCode::H,
                to: KeyCode::LEFT_ARROW,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);
        let action = engine.on_key_down(KeyCode::H, no_modifiers());
        assert!(
            matches!(action, Action::Pass),
            "H without ctrl should pass through"
        );
    }

    // --- Tap-hold ---

    #[test]
    fn tap_hold_quick_press_and_release_emits_tap_key() {
        let config = Config {
            tap_holds: vec![TapHold {
                key: KeyCode::LEFT_CTRL,
                tap: KeyCode::ESCAPE,
                hold: KeyCode::LEFT_CTRL,
                timeout_ms: 300,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);

        // Simulate press (flags changed with ctrl active)
        let press_mods = Modifiers {
            ctrl: true,
            ..no_modifiers()
        };
        let _ = engine.on_flags_changed(KeyCode::LEFT_CTRL, press_mods);

        // Release quickly — within timeout
        std::thread::sleep(std::time::Duration::from_millis(20));
        let release_mods = no_modifiers(); // ctrl no longer held
        let action = engine.on_flags_changed(KeyCode::LEFT_CTRL, release_mods);

        assert!(
            matches!(action, Action::EmitTap(k) if k == KeyCode::ESCAPE),
            "quick tap should emit tap key (Escape)"
        );
    }

    #[test]
    fn tap_hold_key_used_as_hold_when_another_key_pressed() {
        let config = Config {
            tap_holds: vec![TapHold {
                key: KeyCode::LEFT_CTRL,
                tap: KeyCode::ESCAPE,
                hold: KeyCode::LEFT_CTRL,
                timeout_ms: 300,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);

        // Press the tap-hold key
        let press_mods = Modifiers {
            ctrl: true,
            ..no_modifiers()
        };
        let _ = engine.on_flags_changed(KeyCode::LEFT_CTRL, press_mods);

        // Press another key while it is held — marks it as used-as-hold
        let _ = engine.on_key_down(KeyCode::H, press_mods);

        // Release the tap-hold key — should pass (not emit tap)
        let release_mods = no_modifiers();
        let action = engine.on_flags_changed(KeyCode::LEFT_CTRL, release_mods);

        assert!(
            matches!(action, Action::Pass),
            "key used as hold should result in Pass on release"
        );
    }

    // --- Chords ---

    #[test]
    fn chord_both_keys_within_window_triggers_emit() {
        let config = Config {
            chords: vec![Chord {
                keys: vec![KeyCode::LEFT_SHIFT, KeyCode::RIGHT_SHIFT],
                emit: KeyCode::CAPS_LOCK,
                window_ms: 200,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);

        // Press first key (flags changed — treat as key_down for chord purposes)
        let after_first = engine.on_key_down(KeyCode::LEFT_SHIFT, no_modifiers());
        // First key alone doesn't complete the chord
        assert!(
            !matches!(after_first, Action::EmitTap(_)),
            "single key should not trigger chord"
        );

        // Press second key quickly within window
        std::thread::sleep(std::time::Duration::from_millis(10));
        let after_second = engine.on_key_down(KeyCode::RIGHT_SHIFT, no_modifiers());
        assert!(
            matches!(after_second, Action::EmitTap(k) if k == KeyCode::CAPS_LOCK),
            "both chord keys within window should trigger EmitTap(CAPS_LOCK)"
        );
    }

    #[test]
    fn chord_keys_outside_window_do_not_trigger() {
        let config = Config {
            chords: vec![Chord {
                keys: vec![KeyCode::LEFT_SHIFT, KeyCode::RIGHT_SHIFT],
                emit: KeyCode::CAPS_LOCK,
                window_ms: 30,
            }],
            ..empty_config()
        };
        let mut engine = Engine::new(config);

        // Press first key
        let _ = engine.on_key_down(KeyCode::LEFT_SHIFT, no_modifiers());

        // Wait longer than the chord window
        std::thread::sleep(std::time::Duration::from_millis(60));

        // Press second key — first press is now stale
        let action = engine.on_key_down(KeyCode::RIGHT_SHIFT, no_modifiers());
        assert!(
            !matches!(action, Action::EmitTap(_)),
            "stale chord keys outside window should not trigger"
        );
    }
}

/// Determine if a flagsChanged event represents a press or release.
fn is_modifier_press(keycode: KeyCode, modifiers: Modifiers) -> bool {
    match keycode {
        KeyCode::LEFT_SHIFT | KeyCode::RIGHT_SHIFT => modifiers.shift,
        KeyCode::LEFT_CTRL | KeyCode::RIGHT_CTRL => modifiers.ctrl,
        KeyCode::LEFT_OPTION | KeyCode::RIGHT_OPTION => modifiers.option,
        KeyCode::LEFT_CMD | KeyCode::RIGHT_CMD => modifiers.cmd,
        KeyCode::CAPS_LOCK => modifiers.caps_lock,
        _ => false,
    }
}
