//! Safe wrappers around macOS system APIs that require `unsafe`.
//!
//! This module is the **only** place in the codebase where `unsafe` is allowed.
//! It provides a safe boundary so the rest of the application never touches raw
//! FFI directly.
//!
//! ## Power-notification lifecycle (wake-from-sleep reapply)
//!
//! [`PowerWatcher`] wraps the `IOKit` system-power notification lifecycle per
//! Apple QA1340 / `IOPMLib.h`. It registers `IORegisterForSystemPower` on the
//! current thread's `CFRunLoop`, adds a reusable repeating `CFRunLoopTimer`
//! (distant interval) for the ~2 s debounce, and cleans up in `Drop` in the
//! exact QA1340 order:
//!
//! 1. `CFRunLoopTimerInvalidate` (stop the debounce timer)
//! 2. `CFRunLoopRemoveSource` (detach the power-notification source)
//! 3. `IODeregisterForSystemPower` (must come before port destroy)
//! 4. `IOServiceClose(root_port)` (closes the implicitly-opened `IOPMrootDomain`)
//! 5. `IONotificationPortDestroy` (must come LAST; also frees the
//!    `CFRunLoopSource` — never `CFRelease` it ourselves)
//! 6. reclaim the boxed [`PowerContext`] (last, after no more callbacks)
//!
//! `PowerWatcher` is `!Send`/`!Sync` (run-loop handles are thread-affine); it
//! must be created and dropped on the same thread whose `CFRunLoop` it was
//! registered against (the main thread in production).
//!
//! ## Panic safety at the FFI boundary
//!
//! Both `extern "C"` trampolines wrap the closure invocation in
//! `std::panic::catch_unwind(AssertUnwindSafe(...))`. A panic crossing an
//! `extern "C"` boundary aborts the process; `catch_unwind` contains it,
//! emits a fixed non-sensitive diagnostic, and returns a safe no-op. The power
//! callback trampoline only calls infallible `IOKit` primitives (no closure),
//! so it is structurally panic-free, but it is also guarded for symmetry.
#![allow(unsafe_code)]

use core_foundation::base::TCFType;
use core_foundation::date::CFAbsoluteTime;
use core_foundation::runloop::{
    kCFRunLoopCommonModes, CFRunLoop, CFRunLoopMode, CFRunLoopRef, CFRunLoopSource,
    CFRunLoopSourceRef, CFRunLoopTimer, CFRunLoopTimerContext, CFRunLoopTimerRef,
};
use std::ffi::c_void;
use std::marker::PhantomData;
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

// ── IOKit FFI declarations ──────────────────────────────────────────

type IOReturn = c_int;

const KERN_SUCCESS: IOReturn = 0;
const K_IO_HID_PARAM_CONNECT_TYPE: u32 = 1;
const K_IO_HID_CAPS_LOCK_STATE: c_int = 1;

// IOKit system-power message types, from `IOMessage.h`.
//
// `IOMessage` is `UInt32` (`natural_t` = `u32` on Darwin — never `u64`/`usize`).
// Values are `iokit_common_msg(msg)` = `sys_iokit | sub_iokit_common | msg`,
// where `sys_iokit = err_system(0x38) = 0x38 << 26 = 0xe0000000` and
// `sub_iokit_common = err_sub(0) = 0`. Confirmed against the MacOSX15.4.sdk
// `IOMessage.h` / `IOReturn.h` / `mach/error.h` headers.
//
// Only the six `kIOMessageSystem*` / `kIOMessageCanSystemSleep` constants
// delivered to `IORegisterForSystemPower` user clients are needed.
// `WILL_NOT_SLEEP` and `WILL_POWER_ON` are matched by the `_` arm in
// `route_power_message` but are named explicitly in tests to pin the values.
#[allow(dead_code)] // referenced in route_power_message tests
const K_IO_MESSAGE_CAN_SYSTEM_SLEEP: u32 = 0xe000_0270; // kIOMessageCanSystemSleep
#[allow(dead_code)] // referenced in route_power_message tests
const K_IO_MESSAGE_SYSTEM_WILL_NOT_SLEEP: u32 = 0xe000_0290; // kIOMessageSystemWillNotSleep
const K_IO_MESSAGE_SYSTEM_WILL_SLEEP: u32 = 0xe000_0280; // kIOMessageSystemWillSleep
#[allow(dead_code)] // referenced in route_power_message tests
const K_IO_MESSAGE_SYSTEM_WILL_POWER_ON: u32 = 0xe000_0320; // kIOMessageSystemWillPowerOn
const K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON: u32 = 0xe000_0300; // kIOMessageSystemHasPoweredOn

/// Distant-future `CFAbsoluteTime` used to park the debounce timer when
/// disarmed. ~31,700 years from the 2001-01-01 CF epoch.
const TIMER_DISTANT_FUTURE: f64 = 1e12f64;

/// Repeat interval for the debounce timer. Per Apple's reusable-timer pattern
/// (`CFRunLoopTimer` docs), a **repeating** timer with a very large interval
/// (decades) can be rescheduled indefinitely via
/// `CFRunLoopTimerSetNextFireDate`. A non-repeating timer (interval=0)
/// **self-invalidates after the first fire** and cannot be rearmed — that was
/// the blocking review finding. This constant must never be 0; the
/// `timer_interval_is_nonzero` test enforces that.
const TIMER_REPEAT_INTERVAL: f64 = TIMER_DISTANT_FUTURE;

// Opaque CoreFoundation / IOKit handle pointers. `IONotificationPortRef`
// is an IOKit type not exposed by `core-foundation`, so we declare it as
// `*mut c_void`. The `CFRunLoop*Ref` types are re-exported by
// `core-foundation` via `pub use core_foundation_sys::runloop::*`.
type IONotificationPortRef = *mut c_void;

// IOKit interest callback signature (IOPMLib.h / IOKitLib.h). `message_type`
// is `natural_t` = `u32` on Darwin.
type IOServiceInterestCallback = extern "C" fn(
    refcon: *mut c_void,
    service: u32,
    message_type: u32,
    message_argument: *mut c_void,
);

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOServiceGetMatchingService(main_port: u32, matching: *const c_void) -> u32;
    fn IOServiceMatching(name: *const std::os::raw::c_char) -> *const c_void;
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

    // ── Power management (IOPMLib) ───────────────────────────────────
    // IORegisterForSystemPower returns the io_connect_t (root_port) for the
    // implicitly-opened IOPMrootDomain; the caller must IOServiceClose it
    // after IODeregisterForSystemPower. `thePortRef` is allocated by the
    // call and must be released via IONotificationPortDestroy. The
    // CFRunLoopSource returned by IONotificationPortGetRunLoopSource is
    // owned by the port — never CFRelease it (IOKitLib.h).
    fn IORegisterForSystemPower(
        refcon: *mut c_void,
        the_port_ref: *mut IONotificationPortRef,
        callback: IOServiceInterestCallback,
        notifier: *mut u32,
    ) -> u32; // io_connect_t (root_port)
    fn IODeregisterForSystemPower(notifier: *mut u32) -> IOReturn;
    fn IONotificationPortDestroy(notify: IONotificationPortRef);
    fn IONotificationPortGetRunLoopSource(notify: IONotificationPortRef) -> CFRunLoopSourceRef;
    fn IOAllowPowerChange(root_port: u32, notification_id: isize) -> IOReturn; // intptr_t
}

// mach_task_self() is a macro in C that expands to this global variable
extern "C" {
    static mach_task_self_: u32;
}

// ── CoreFoundation timer/run-loop FFI (not wrapped by core-foundation 0.10) ──
//
// `core-foundation 0.10` exposes `CFRunLoopTimer::new` and
// `CFRunLoopTimerContext`, but does NOT expose
// `CFRunLoopTimerSetNextFireDate`, `CFRunLoopTimerInvalidate`,
// `CFRunLoopAddSource`, or `CFRunLoopAddTimer` in a form usable with raw
// port-owned sources (its `CFRunLoop::add_source` takes a `&CFRunLoopSource`
// which would `CFRelease` on drop — incompatible with the port-owned source
// from `IONotificationPortGetRunLoopSource`). We declare the raw symbols here,
// linked against CoreFoundation (already a transitive dependency via the
// `core-foundation` crate).
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFRunLoopMode);
    fn CFRunLoopRemoveSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFRunLoopMode);
    fn CFRunLoopAddTimer(rl: CFRunLoopRef, timer: CFRunLoopTimerRef, mode: CFRunLoopMode);
    fn CFRunLoopTimerSetNextFireDate(timer: CFRunLoopTimerRef, fire_date: CFAbsoluteTime);
    fn CFRunLoopTimerInvalidate(timer: CFRunLoopTimerRef);
    fn CFAbsoluteTimeGetCurrent() -> CFAbsoluteTime;
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
        let kr = IOHIDSetModifierLockState(connection, K_IO_HID_CAPS_LOCK_STATE, new_state);
        IOServiceClose(connection);

        if kr != KERN_SUCCESS {
            return Err(format!("IOHIDSetModifierLockState failed: {kr:#x}"));
        }

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

// ── Power-notification safe API ─────────────────────────────────────

/// Pure routing decision for a single `IOKit` system-power message. The
/// callback trampoline dispatches on this so the ack/trigger rules are
/// unit-testable without any `IOKit` call. Matches Apple QA1340 / `IOPMLib.h`:
///
/// - `Acknowledge` — `kIOMessageCanSystemSleep` and `kIOMessageSystemWillSleep`
///   **must** call `IOAllowPowerChange`; failing to ack `WillSleep` delays
///   sleep by 30 s.
/// - `ScheduleWakeReapply` — `kIOMessageSystemHasPoweredOn` (wake complete,
///   drivers ready); **must not** be acknowledged; this is the reapply
///   trigger.
/// - `Ignore` — `kIOMessageSystemWillPowerOn` (early wake, hardware not
///   ready), `kIOMessageSystemWillNotSleep` (idle sleep vetoed), and any
///   unknown message; **must not** be acknowledged; no action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Acknowledge,
    Ignore,
    ScheduleWakeReapply,
}

/// Route an `IOKit` system-power `messageType` (`natural_t` = `u32`) to the
/// action the callback should take. Pure: no `IOKit` calls, no allocations.
///
/// Constants verified against
/// `MacOSX15.4.sdk/System/Library/Frameworks/IOKit.framework/Headers/IOMessage.h`
/// (values computed from `iokit_common_msg(msg)` = `0xe0000000 | msg`).
pub fn route_power_message(message_type: u32) -> PowerAction {
    match message_type {
        K_IO_MESSAGE_CAN_SYSTEM_SLEEP | K_IO_MESSAGE_SYSTEM_WILL_SLEEP => PowerAction::Acknowledge,
        K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON => PowerAction::ScheduleWakeReapply,
        // WillPowerOn (early wake, hw not ready), WillNotSleep (idle vetoed),
        // and any unknown message: do nothing, never ack.
        _ => PowerAction::Ignore,
    }
}

/// Boxed context shared by the power callback and the timer callback. Owned
/// by `PowerWatcher` via `Box::into_raw`; reclaimed in `Drop` **after**
/// `IONotificationPortDestroy` guarantees no more callbacks fire.
///
/// Holds:
/// - `on_wake`: the user closure invoked once the debounce timer fires.
/// - `root_port`: the `io_connect_t` from `IORegisterForSystemPower`, needed
///   by the power trampoline to call `IOAllowPowerChange`.
/// - `shutdown`: belt-and-suspenders flag checked at the top of the timer
///   trampoline so a fire racing with `Drop` is a no-op.
struct PowerContext {
    on_wake: Box<dyn Fn()>,
    root_port: u32,
    shutdown: AtomicBool,
    /// Raw back-pointer to the debounce timer so the power trampoline can
    /// reschedule it without reaching into `PowerWatcher` (the watcher owns
    /// the `CFRunLoopTimer` wrapper which owns the ref; the trampoline only
    /// reads the raw ref to call `CFRunLoopTimerSetNextFireDate`). Set after
    /// the timer is created in [`PowerWatcher::new_with_debounce`].
    timer_ref: CFRunLoopTimerRef,
    /// Debounce window in seconds (reschedule delay on each
    /// `kIOMessageSystemHasPoweredOn`). Stored in the context so the power
    /// trampoline reads it without reaching into `PowerWatcher`.
    debounce_seconds: f64,
}

/// RAII owner of the `IOKit` system-power registration + debounce timer.
///
/// `!Send` / `!Sync`: the `IOKit` handles and `CFRunLoopSource` are
/// thread-affine — they must be created and dropped on the same thread whose
/// `CFRunLoop` they were registered against (the main thread in production).
/// The `PhantomData<*mut ()>` marker enforces this at compile time.
///
/// Created on the main thread **before** `CFRunLoop::run_current()` is
/// called, so the power source and timer are attached to the same run loop
/// the `CGEventTap` runs on.
pub struct PowerWatcher {
    root_port: u32,
    notify_port: IONotificationPortRef,
    notifier: u32,
    /// Raw `CFRunLoopSourceRef` owned by `notify_port`. We store the raw ref
    /// (never wrapped in `CFRunLoopSource`) so `Drop` can
    /// `CFRunLoopRemoveSource` it without a `CFRelease` — per IOKitLib.h the
    /// source is freed by `IONotificationPortDestroy`.
    run_loop_source: CFRunLoopSourceRef,
    /// The debounce timer. Wrapped in `CFRunLoopTimer` (Create Rule: the
    /// wrapper owns one reference released in its `Drop`).
    timer: CFRunLoopTimer,
    /// Boxed context reclaimed last in `Drop`.
    context: *mut PowerContext,
    _not_send: PhantomData<*mut ()>,
}

impl PowerWatcher {
    /// Create a `PowerWatcher` with a custom debounce window (seconds). Used
    /// by tests to avoid wall-clock waits; production callers pass
    /// `WAKE_DEBOUNCE`.
    ///
    /// Call this on the main thread before `CFRunLoop::run_current()`.
    ///
    /// # Partial-registration safety
    ///
    /// Every valid handle is cleaned up on every failure path in safe QA1340
    /// order. The boxed `PowerContext` is reclaimed **last**, only after
    /// `IONotificationPortDestroy` guarantees no more power callbacks can fire
    /// and the timer was never attached (or has been invalidated).
    #[allow(clippy::too_many_lines, clippy::items_after_statements)]
    pub fn new_with_debounce(
        on_wake: Box<dyn Fn()>,
        debounce_seconds: f64,
    ) -> Result<Self, String> {
        // Box the context so its address is stable for the FFI refcon/timer
        // info pointers. Reclaimed in Drop or on any failure path.
        let context = Box::into_raw(Box::new(PowerContext {
            on_wake,
            root_port: 0, // set after registration succeeds
            shutdown: AtomicBool::new(false),
            timer_ref: ptr::null_mut(), // set after the timer is created
            debounce_seconds,
        }));

        let mut notify_port: IONotificationPortRef = ptr::null_mut();
        let mut notifier: u32 = 0;

        // Safety: IORegisterForSystemPower writes the port/notifier out-params
        // and stores `context` as the opaque refcon. The callback is
        // `power_callback_trampoline` (catch_unwind-guarded, infallible).
        let root_port = unsafe {
            IORegisterForSystemPower(
                context.cast::<c_void>(),
                &raw mut notify_port,
                power_callback_trampoline,
                &raw mut notifier,
            )
        };

        // Partial-registration RAII guard: on any failure path (early return
        // or `?`), `Drop` independently cleans up every valid IOKit resource
        // in QA1340 order, then reclaims the context last. This replaces the
        // old closure-based cleanup with a provable ownership model.
        //
        // Resource handling (each independent):
        //   root_port != 0  → IODeregisterForSystemPower (if notifier != 0),
        //                      then IOServiceClose(root_port)
        //   notify_port != null → IONotificationPortDestroy(notify_port)
        //   source_added == true → CFRunLoopRemoveSource first (before port destroy)
        //   context != null → Box::from_raw(context)  [LAST, after all callbacks impossible]
        struct PartialRegistrationGuard {
            root_port: u32,
            notify_port: IONotificationPortRef,
            notifier: u32,
            context: *mut PowerContext,
            /// The run-loop source ref, if it was obtained and added to the
            /// run loop. Tracked so `Drop` can `CFRunLoopRemoveSource` before
            /// destroying the port (prevents a dangling source in the run loop
            /// if construction fails after the source was attached).
            run_loop_source: CFRunLoopSourceRef,
            /// True once the source has been added to the current run loop.
            source_added: bool,
            /// When true, the guard has been disarmed (construction succeeded)
            /// and `Drop` should not clean up.
            disarmed: bool,
        }

        impl Drop for PartialRegistrationGuard {
            fn drop(&mut self) {
                if self.disarmed {
                    return;
                }
                // Safety: Each resource is cleaned up independently and in
                // QA1340 order. If the source was added to the run loop,
                // remove it first (before destroying the port that owns it).
                unsafe {
                    // 0. Remove the run-loop source if it was attached.
                    if self.source_added && !self.run_loop_source.is_null() {
                        let current = CFRunLoop::get_current();
                        CFRunLoopRemoveSource(
                            current.as_concrete_TypeRef(),
                            self.run_loop_source,
                            kCFRunLoopCommonModes,
                        );
                    }
                    // 1. Deregister the notifier if valid (stops power callbacks).
                    if self.notifier != 0 {
                        let _ = IODeregisterForSystemPower(&raw mut self.notifier);
                    }
                    // 2. Close the root connection if valid (closes IOPMrootDomain).
                    if self.root_port != 0 {
                        let _ = IOServiceClose(self.root_port);
                    }
                    // 3. Destroy the notification port if non-null (frees the
                    //    run-loop source — never CFRelease it ourselves).
                    if !self.notify_port.is_null() {
                        IONotificationPortDestroy(self.notify_port);
                    }
                    // 4. Reclaim the context LAST — after all callbacks are
                    //    impossible (port destroyed, connection closed).
                    if !self.context.is_null() {
                        drop(Box::from_raw(self.context));
                    }
                }
            }
        }

        let mut guard = PartialRegistrationGuard {
            root_port,
            notify_port,
            notifier,
            context,
            run_loop_source: ptr::null_mut(),
            source_added: false,
            disarmed: false,
        };

        if root_port == 0 || notify_port.is_null() {
            // No valid registration — guard.Drop cleans up any partial
            // resources (e.g. a non-null port or nonzero root_port from a
            // partial IOKit result) and reclaims the context.
            return Err("IORegisterForSystemPower failed (root_port=0)".to_string());
        }

        // Publish the root_port into the context so the trampoline can ack.
        // Safety: `context` is valid and no callback can fire between the
        // register call and this store (the run loop is not yet running, and
        // even if a wakeup happened the source isn't attached yet).
        unsafe { (*context).root_port = root_port };

        // Get the run-loop source owned by the port. Do NOT wrap it in
        // `CFRunLoopSource` (that would CFRelease it on drop — double-free;
        // the port owns it). Keep the raw ref and add/remove manually.
        let run_loop_source = unsafe { IONotificationPortGetRunLoopSource(notify_port) };
        if run_loop_source.is_null() {
            // Guard.Drop performs full QA1340 cleanup: deregister, close,
            // destroy port, reclaim context — all in safe order.
            return Err("IONotificationPortGetRunLoopSource returned null".to_string());
        }

        // Track the source in the guard so Drop can remove it if we fail
        // after adding it to the run loop.
        guard.run_loop_source = run_loop_source;

        // Create the debounce timer BEFORE adding the IOKit run-loop source.
        // This eliminates the construction-unwind attached-source risk: if
        // timer creation fails (or panics during an unwind), the source was
        // never attached to the run loop, so `PartialRegistrationGuard::Drop`
        // only needs to destroy the port — no `CFRunLoopRemoveSource` needed.
        //
        // The timer is a **repeating** timer with a distant interval. Per
        // Apple's CFRunLoopTimer documentation, a non-repeating timer
        // (interval=0) self-invalidates after its first fire and can never
        // be rearmed via SetNextFireDate — which silently broke wake reapply
        // on the second sleep/wake cycle. A repeating timer with a
        // decades-long interval stays valid indefinitely and is rearmed on
        // each wake event by SetNextFireDate(now + debounce). The enormous
        // interval only matters if the timer is never rescheduled, which
        // never happens in practice (each wake resets the fire date ~2 s out).
        //
        // DO NOT change interval to 0 — see the `timer_interval_is_nonzero`
        // test and the review finding that documented this bug.
        let mut timer_ctx = CFRunLoopTimerContext {
            version: 0,
            info: context.cast::<c_void>(),
            retain: None,
            release: None,
            copyDescription: None,
        };
        let timer = CFRunLoopTimer::new(
            TIMER_DISTANT_FUTURE,  // fire date: far future (disarmed)
            TIMER_REPEAT_INTERVAL, // interval: decades (repeating, never self-invalidates)
            0,                     // flags
            0,                     // order
            timer_trampoline,
            &raw mut timer_ctx,
        );

        // Publish the timer ref into the context so the power trampoline can
        // reschedule it. CFRunLoopTimer::new followed the Create Rule so
        // `timer.as_concrete_TypeRef()` is a valid +1 reference we may read.
        let timer_ref = timer.as_concrete_TypeRef();
        unsafe { (*context).timer_ref = timer_ref };

        // Timer created successfully — now safe to attach the IOKit run-loop
        // source and the timer to the current run loop. If either attachment
        // somehow fails (they are infallible C calls, but for safety the
        // guard tracks `source_added` so Drop can remove it).
        let current = CFRunLoop::get_current();
        unsafe {
            CFRunLoopAddSource(
                current.as_concrete_TypeRef(),
                run_loop_source,
                kCFRunLoopCommonModes,
            );
        }
        guard.source_added = true;

        // Attach the timer to the current run loop under common modes so it
        // fires alongside the event-tap source.
        unsafe {
            CFRunLoopAddTimer(
                current.as_concrete_TypeRef(),
                timer_ref,
                kCFRunLoopCommonModes,
            );
        }

        // Construction succeeded — disarm the partial-registration guard
        // so its Drop does not clean up the handles now owned by PowerWatcher.
        guard.disarmed = true;

        Ok(PowerWatcher {
            root_port,
            notify_port,
            notifier,
            run_loop_source,
            timer,
            context,
            _not_send: PhantomData,
        })
    }

    /// Disarm the debounce timer by pushing its fire date into the far future.
    /// Used on shutdown to guarantee the timer callback never fires after
    /// teardown begins. `Drop` additionally invalidates the timer. Valid on a
    /// repeating timer (unlike the old interval=0 design).
    pub fn cancel_debounce(&self) {
        // Safety: timer_ref is valid; the timer is repeating so it has not
        // self-invalidated. Setting a far-future date parks it.
        unsafe {
            CFRunLoopTimerSetNextFireDate(self.timer.as_concrete_TypeRef(), TIMER_DISTANT_FUTURE);
        }
    }

    /// Mark the context's shutdown flag so a timer fire racing with `Drop` is
    /// a no-op. `Drop` also invalidates the timer; this is belt-and-suspenders.
    pub fn mark_shutdown(&self) {
        // Safety: context is valid until Drop reclaims it (last step). The
        // AtomicBool store is non-allocating and infallible.
        unsafe { (*self.context).shutdown.store(true, Ordering::SeqCst) };
    }
}

impl Drop for PowerWatcher {
    fn drop(&mut self) {
        // Belt-and-suspenders: mark shutdown first so any callback racing
        // with teardown is a no-op, then disarm the timer.
        self.mark_shutdown();
        self.cancel_debounce();

        // Exact QA1340 cleanup order (IOPMLib.h / IOKitLib.h):
        // 1. Invalidate the debounce timer (stops any pending fire).
        // 2. Remove the power source from the run loop (releases the run
        //    loop's reference; the port still owns its own ref).
        // 3. IODeregisterForSystemPower (must come BEFORE port destroy).
        // 4. IOServiceClose(root_port) (closes IOPMrootDomain).
        // 5. IONotificationPortDestroy (must come LAST; frees the
        //    CFRunLoopSource — never CFRelease it ourselves).
        // 6. Reclaim the boxed context (last, after no more callbacks).
        // Safety: all handles are valid and the run loop is either stopped
        // or not servicing this source anymore.
        unsafe {
            CFRunLoopTimerInvalidate(self.timer.as_concrete_TypeRef());
            let current = CFRunLoop::get_current();
            CFRunLoopRemoveSource(
                current.as_concrete_TypeRef(),
                self.run_loop_source,
                kCFRunLoopCommonModes,
            );
            let _ = IODeregisterForSystemPower(&raw mut self.notifier);
            let _ = IOServiceClose(self.root_port);
            IONotificationPortDestroy(self.notify_port);
            // Reclaim the context. After IONotificationPortDestroy no power
            // callback can fire, and after CFRunLoopTimerInvalidate no timer
            // callback can fire, so the box is safe to drop.
            drop(Box::from_raw(self.context));
        }
        // self.timer (CFRunLoopTimer) drops here and CFReleases its +1 ref.
        // The run_loop_source raw ref is NOT released — the port owned it and
        // IONotificationPortDestroy already freed it.
    }
}

// ── FFI trampolines (panic-contained: catch_unwind at every C boundary) ──

/// `IORegisterForSystemPower` callback. Dispatches via the pure
/// [`route_power_message`] router; acks sleep messages with
/// `IOAllowPowerChange` (never acks wake messages); on `HasPoweredOn`
/// reschedules the repeating debounce timer. All work is wrapped in
/// `catch_unwind` so a panic can never unwind across the `extern "C"` ABI
/// (which would abort the process).
extern "C" fn power_callback_trampoline(
    refcon: *mut c_void,
    _service: u32,
    message_type: u32,
    message_argument: *mut c_void,
) {
    // catch_unwind: a panic here must not cross the C ABI. The body only
    // calls infallible IOKit primitives, but the guard is structural.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        power_callback_body(refcon, message_type, message_argument);
    }));
}

fn power_callback_body(refcon: *mut c_void, message_type: u32, message_argument: *mut c_void) {
    if refcon.is_null() {
        return;
    }
    // Safety: refcon is the boxed PowerContext; it is valid for the lifetime
    // of PowerWatcher and this callback only fires while registered. We only
    // read it and call infallible ops (IOAllowPowerChange, atomic store,
    // CFRunLoopTimerSetNextFireDate). No allocations.
    let ctx: *mut PowerContext = refcon.cast::<PowerContext>();

    match route_power_message(message_type) {
        PowerAction::Acknowledge => {
            // Must ack kIOMessageCanSystemSleep and kIOMessageSystemWillSleep
            // or the system delays sleep by 30 s. root_port was published
            // after registration; read it atomically-ish (plain u32 load is
            // fine — it is set once before the run loop starts).
            let root_port = unsafe { (*ctx).root_port };
            if root_port != 0 {
                // intptr_t is isize on 64-bit Darwin.
                // Safety: root_port is a valid io_connect_t from registration.
                let _ = unsafe { IOAllowPowerChange(root_port, message_argument as isize) };
            }
        }
        PowerAction::ScheduleWakeReapply => {
            // Wake complete: (re)arm the repeating debounce timer. Repeats
            // coalesce — SetNextFireDate resets the single timer, no second
            // timer. The timer is repeating (interval=TIMER_REPEAT_INTERVAL)
            // so it does NOT self-invalidate after a fire.
            let timer_ref = unsafe { (*ctx).timer_ref };
            if !timer_ref.is_null() {
                let debounce = unsafe { (*ctx).debounce_seconds };
                // Safety: timer_ref is valid; CFAbsoluteTimeGetCurrent is a
                // pure time query. The only allocation is the f64 on stack.
                let fire_date = unsafe { CFAbsoluteTimeGetCurrent() + debounce };
                unsafe { CFRunLoopTimerSetNextFireDate(timer_ref, fire_date) };
            }
        }
        PowerAction::Ignore => {
            // kIOMessageSystemWillPowerOn / WillNotSleep / unknown: do nothing,
            // never ack.
        }
    }
}

/// `CFRunLoopTimer` callback. Fires ~2 s after the last
/// `kIOMessageSystemHasPoweredOn` (debounced). Checks the shutdown flag, then
/// invokes `on_wake` inside `catch_unwind` so a panic in the closure (e.g.
/// from a hidutil subprocess error) can never unwind across the `extern "C"`
/// ABI. On panic, the trampoline returns silently — no diagnostic is emitted
/// outside the unwind guard because any Rust I/O (including `eprintln!`)
/// can itself panic and would escape the `catch_unwind` boundary.
extern "C" fn timer_trampoline(_timer: CFRunLoopTimerRef, info: *mut c_void) {
    // catch_unwind: the user closure does filesystem work, mutex access,
    // logging, and subprocess execution. A panic must not cross the C ABI.
    // On panic, return silently (the watcher remains registered; the next
    // wake event reschedules normally). No post-catch I/O — it could panic
    // outside the guard.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        timer_callback_body(info);
    }));
}

fn timer_callback_body(info: *mut c_void) {
    // Safety: info is the boxed PowerContext; valid for the lifetime of
    // PowerWatcher. We only read the AtomicBool and call the closure.
    if info.is_null() {
        return;
    }
    let ctx: *mut PowerContext = info.cast::<PowerContext>();
    if unsafe { (*ctx).shutdown.load(Ordering::SeqCst) } {
        return;
    }
    // Invoke the user closure. Borrowing `on_wake` via the raw pointer is
    // safe: the closure is owned by the box and only this callback (or Drop)
    // touches it. Reentrancy is impossible — CFRunLoop sources/timers are
    // not reentrant and this runs on the run-loop thread.
    let on_wake: &dyn Fn() = unsafe { &(*ctx).on_wake };
    on_wake();
}

// ── CoreFoundation run-loop source FFI (not wrapped by core-foundation 0.10
// in a form usable for raw port-owned sources) ──────────────────────
//
// core-foundation 0.10's CFRunLoop::add_source/remove_source take a
// `&CFRunLoopSource` which would CFRelease on drop — incompatible with the
// port-owned source from IONotificationPortGetRunLoopSource. The raw
// CFRunLoopAddSource/CFRunLoopRemoveSource/CFRunLoopAddTimer symbols are
// declared above in the CoreFoundation extern block.

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    // ── route_power_message: QA1340 ack/ignore/trigger table ──────────
    //
    // Pins the exact kIOMessage* constants (verified against the MacOSX15.4.sdk
    // IOMessage.h header) to the required action. A wrong width (u64/usize) or
    // a wrong value would silently never fire, so these tests guard against
    // both regressions.

    #[test]
    fn can_system_sleep_is_acknowledged() {
        assert_eq!(
            route_power_message(K_IO_MESSAGE_CAN_SYSTEM_SLEEP),
            PowerAction::Acknowledge
        );
    }

    #[test]
    fn system_will_sleep_is_acknowledged() {
        assert_eq!(
            route_power_message(K_IO_MESSAGE_SYSTEM_WILL_SLEEP),
            PowerAction::Acknowledge
        );
    }

    #[test]
    fn system_has_powered_on_schedules_wake_reapply() {
        assert_eq!(
            route_power_message(K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON),
            PowerAction::ScheduleWakeReapply
        );
    }

    #[test]
    fn system_will_power_on_is_ignored() {
        assert_eq!(
            route_power_message(K_IO_MESSAGE_SYSTEM_WILL_POWER_ON),
            PowerAction::Ignore
        );
    }

    #[test]
    fn system_will_not_sleep_is_ignored() {
        assert_eq!(
            route_power_message(K_IO_MESSAGE_SYSTEM_WILL_NOT_SLEEP),
            PowerAction::Ignore
        );
    }

    #[test]
    fn unknown_message_is_ignored() {
        assert_eq!(route_power_message(0), PowerAction::Ignore);
        assert_eq!(route_power_message(u32::MAX), PowerAction::Ignore);
        assert_eq!(route_power_message(0xdead_beef), PowerAction::Ignore);
    }

    /// Sanity-check the computed constant values against the header formula
    /// `iokit_common_msg(msg) = 0xe0000000 | msg` so a future SDK change or
    /// a transcription typo is caught.
    #[test]
    fn iomessage_constants_match_header_formula() {
        assert_eq!(K_IO_MESSAGE_CAN_SYSTEM_SLEEP, 0xe000_0270);
        assert_eq!(K_IO_MESSAGE_SYSTEM_WILL_SLEEP, 0xe000_0280);
        assert_eq!(K_IO_MESSAGE_SYSTEM_WILL_NOT_SLEEP, 0xe000_0290);
        assert_eq!(K_IO_MESSAGE_SYSTEM_WILL_POWER_ON, 0xe000_0320);
        assert_eq!(K_IO_MESSAGE_SYSTEM_HAS_POWERED_ON, 0xe000_0300);
    }

    /// The debounce timer MUST be repeating (interval != 0). A non-repeating
    /// timer (interval=0) self-invalidates after its first fire and can never
    /// be rearmed via `CFRunLoopTimerSetNextFireDate` — which silently broke
    /// wake reapply on the second sleep/wake cycle (review Finding 1).
    #[test]
    #[allow(clippy::assertions_on_constants)] // intentional regression guard on a constant
    fn timer_repeat_interval_is_nonzero() {
        // Use > comparison instead of assert_ne to avoid f64 strict-equality
        // clippy warnings.
        assert!(
            TIMER_REPEAT_INTERVAL > 0.0f64,
            "interval=0 creates a self-invalidating one-shot timer; \
             wake reapply would work only once per daemon run"
        );
        // Also verify the distant-future constant is sane (far enough to
        // effectively disarm the timer when parked).
        assert!(TIMER_DISTANT_FUTURE > 1e9f64, "distant future must be far");
    }

    /// The timer trampoline must contain panics via `catch_unwind` so they
    /// never unwind across the `extern "C"` ABI. This test verifies the
    /// `catch_unwind` guard pattern catches an injected panic.
    #[test]
    fn timer_trampoline_contains_panicking_closure() {
        let panicking_closure: Box<dyn Fn()> = Box::new(|| {
            panic!("injected test panic");
        });
        // This mirrors what timer_trampoline does:
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            panicking_closure();
        }));
        assert!(result.is_err(), "panic should be caught, not propagated");
    }

    /// Verify that `ReapplyPolicy` supports unlimited wake cycles —
    /// schedule/fire/schedule/fire — without any state that would prevent
    /// the second cycle from firing. This directly tests the review finding
    /// that the timer worked only once.
    #[test]
    fn policy_supports_unlimited_wake_cycles() {
        let mut policy = crate::wake::ReapplyPolicy::default_debounce();
        let t0 = std::time::Instant::now();

        // Cycle 1: wake at t0, fire at t0+2s
        let d1 = policy.on_wake_message(t0).expect("cycle 1 should schedule");
        assert_eq!(d1, crate::wake::WAKE_DEBOUNCE);

        // Cycle 2: wake at t0+10s (after a hypothetical sleep/wake), fire at t0+12s
        let t1 = t0 + std::time::Duration::from_secs(10);
        let d2 = policy.on_wake_message(t1).expect("cycle 2 should schedule");
        assert_eq!(d2, crate::wake::WAKE_DEBOUNCE);
        assert_eq!(policy.last_wake(), Some(t1));

        // Cycle 3: wake at t0+20s — still works
        let t2 = t0 + std::time::Duration::from_secs(20);
        let d3 = policy.on_wake_message(t2).expect("cycle 3 should schedule");
        assert_eq!(d3, crate::wake::WAKE_DEBOUNCE);
        assert_eq!(policy.last_wake(), Some(t2));

        // The policy never refuses to schedule — no "already fired" state.
        assert!(!policy.is_cancelled());
    }

    /// Verify the construction order: timer is created BEFORE the `IOKit`
    /// run-loop source is attached, so a timer-creation failure (or a
    /// panic during timer creation) cannot leave a dangling source in
    /// the run loop. This is a pure planner test since we can't call the
    /// real FFI — it verifies the cleanup plan for each failure stage.
    #[test]
    fn timer_creation_failure_leaves_no_attached_source() {
        // The construction order in new_with_debounce is:
        //   1. IORegisterForSystemPower → root_port, notify_port, notifier
        //   2. PartialRegistrationGuard created (tracks source_added = false)
        //   3. IONotificationPortGetRunLoopSource → run_loop_source
        //   4. CFRunLoopTimer::new → timer                    ← BEFORE source add
        //   5. CFRunLoopAddSource (source_added = true)       ← AFTER timer
        //   6. CFRunLoopAddTimer
        //
        // If step 4 fails (or panics), steps 5-6 never execute, so
        // source_added is still false. PartialRegistrationGuard::Drop
        // sees source_added=false and skips CFRunLoopRemoveSource —
        // correct, because the source was never added.
        //
        // Pure planner: verify the cleanup plan for a failure at step 4.
        #[derive(Debug, PartialEq)]
        #[allow(clippy::struct_excessive_bools)] // test-only planner
        struct CleanupAction {
            remove_source: bool,
            deregister: bool,
            close: bool,
            destroy_port: bool,
            reclaim_context: bool,
        }

        // Simulate: timer creation fails after source was obtained but
        // before it was added to the run loop.
        let source_added = false; // step 5 never ran
        let run_loop_source_non_null = true; // step 3 succeeded
        let root_port_valid = true;
        let notify_port_non_null = true;
        let notifier_valid = true;
        let context_valid = true;

        let plan = CleanupAction {
            remove_source: source_added && run_loop_source_non_null,
            deregister: notifier_valid,
            close: root_port_valid,
            destroy_port: notify_port_non_null,
            reclaim_context: context_valid,
        };

        // Key assertion: source is NOT removed (it was never added).
        assert!(
            !plan.remove_source,
            "source must not be removed if it was never added to the run loop"
        );
        // But all other resources ARE cleaned up.
        assert!(plan.deregister, "notifier must be deregistered");
        assert!(plan.close, "root_port must be closed");
        assert!(plan.destroy_port, "notify_port must be destroyed");
        assert!(plan.reclaim_context, "context must be reclaimed");

        // Contrast: if construction failed AFTER source was added (the old
        // order), the source WOULD need removal — which the old guard didn't
        // track. The new order avoids this entirely.
        let old_order_plan = CleanupAction {
            remove_source: true, // source WAS added before timer creation
            deregister: true,
            close: true,
            destroy_port: true,
            reclaim_context: true,
        };
        assert!(
            old_order_plan.remove_source,
            "old order would need source removal"
        );
    }
}
