use crate::engine::{Action, Engine};
use crate::keycode::{KeyCode, Modifiers};
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_graphics::event::{
    CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use log::{debug, info, warn};
use std::cell::RefCell;

// CGEvent flag masks
const K_CG_EVENT_FLAG_MASK_CONTROL: u64 = 0x40000;

// IOKit FFI for toggling caps lock state
mod iokit {
    use std::os::raw::c_int;

    pub type IOReturn = c_int;

    const KERN_SUCCESS: IOReturn = 0;
    const K_IO_HID_PARAM_CONNECT_TYPE: u32 = 1;
    const K_IO_HID_CAPS_LOCK_STATE: c_int = 1;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceGetMatchingService(main_port: u32, matching: *const std::ffi::c_void) -> u32;
        fn IOServiceMatching(name: *const std::os::raw::c_char) -> *const std::ffi::c_void;
        fn IOServiceOpen(
            service: u32,
            owning_task: u32,
            connect_type: u32,
            connection: *mut u32,
        ) -> IOReturn;
        fn IOServiceClose(connection: u32) -> IOReturn;
        fn IOObjectRelease(object: u32) -> IOReturn;
        fn IOHIDGetModifierLockState(handle: u32, selector: c_int, state: *mut bool) -> IOReturn;
        fn IOHIDSetModifierLockState(handle: u32, selector: c_int, state: bool) -> IOReturn;
    }

    // mach_task_self() is a macro that expands to this global variable
    extern "C" {
        static mach_task_self_: u32;
    }

    /// Toggle caps lock on/off using IOKit
    pub fn toggle_caps_lock() -> Result<(), String> {
        unsafe {
            let class_name = std::ffi::CString::new("IOHIDSystem").unwrap();
            let matching = IOServiceMatching(class_name.as_ptr());
            if matching.is_null() {
                return Err("IOServiceMatching failed".into());
            }

            // kIOMasterPortDefault = 0
            let service = IOServiceGetMatchingService(0, matching);
            if service == 0 {
                return Err("IOServiceGetMatchingService failed".into());
            }

            let mut connection: u32 = 0;
            let kr = IOServiceOpen(
                service,
                mach_task_self_,
                K_IO_HID_PARAM_CONNECT_TYPE,
                &mut connection,
            );
            IOObjectRelease(service);

            if kr != KERN_SUCCESS {
                return Err(format!("IOServiceOpen failed: {:#x}", kr));
            }

            // Get current state
            let mut current_state = false;
            let kr =
                IOHIDGetModifierLockState(connection, K_IO_HID_CAPS_LOCK_STATE, &mut current_state);
            if kr != KERN_SUCCESS {
                IOServiceClose(connection);
                return Err(format!("IOHIDGetModifierLockState failed: {:#x}", kr));
            }

            // Toggle it
            let new_state = !current_state;
            log::debug!(
                "Caps lock: current={}, setting to={}",
                current_state,
                new_state
            );
            let kr = IOHIDSetModifierLockState(connection, K_IO_HID_CAPS_LOCK_STATE, new_state);
            IOServiceClose(connection);

            if kr != KERN_SUCCESS {
                return Err(format!("IOHIDSetModifierLockState failed: {:#x}", kr));
            }

            log::info!("Caps lock toggled: {} → {}", current_state, new_state);
            Ok(())
        }
    }
}

/// Post a synthetic key tap (keyDown + keyUp) for the given keycode.
/// For caps_lock, uses IOKit to toggle the system caps lock state.
fn post_key_tap(proxy: core_graphics::event::CGEventTapProxy, keycode: KeyCode) {
    if keycode == KeyCode::CAPS_LOCK {
        if let Err(e) = iokit::toggle_caps_lock() {
            log::error!("Failed to toggle caps lock: {}", e);
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

/// Start the CGEventTap and run the event loop. This blocks forever.
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
                    debug!("KeyDown: {} modifiers: {:?}", keycode, modifiers);
                    engine.on_key_down(keycode, modifiers)
                }
                CGEventType::KeyUp => {
                    debug!("KeyUp: {} modifiers: {:?}", keycode, modifiers);
                    engine.on_key_up(keycode, modifiers)
                }
                CGEventType::FlagsChanged => {
                    debug!("FlagsChanged: {} modifiers: {:?}", keycode, modifiers);
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
                        new_keycode.0 as i64,
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
                    debug!("EmitTap: posting synthetic {} tap", tap_keycode);
                    post_key_tap(proxy, tap_keycode);
                    // Still pass through the original flagsChanged so the modifier
                    // state updates correctly in the system
                    Some(cg_event.clone())
                }
            }
        },
    )
    .map_err(|_| {
        "Failed to create CGEventTap. \
         Make sure Accessibility is enabled in \
         System Settings → Privacy & Security → Accessibility"
            .to_string()
    })?;

    info!("CGEventTap created successfully");

    let loop_source = tap
        .mach_port
        .create_runloop_source(0)
        .map_err(|_| "Failed to create run loop source".to_string())?;

    let current = CFRunLoop::get_current();
    current.add_source(&loop_source, unsafe { kCFRunLoopCommonModes });

    tap.enable();

    info!("Event loop running — press Ctrl+C to stop");

    CFRunLoop::run_current();

    Ok(())
}
