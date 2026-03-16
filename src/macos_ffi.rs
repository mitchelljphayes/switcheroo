//! Safe wrappers around macOS system APIs that require `unsafe`.
//!
//! This module is the **only** place in the codebase where `unsafe` is allowed.
//! It provides a safe boundary so the rest of the application never touches raw
//! FFI directly.
#![allow(unsafe_code)]

use core_foundation::runloop::CFRunLoopSource;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopMode};
use std::os::raw::c_int;

// ── IOKit FFI declarations ──────────────────────────────────────────

type IOReturn = c_int;

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

// mach_task_self() is a macro in C that expands to this global variable
extern "C" {
    static mach_task_self_: u32;
}

// ── Safe public API ─────────────────────────────────────────────────

/// Toggle the system caps lock state on/off via `IOKit`.
///
/// This calls into the `IOHIDSystem` kernel service to read and flip
/// the caps lock modifier lock state.
pub fn toggle_caps_lock() -> Result<(), String> {
    // Safety: all IOKit calls here follow the documented calling convention.
    // We check every return code and clean up resources (IOObjectRelease,
    // IOServiceClose) on all paths.
    unsafe {
        let class_name = std::ffi::CString::new("IOHIDSystem")
            .map_err(|e| format!("CString creation failed: {e}"))?;
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
            &raw mut connection,
        );
        IOObjectRelease(service);

        if kr != KERN_SUCCESS {
            return Err(format!("IOServiceOpen failed: {kr:#x}"));
        }

        // Get current state
        let mut current_state = false;
        let kr =
            IOHIDGetModifierLockState(connection, K_IO_HID_CAPS_LOCK_STATE, &raw mut current_state);
        if kr != KERN_SUCCESS {
            IOServiceClose(connection);
            return Err(format!("IOHIDGetModifierLockState failed: {kr:#x}"));
        }

        // Toggle it
        let new_state = !current_state;
        log::debug!("Caps lock: current={current_state}, setting to={new_state}");
        let kr = IOHIDSetModifierLockState(connection, K_IO_HID_CAPS_LOCK_STATE, new_state);
        IOServiceClose(connection);

        if kr != KERN_SUCCESS {
            return Err(format!("IOHIDSetModifierLockState failed: {kr:#x}"));
        }

        log::info!("Caps lock toggled: {current_state} → {new_state}");
        Ok(())
    }
}

/// Get the `kCFRunLoopCommonModes` constant safely.
///
/// The underlying value is an `extern "C"` static which requires `unsafe`
/// to access, but the value itself is a plain `CFRunLoopMode` string constant.
pub fn common_run_loop_mode() -> CFRunLoopMode {
    // Safety: kCFRunLoopCommonModes is a well-known CoreFoundation constant
    // that is always valid for the lifetime of the process.
    unsafe { kCFRunLoopCommonModes }
}

/// Add a `CFRunLoopSource` to the current run loop using `kCFRunLoopCommonModes`.
pub fn add_source_to_current_run_loop(source: &CFRunLoopSource) {
    let current = CFRunLoop::get_current();
    current.add_source(source, common_run_loop_mode());
}
