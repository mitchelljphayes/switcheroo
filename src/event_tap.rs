use crate::engine::{Action, Engine};
use crate::keycode::{KeyCode, Modifiers};
use crate::macos_ffi;
use core_foundation::runloop::CFRunLoop;
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use log::{debug, info, warn};
use std::cell::RefCell;

// `CGEvent` flag masks
const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 0x40000;

/// Post a synthetic key tap (keyDown + keyUp) for the given keycode.
/// For `caps_lock`, uses `IOKit` to toggle the system caps lock state.
fn post_key_tap(proxy: core_graphics::event::CGEventTapProxy, keycode: KeyCode) {
    if keycode == KeyCode::CAPS_LOCK {
        if let Err(e) = macos_ffi::toggle_caps_lock() {
            log::error!("Failed to toggle caps lock: {e}");
        }
        return;
    }

    let source = CGEventSource::new(CGEventSourceStateID::Private);
    if let Ok(source) = source {
        if let Ok(key_down) = CGEvent::new_keyboard_event(source.clone(), keycode.0, true) {
            key_down.set_flags(CGEventFlags::empty());
            key_down.post_from_tap(proxy);
        }
        if let Ok(key_up) = CGEvent::new_keyboard_event(source, keycode.0, false) {
            key_up.set_flags(CGEventFlags::empty());
            key_up.post_from_tap(proxy);
        }
    }
}

/// Start the `CGEventTap` and run the event loop. This blocks forever.
pub fn run(engine: Engine) -> Result<(), String> {
    info!("Starting CGEventTap...");

    // RefCell because the callback is Fn (not FnMut), but we need mutability
    let engine_cell = RefCell::new(engine);

    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![
            CGEventType::KeyDown,
            CGEventType::KeyUp,
            CGEventType::FlagsChanged,
        ],
        move |proxy, event_type, cg_event| {
            // If the tap gets disabled, just pass through
            if matches!(
                event_type,
                CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
            ) {
                warn!("Event tap was disabled, re-enabling");
                return Some(cg_event.clone());
            }

            let keycode = KeyCode(
                cg_event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16,
            );
            let flags = cg_event.get_flags().bits();
            let modifiers = Modifiers::from_cg_flags(flags);

            let mut engine = engine_cell.borrow_mut();

            let action = match event_type {
                CGEventType::KeyDown => {
                    debug!("KeyDown: {keycode} modifiers: {modifiers:?}");
                    engine.on_key_down(keycode, modifiers)
                }
                CGEventType::KeyUp => {
                    debug!("KeyUp: {keycode} modifiers: {modifiers:?}");
                    engine.on_key_up(keycode, modifiers)
                }
                CGEventType::FlagsChanged => {
                    debug!("FlagsChanged: {keycode} modifiers: {modifiers:?}");
                    engine.on_flags_changed(keycode, modifiers)
                }
                _ => Action::Pass,
            };

            match action {
                Action::Pass => Some(cg_event.clone()),
                Action::Suppress => None,
                Action::Replace(new_keycode) => {
                    let event = cg_event.clone();
                    event.set_integer_value_field(
                        EventField::KEYBOARD_EVENT_KEYCODE,
                        i64::from(new_keycode.0),
                    );

                    // For conditional remaps (Ctrl+HJKL → arrows), strip the ctrl modifier
                    // so the app receives a clean arrow key event
                    if modifiers.ctrl
                        && matches!(
                            new_keycode,
                            KeyCode::LEFT_ARROW
                                | KeyCode::RIGHT_ARROW
                                | KeyCode::UP_ARROW
                                | KeyCode::DOWN_ARROW
                        )
                    {
                        let new_flags =
                            CGEventFlags::from_bits_truncate(flags & !K_CG_EVENT_FLAG_MASK_CONTROL);
                        event.set_flags(new_flags);
                    }

                    Some(event)
                }
                Action::EmitTap(tap_keycode) => {
                    // Suppress the original event (modifier key-up) and post
                    // a synthetic key tap instead
                    debug!("EmitTap: posting synthetic {tap_keycode} tap");
                    post_key_tap(proxy, tap_keycode);
                    // Still pass through the original flagsChanged so the modifier
                    // state updates correctly in the system
                    Some(cg_event.clone())
                }
            }
        },
    )
    .map_err(|()| {
        "Failed to create CGEventTap. \
         Make sure Accessibility is enabled in \
         System Settings → Privacy & Security → Accessibility"
            .to_string()
    })?;

    info!("CGEventTap created successfully");

    let loop_source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|()| "Failed to create run loop source".to_string())?;

    macos_ffi::add_source_to_current_run_loop(&loop_source);

    tap.enable();

    info!("Event loop running — press Ctrl+C to stop");

    CFRunLoop::run_current();

    Ok(())
}
