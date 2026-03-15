use crate::config::{Config, Modifier};
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
        if let Some(action) = self.check_conditional_remaps(keycode, &modifiers) {
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
        if let Some(action) = self.check_conditional_remaps_up(keycode, &modifiers) {
            return action;
        }

        Action::Pass
    }

    /// Process a flags-changed event (modifier key press/release).
    pub fn on_flags_changed(&mut self, keycode: KeyCode, modifiers: Modifiers) -> Action {
        let is_press = is_modifier_press(keycode, &modifiers);

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
        for (i, th) in self.config.tap_holds.iter().enumerate() {
            if keycode != th.key {
                continue;
            }

            let state = &mut self.tap_hold_state[i];

            if is_press {
                // Key pressed — start tracking
                debug!("Tap-hold key pressed: {}", keycode);
                state.pressed_at = Some(Instant::now());
                state.used_as_hold = false;
                // Pass through — let the modifier take effect normally
                return Some(Action::Pass);
            } else {
                // Key released — was it a tap or a hold?
                let was_tap = if let Some(pressed_at) = state.pressed_at {
                    let held_duration = pressed_at.elapsed();
                    let within_timeout = held_duration.as_millis() < th.timeout_ms as u128;
                    !state.used_as_hold && within_timeout
                } else {
                    false
                };

                state.pressed_at = None;
                state.used_as_hold = false;

                if was_tap {
                    debug!("Tap-hold: {} tapped → emitting {}", keycode, th.tap);
                    // Suppress the modifier key-up and emit the tap key instead
                    return Some(Action::EmitTap(th.tap));
                } else {
                    debug!("Tap-hold: {} used as hold ({})", keycode, th.hold);
                    // Normal modifier release — pass through
                    return Some(Action::Pass);
                }
            }
        }
        None
    }

    /// Mark all currently-held tap-hold keys as "used as hold"
    /// because another key was pressed while they were down.
    fn mark_tap_holds_as_used(&mut self, _trigger_key: KeyCode) {
        for (i, th) in self.config.tap_holds.iter().enumerate() {
            let state = &mut self.tap_hold_state[i];
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

    fn check_conditional_remaps(&self, keycode: KeyCode, modifiers: &Modifiers) -> Option<Action> {
        for remap in &self.config.conditional_remaps {
            if keycode == remap.from && modifier_active(remap.modifier, modifiers) {
                debug!(
                    "Conditional remap: {} + {} → {}",
                    modifier_name(remap.modifier),
                    remap.from,
                    remap.to
                );
                return Some(Action::Replace(remap.to));
            }
        }
        None
    }

    fn check_conditional_remaps_up(
        &self,
        keycode: KeyCode,
        modifiers: &Modifiers,
    ) -> Option<Action> {
        for remap in &self.config.conditional_remaps {
            if keycode == remap.from && modifier_active(remap.modifier, modifiers) {
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
            if !pending.pressed.iter().any(|(k, _)| *k == keycode) {
                pending.pressed.push((keycode, now));
            } else {
                // Update timestamp for existing key
                for (k, t) in pending.pressed.iter_mut() {
                    if *k == keycode {
                        *t = now;
                        break;
                    }
                }
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

fn modifier_active(modifier: Modifier, modifiers: &Modifiers) -> bool {
    match modifier {
        Modifier::Ctrl => modifiers.ctrl,
        Modifier::Shift => modifiers.shift,
        Modifier::Option => modifiers.option,
        Modifier::Cmd => modifiers.cmd,
    }
}

fn modifier_name(modifier: Modifier) -> &'static str {
    match modifier {
        Modifier::Ctrl => "ctrl",
        Modifier::Shift => "shift",
        Modifier::Option => "option",
        Modifier::Cmd => "cmd",
    }
}

/// Determine if a flagsChanged event represents a press or release.
fn is_modifier_press(keycode: KeyCode, modifiers: &Modifiers) -> bool {
    match keycode {
        KeyCode::LEFT_SHIFT | KeyCode::RIGHT_SHIFT => modifiers.shift,
        KeyCode::LEFT_CTRL | KeyCode::RIGHT_CTRL => modifiers.ctrl,
        KeyCode::LEFT_OPTION | KeyCode::RIGHT_OPTION => modifiers.option,
        KeyCode::LEFT_CMD | KeyCode::RIGHT_CMD => modifiers.cmd,
        KeyCode::CAPS_LOCK => modifiers.caps_lock,
        _ => false,
    }
}
