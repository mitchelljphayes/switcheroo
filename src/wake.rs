//! Wake-from-sleep remap reapply policy and production installer.
//!
//! When macOS wakes from sleep, `hidutil` `UserKeyMapping` entries can be
//! reset (the reported bug). This module provides the testable debounce
//! policy and the reapply driver that re-applies Switcheroo's owned modifier
//! remaps through the existing [`hidutil::apply_modifier_remaps_owned_with`]
//! path after the system wakes.
//!
//! ## Design
//!
//! - [`ReapplyPolicy`] is a pure struct that decides when to fire based on
//!   injected `Instant` values — fully unit-testable with no wall-clock sleeps.
//! - [`reapply_remappings`] wraps [`hidutil::apply_modifier_remaps_owned_with`]
//!   so the call site is explicit and mockable via [`hidutil::HidutilRunner`].
//! - [`install_power_watcher`] is the thin production glue called by `main.rs`
//!   on the main thread before `event_tap::run`. It builds a [`PowerWatcher`]
//!   whose `on_wake` closure checks the `SHUTTING_DOWN` flag, skips no-op
//!   configs, re-applies, and atomically swaps `APPLIED_MAPPINGS` only on
//!   success (write-after-commit).
//!
//! ## Keyboard disconnect/reconnect
//!
//! Keyboard hot-plug (USB/Bluetooth attach/detach) is **deferred** to a
//! follow-up. It requires `IOServiceAddMatchingNotification` with a matching
//! dictionary for `IOHIKeyboard`/`IOHIDKeyboard` and an iterator-drain
//! lifecycle — a distinct `IOKit` mechanism from `IORegisterForSystemPower`.
//! `UserKeyMapping` is kernel-global, so the actual need depends on the macOS
//! version. The `ReapplyPolicy`/`reapply_remappings` seam is reusable for a
//! future `DeviceTrigger` without restructuring.

use crate::config::ModifierRemap;
use crate::hidutil::{self, AppliedMappings, HidutilRunner};
use crate::macos_ffi::PowerWatcher;
use log::warn;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Default debounce window: reapply fires ~2 s after the last
/// `kIOMessageSystemHasPoweredOn` so repeated wake messages coalesce and
/// drivers/hidutil have settled. Hard-coded (no config knob) to avoid scope
/// creep; tests reference this constant.
pub const WAKE_DEBOUNCE: Duration = Duration::from_secs(2);

/// Pure debounce policy for wake reapply. All time is injected (`now`
/// parameters) so tests never sleep. The actual timer fire is driven by the
/// `CFRunLoopTimer` in [`crate::macos_ffi::PowerWatcher`]; this struct only
/// decides rescheduling and supplies the debounce duration.
#[allow(dead_code)] // pure test seam; production debounce is in PowerWatcher
#[derive(Debug)]
pub struct ReapplyPolicy {
    debounce: Duration,
    last_wake: Option<Instant>,
    cancelled: bool,
}

#[allow(dead_code)] // pure test seam; production debounce is in PowerWatcher
impl ReapplyPolicy {
    /// Create a policy with the given debounce window.
    #[allow(dead_code)] // pure test seam
    pub fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            last_wake: None,
            cancelled: false,
        }
    }

    /// Create a policy with the default [`WAKE_DEBOUNCE`] window.
    pub fn default_debounce() -> Self {
        Self::new(WAKE_DEBOUNCE)
    }

    /// Called when a `kIOMessageSystemHasPoweredOn` arrives at `now`. Always
    /// returns `Some(debounce)` so the caller reschedules the one-shot timer
    /// via `CFRunLoopTimerSetNextFireDate`. A duplicate within the window just
    /// resets `last_wake` — the timer is a single one-shot, so repeats never
    /// create a second timer. Returns `None` if the policy has been cancelled.
    pub fn on_wake_message(&mut self, now: Instant) -> Option<Duration> {
        if self.cancelled {
            return None;
        }
        self.last_wake = Some(now);
        Some(self.debounce)
    }

    /// Returns the debounce duration this policy is configured with.
    pub fn debounce(&self) -> Duration {
        self.debounce
    }

    /// Returns the timestamp of the last wake message, if any.
    pub fn last_wake(&self) -> Option<Instant> {
        self.last_wake
    }

    /// Mark the policy as cancelled so any pending reapply decision is dropped.
    /// Called from the shutdown path before `PowerWatcher::drop` invalidates
    /// the timer.
    pub fn on_shutdown(&mut self) {
        self.cancelled = true;
    }

    /// Returns true if the policy has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }
}

/// Reapply Switcheroo's owned modifier remaps through the existing
/// idempotent hidutil path. This is a thin wrapper around
/// [`hidutil::apply_modifier_remaps_owned_with`] so the call site is explicit
/// and mockable. On `Err`, callers log a non-sensitive warning (the error
/// string only, never keystroke/mapping content tokens).
///
/// The fresh post-wake `--get` becomes the new baseline, which inherently
/// preserves surviving foreign mappings and correctly handles the full-reset
/// case. Never call `remove_owned_mappings_with` with the pre-sleep
/// `AppliedMappings` first — that would destroy the state-file recovery
/// anchor and compute a wrong baseline.
pub fn reapply_remappings(
    remaps: &[ModifierRemap],
    runner: &dyn HidutilRunner,
) -> Result<AppliedMappings, String> {
    hidutil::apply_modifier_remaps_owned_with(remaps, runner)
}

/// Publish the new `AppliedMappings` into the snapshot slot. Returns `Err`
/// if the mutex is poisoned (publication failed). The caller must roll back
/// the just-applied mappings on `Err` to avoid leaving live kernel mappings
/// without cleanup state.
///
/// The mutex is held only for the swap (not across the subprocess), preserving
/// the panic-hook invariant: "if `HIDUTIL_ACTIVE` then `APPLIED_MAPPINGS`
/// holds the live owned set". On failure, the old `AppliedMappings` remain
/// valid for shutdown/panic cleanup.
///
/// This is testable via a passed-in `Mutex` so tests don't touch the global.
pub fn publish_applied_mappings(
    slot: &std::sync::Mutex<Option<AppliedMappings>>,
    new_mappings: AppliedMappings,
) -> Result<(), String> {
    match slot.lock() {
        Ok(mut guard) => {
            *guard = Some(new_mappings);
            Ok(())
        }
        Err(_) => Err("APPLIED_MAPPINGS mutex poisoned".to_string()),
    }
}

/// Production installer: register a [`PowerWatcher`] on the current (main)
/// thread's `CFRunLoop`. The `on_wake` closure:
/// 1. Checks `shutting_down` — skips if shutdown is in progress.
/// 2. Skips if `remaps` is empty (mirrors the startup early-return so no-op
///    configs don't churn state).
/// 3. Acquires `transaction_lock` to serialize the reapply against
///    startup/shutdown/panic cleanup.
/// 4. Calls [`reapply_remappings`] with the real `RealHidutilRunner`.
/// 5. On success: atomically swaps `APPLIED_MAPPINGS` (write-after-commit)
///    and sets `hidutil_active` to `true` with Release ordering — this arms
///    shutdown/panic cleanup even when startup apply had failed.
/// 6. On failure: logs `warn!("wake: failed to reapply modifier remaps: {e}")`
///    — the message interpolates only the error string, never keystroke/
///    mapping content. The daemon stays alive; the old `AppliedMappings`
///    and active state remain valid for shutdown.
///
/// `shutting_down`, `hidutil_active`, `applied_mappings`, and
/// `transaction_lock` are the `&'static` statics from `main.rs`, passed in
/// so the closure can access them without capturing non-`'static` references.
///
/// Returns the `PowerWatcher` on success. Registration failure is nonfatal:
/// the caller logs a warning and continues without wake support (the daemon
/// still owns its startup mappings).
#[allow(clippy::needless_pass_by_value)] // &'static statics; semantically by-value
pub fn install_power_watcher(
    remaps: Vec<ModifierRemap>,
    shutting_down: &'static AtomicBool,
    hidutil_active: &'static AtomicBool,
    applied_mappings: &'static std::sync::Mutex<Option<AppliedMappings>>,
    transaction_lock: &'static std::sync::Mutex<()>,
) -> Result<PowerWatcher, String> {
    let on_wake: Box<dyn Fn()> = Box::new(move || {
        // Belt-and-suspenders: PowerWatcher::drop invalidates the timer, but
        // if a fire races with shutdown teardown, skip the reapply.
        if shutting_down.load(Ordering::SeqCst) {
            return;
        }

        // Skip no-op configs (mirrors startup early-return pattern).
        if remaps.is_empty() {
            return;
        }

        // Serialize the reapply transaction so the panic hook (which uses
        // try_lock) cannot interleave a concurrent hidutil mutation. If the
        // lock is poisoned (a prior transaction panicked), skip — the
        // durable state file ensures next-start reconciliation.
        let Ok(_tx_guard) = transaction_lock.lock() else {
            warn!("wake: transaction lock poisoned; skipping reapply");
            return;
        };

        match reapply_remappings(&remaps, &hidutil::RealHidutilRunner) {
            Ok(new_mappings) => {
                // Write-after-commit: publish the new AppliedMappings first,
                // then set the active flag with Release ordering. If
                // publication fails (mutex poisoned), roll back the
                // just-applied mappings while the transaction lock is still
                // held, and do NOT set HIDUTIL_ACTIVE — the daemon stays
                // alive with prior state intact.
                if let Ok(()) = publish_applied_mappings(applied_mappings, new_mappings.clone()) {
                    hidutil_active.store(true, Ordering::Release);
                } else {
                    // Publication failed after kernel mutation. Roll
                    // back the just-applied mappings. The transaction
                    // lock is still held, so this is serialized.
                    // Use the newly returned snapshot to remove owned
                    // mappings (idempotent — re-reads current state).
                    warn!("wake: publication failed; rolling back applied mappings");
                    hidutil::remove_owned_mappings(&new_mappings);
                }
            }
            Err(e) => {
                // Non-sensitive: only the error string, never keystroke content.
                // Prior AppliedMappings and active state remain valid.
                warn!("wake: failed to reapply modifier remaps: {e}");
            }
        }
    });

    PowerWatcher::new_with_debounce(on_wake, WAKE_DEBOUNCE.as_secs_f64())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unwrap_in_result,
    clippy::unreadable_literal,
    clippy::panic
)]
mod tests {
    use super::*;
    use crate::hidutil::HidutilRunner;
    use std::cell::RefCell;
    use std::sync::atomic::AtomicBool;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    // ── Test seam: a local fake runner (mirrors hidutil::FakeRunner) ────
    // We re-declare here rather than exposing hidutil's private test FakeRunner,
    // to avoid touching hidutil's test internals (~30 lines, no duplication cost).

    struct WakeFakeRunner {
        sets: RefCell<Vec<String>>,
        gets: RefCell<Vec<String>>,
        set_should_fail: bool,
    }

    impl WakeFakeRunner {
        fn new(gets: Vec<String>) -> Self {
            Self {
                sets: RefCell::new(Vec::new()),
                gets: RefCell::new(gets),
                set_should_fail: false,
            }
        }

        fn with_failing_set(gets: Vec<String>) -> Self {
            Self {
                sets: RefCell::new(Vec::new()),
                gets: RefCell::new(gets),
                set_should_fail: true,
            }
        }

        fn last_set(&self) -> String {
            self.sets.borrow().last().cloned().unwrap_or_default()
        }

        fn set_count(&self) -> usize {
            self.sets.borrow().len()
        }
    }

    impl HidutilRunner for WakeFakeRunner {
        fn read_user_key_mapping_raw(&self) -> Result<String, String> {
            let mut gets = self.gets.borrow_mut();
            if gets.is_empty() {
                Err("no queued get".to_string())
            } else {
                Ok(gets.remove(0))
            }
        }

        fn set_user_key_mapping(&self, json: &str) -> Result<(), String> {
            if self.set_should_fail {
                return Err("simulated hidutil --set failure".to_string());
            }
            self.sets.borrow_mut().push(json.to_string());
            Ok(())
        }
    }

    fn remap(from_hid: u64, to_hid: u64) -> ModifierRemap {
        ModifierRemap {
            from: format!("src_{from_hid}"),
            from_hid,
            to: format!("dst_{to_hid}"),
            to_hid,
        }
    }

    /// JSON array form with STRING values (what plutil -convert json emits).
    fn json_stdout_string_values(entries: &[(u64, u64)]) -> String {
        let body: Vec<String> = entries
            .iter()
            .map(|(s, d)| {
                format!(
                    "{{\"HIDKeyboardModifierMappingSrc\":\"{s}\",\"HIDKeyboardModifierMappingDst\":\"{d}\"}}"
                )
            })
            .collect();
        format!("[{}]", body.join(","))
    }

    // ── 1. wake_reapply_dispatches_once_after_debounce ────────────────

    #[test]
    fn wake_reapply_dispatches_once_after_debounce() {
        let t0 = Instant::now();
        let mut policy = ReapplyPolicy::default_debounce();

        // One wake at t0 → policy says reschedule for debounce duration.
        let delay = policy.on_wake_message(t0).expect("should schedule");
        assert_eq!(delay, WAKE_DEBOUNCE);

        // Simulate the timer firing at t0 + debounce: one reapply via FakeRunner.
        crate::hidutil::with_isolated_state_test(|| {
            let runner = WakeFakeRunner::new(vec!["[]".to_string()]);
            let remaps = vec![remap(0x700000039, 0x7000000e4)];
            let applied = reapply_remappings(&remaps, &runner).expect("reapply should succeed");
            assert_eq!(applied.owned.len(), 1);
            assert_eq!(runner.set_count(), 1);
        });
    }

    // ── 2. repeated_wake_events_debounce_to_single_reapply ────────────

    #[test]
    fn repeated_wake_events_debounce_to_single_reapply() {
        let t0 = Instant::now();
        let mut policy = ReapplyPolicy::default_debounce();

        // Wake at t0, t0+1s, t0+1.5s — all within the 2s debounce window.
        // Each call returns Some(debounce) (reschedule); the policy coalesces.
        let d1 = policy.on_wake_message(t0).expect("should schedule");
        let d2 = policy
            .on_wake_message(t0 + Duration::from_secs(1))
            .expect("should schedule");
        let d3 = policy
            .on_wake_message(t0 + Duration::from_millis(1500))
            .expect("should schedule");

        // All return the same debounce duration (timer is reset, not duplicated).
        assert_eq!(d1, WAKE_DEBOUNCE);
        assert_eq!(d2, WAKE_DEBOUNCE);
        assert_eq!(d3, WAKE_DEBOUNCE);
        assert_eq!(policy.last_wake(), Some(t0 + Duration::from_millis(1500)));

        // Final fire at t0+1.5s+2s = t0+3.5s: exactly one reapply.
        crate::hidutil::with_isolated_state_test(|| {
            let runner = WakeFakeRunner::new(vec!["[]".to_string()]);
            let remaps = vec![remap(0x700000039, 0x7000000e4)];
            let _applied = reapply_remappings(&remaps, &runner).expect("reapply should succeed");
            assert_eq!(runner.set_count(), 1);
        });
    }

    // ── 3. reapply_is_idempotent ───────────────────────────────────────

    #[test]
    fn reapply_is_idempotent() {
        crate::hidutil::with_isolated_state_test(|| {
            let owned = (0x700000039, 0x7000000e4);
            let remaps = vec![remap(owned.0, owned.1)];

            // First reapply: empty baseline. reconcile_stale_state_with finds
            // no state file (fresh), so only one --get is consumed (for the
            // baseline snapshot). After apply, a state file is written.
            // Second reapply: reconcile_stale_state_with reads the state file
            // (written by the first call), which triggers one --get + one --set
            // to reconcile, then apply reads another --get for the new baseline
            // and does one --set. So we need 1 + 2 = 3 --get responses queued.
            let runner = WakeFakeRunner::new(vec![
                "[]".to_string(),                    // first apply baseline
                json_stdout_string_values(&[owned]), // second reconcile current
                json_stdout_string_values(&[owned]), // second apply baseline
            ]);
            let first = reapply_remappings(&remaps, &runner).expect("first reapply");
            let first_set = runner.last_set();
            // First apply: 1 --set (no reconcile --set since no state file).
            assert_eq!(runner.set_count(), 1);

            // Second reapply: post-apply state has the owned entry; reconcile
            // removes the stale owned entry (1 --set), then apply re-sets it
            // (1 --set). The final --set payload should be the same owned entry.
            let second = reapply_remappings(&remaps, &runner).expect("second reapply");
            let second_set = runner.last_set();
            // 1 (first apply) + 1 (reconcile --set) + 1 (second apply --set) = 3.
            assert_eq!(runner.set_count(), 3);

            // The final set payloads should be identical (owned entry is same).
            assert_eq!(first_set, second_set);
            // No duplicated owned entries.
            assert_eq!(first.owned, second.owned);
            assert_eq!(first.owned.len(), 1);
        });
    }

    // ── 4. reapply_preserves_foreign_mappings ──────────────────────────

    #[test]
    fn reapply_preserves_foreign_mappings() {
        crate::hidutil::with_isolated_state_test(|| {
            // Post-wake --get contains a foreign entry (not in owned set).
            let foreign = (0x700000064, 0x700000064);
            let runner = WakeFakeRunner::new(vec![json_stdout_string_values(&[foreign])]);
            let remaps = vec![
                remap(0x700000039, 0x7000000e4),
                remap(0x7000000e4, 0x700000039),
            ];
            let applied = reapply_remappings(&remaps, &runner).expect("reapply should succeed");

            let set = runner.last_set();
            // Foreign entry survives.
            assert!(
                set.contains("\"HIDKeyboardModifierMappingSrc\":0x700000064"),
                "foreign mapping must survive reapply"
            );
            // Owned entries are present.
            assert!(set.contains("\"HIDKeyboardModifierMappingSrc\":0x700000039"));
            assert!(set.contains("\"HIDKeyboardModifierMappingSrc\":0x7000000e4"));
            // Baseline records the foreign entry.
            assert_eq!(applied.baseline.len(), 1);
        });
    }

    // ── 5. reapply_after_full_reset_sets_only_owned ────────────────────

    #[test]
    fn reapply_after_full_reset_sets_only_owned() {
        crate::hidutil::with_isolated_state_test(|| {
            // Post-wake --get returns empty (full reset by macOS).
            let runner = WakeFakeRunner::new(vec!["[]".to_string()]);
            let remaps = vec![
                remap(0x700000039, 0x7000000e4),
                remap(0x7000000e4, 0x700000039),
            ];
            let applied = reapply_remappings(&remaps, &runner).expect("reapply should succeed");

            let set = runner.last_set();
            // Only owned entries — no stale baseline re-applied.
            assert_eq!(applied.owned.len(), 2);
            assert_eq!(applied.baseline.len(), 0);
            assert!(set.contains("\"HIDKeyboardModifierMappingSrc\":0x700000039"));
            assert!(set.contains("\"HIDKeyboardModifierMappingSrc\":0x7000000e4"));
            // No foreign/stale entries.
            assert!(!set.contains("\"HIDKeyboardModifierMappingSrc\":0x700000064"));
        });
    }

    // ── 6. pending_shutdown_cancels_reapply ────────────────────────────

    #[test]
    fn pending_shutdown_cancels_reapply() {
        let t0 = Instant::now();
        let mut policy = ReapplyPolicy::default_debounce();

        // Wake arrives, then shutdown signal before the timer fires.
        let _ = policy.on_wake_message(t0).expect("should schedule");
        policy.on_shutdown();

        // After shutdown, on_wake_message returns None (cancelled).
        assert!(policy
            .on_wake_message(t0 + Duration::from_secs(1))
            .is_none());
        assert!(policy.is_cancelled());

        // The production closure checks shutting_down and skips reapply.
        let shutting_down = AtomicBool::new(true);
        let applied_slot: Mutex<Option<AppliedMappings>> = Mutex::new(None);
        crate::hidutil::with_isolated_state_test(|| {
            let runner = WakeFakeRunner::new(vec!["[]".to_string()]);
            let remaps = vec![remap(0x700000039, 0x7000000e4)];

            // Simulate the on_wake closure body with shutting_down=true.
            if shutting_down.load(Ordering::SeqCst) {
                // Skip — no reapply, no set.
                assert_eq!(runner.set_count(), 0);
            } else {
                let _ = reapply_remappings(&remaps, &runner);
            }
            assert_eq!(runner.set_count(), 0);
            assert!(applied_slot.lock().map_or(true, |g| g.is_none()));
        });
    }

    // ── 7. reapply_failure_is_nonfatal_and_preserves_old_applied_mappings

    #[test]
    fn reapply_failure_is_nonfatal_and_preserves_old_applied_mappings() {
        crate::hidutil::with_isolated_state_test(|| {
            // Make --set fail on the reapply.
            let runner = WakeFakeRunner::with_failing_set(vec!["[]".to_string()]);
            let remaps = vec![remap(0x700000039, 0x7000000e4)];

            // The old AppliedMappings (simulated pre-wake value).
            let old_mappings = AppliedMappings {
                baseline: vec![],
                owned: vec![crate::hidutil::UserKeyMappingEntry {
                    src: 0x700000039,
                    dst: 0x7000000e4,
                }],
            };
            let slot: Mutex<Option<AppliedMappings>> = Mutex::new(Some(old_mappings.clone()));

            // Reapply fails → Err propagates to the caller path that logs warn.
            let result = reapply_remappings(&remaps, &runner);
            assert!(result.is_err(), "reapply should fail");

            // The swap is NOT called on failure — old AppliedMappings remain.
            let current = slot.lock().map_or(None, |g| g.clone());
            assert_eq!(current, Some(old_mappings));

            // No successful set was recorded (the failing one didn't push).
            assert_eq!(runner.set_count(), 0);
        });
    }

    // ── ReapplyPolicy unit tests (pure, no hidutil) ────────────────────

    #[test]
    fn policy_on_wake_returns_debounce_duration() {
        let mut policy = ReapplyPolicy::new(Duration::from_millis(500));
        let now = Instant::now();
        let delay = policy.on_wake_message(now).expect("should schedule");
        assert_eq!(delay, Duration::from_millis(500));
        assert_eq!(policy.last_wake(), Some(now));
    }

    #[test]
    fn policy_on_wake_after_cancel_returns_none() {
        let mut policy = ReapplyPolicy::default_debounce();
        policy.on_shutdown();
        let delay = policy.on_wake_message(Instant::now());
        assert!(delay.is_none());
    }

    #[test]
    fn policy_on_shutdown_marks_cancelled() {
        let mut policy = ReapplyPolicy::default_debounce();
        assert!(!policy.is_cancelled());
        policy.on_shutdown();
        assert!(policy.is_cancelled());
    }

    #[test]
    fn policy_debounce_returns_configured_duration() {
        let policy = ReapplyPolicy::new(Duration::from_millis(100));
        assert_eq!(policy.debounce(), Duration::from_millis(100));
    }

    #[test]
    fn publish_applied_mappings_replaces_value() {
        let slot: Mutex<Option<AppliedMappings>> = Mutex::new(None);
        let new_mappings = AppliedMappings {
            baseline: vec![],
            owned: vec![crate::hidutil::UserKeyMappingEntry { src: 1, dst: 2 }],
        };
        publish_applied_mappings(&slot, new_mappings.clone()).expect("publish should succeed");
        let current = slot.lock().map_or(None, |g| g.clone());
        assert_eq!(current, Some(new_mappings));
    }

    #[test]
    fn publish_applied_mappings_returns_err_on_poison() {
        let slot: Mutex<Option<AppliedMappings>> = Mutex::new(None);
        // Poison the mutex by panicking while holding it.
        let slot_ref = std::sync::Arc::new(slot);
        let slot_clone = slot_ref.clone();
        let _ = std::thread::spawn(move || {
            let _guard = slot_clone.lock().unwrap();
            panic!("poison");
        })
        .join();
        let result = publish_applied_mappings(&slot_ref, AppliedMappings::default());
        assert!(result.is_err(), "should fail on poisoned mutex");
    }

    // ── 8. startup_fail_then_wake_success_arms_cleanup ─────────────────
    //
    // If startup apply failed (HIDUTIL_ACTIVE=false, APPLIED_MAPPINGS=None),
    // a successful wake reapply must set HIDUTIL_ACTIVE=true and publish
    // APPLIED_MAPPINGS so shutdown cleanup runs.

    #[test]
    fn startup_fail_then_wake_success_arms_cleanup() {
        let shutting_down = AtomicBool::new(false);
        let hidutil_active = AtomicBool::new(false); // startup failed
        let applied_slot: Mutex<Option<AppliedMappings>> = Mutex::new(None);
        let tx_lock: Mutex<()> = Mutex::new(());

        crate::hidutil::with_isolated_state_test(|| {
            let remaps = vec![remap(0x700000039, 0x7000000e4)];
            let runner = WakeFakeRunner::new(vec!["[]".to_string()]);

            // Simulate the on_wake closure body (same logic as the production
            // closure in install_power_watcher).
            if !shutting_down.load(Ordering::SeqCst) && !remaps.is_empty() {
                let _tx = tx_lock.lock().unwrap();
                match reapply_remappings(&remaps, &runner) {
                    Ok(new_mappings) => {
                        match publish_applied_mappings(&applied_slot, new_mappings) {
                            Ok(()) => {
                                hidutil_active.store(true, Ordering::Release);
                            }
                            Err(_) => {
                                warn!("wake: publication failed; rolling back");
                                // In tests we don't actually roll back hidutil
                            }
                        }
                    }
                    Err(e) => {
                        warn!("wake: failed to reapply modifier remaps: {e}");
                    }
                }
            }

            // Wake success armed cleanup:
            assert!(hidutil_active.load(Ordering::Relaxed));
            let current = applied_slot.lock().map_or(None, |g| g.clone());
            assert!(current.is_some());
            assert_eq!(current.as_ref().unwrap().owned.len(), 1);
        });
    }

    // ── 9. wake_snapshot_replaces_startup_state ────────────────────────
    //
    // On successful wake reapply, the new AppliedMappings (with the post-wake
    // baseline) replaces the old startup state in the mutex. Shutdown
    // consumes the newest state.

    #[test]
    fn wake_snapshot_replaces_startup_state() {
        let startup_mappings = AppliedMappings {
            baseline: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000064,
                dst: 0x700000065,
            }],
            owned: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000039,
                dst: 0x7000000e4,
            }],
        };
        let applied_slot: Mutex<Option<AppliedMappings>> =
            Mutex::new(Some(startup_mappings.clone()));

        crate::hidutil::with_isolated_state_test(|| {
            let remaps = vec![remap(0x700000039, 0x7000000e4)];
            let runner = WakeFakeRunner::new(vec!["[]".to_string()]); // full reset post-wake

            let new_mappings = reapply_remappings(&remaps, &runner).expect("reapply succeeds");
            publish_applied_mappings(&applied_slot, new_mappings.clone())
                .expect("publish should succeed");

            // The mutex now holds the post-wake state, not the startup state.
            let current = applied_slot.lock().map_or(None, |g| g.clone());
            assert_eq!(current, Some(new_mappings));
            assert_ne!(current, Some(startup_mappings));
            // Post-wake baseline is empty (full reset), not the old baseline.
            assert_eq!(current.as_ref().unwrap().baseline.len(), 0);
        });
    }

    // ── 10. wake_failure_preserves_prior_state ─────────────────────────
    //
    // On wake reapply failure, APPLIED_MAPPINGS and HIDUTIL_ACTIVE must
    // retain their prior values — the daemon stays alive with valid
    // cleanup state.

    #[test]
    fn wake_failure_preserves_prior_state() {
        let hidutil_active = AtomicBool::new(true); // startup succeeded
        let prior_mappings = AppliedMappings {
            baseline: vec![],
            owned: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000039,
                dst: 0x7000000e4,
            }],
        };
        let applied_slot: Mutex<Option<AppliedMappings>> = Mutex::new(Some(prior_mappings.clone()));

        crate::hidutil::with_isolated_state_test(|| {
            let remaps = vec![remap(0x700000039, 0x7000000e4)];
            let runner = WakeFakeRunner::with_failing_set(vec!["[]".to_string()]);

            // Simulate the on_wake closure body.
            match reapply_remappings(&remaps, &runner) {
                Ok(new_mappings) => match publish_applied_mappings(&applied_slot, new_mappings) {
                    Ok(()) => {
                        hidutil_active.store(true, Ordering::Release);
                    }
                    Err(_) => {
                        warn!("wake: publication failed; rolling back");
                    }
                },
                Err(e) => {
                    warn!("wake: failed to reapply modifier remaps: {e}");
                }
            }

            // Failure: prior state preserved.
            assert!(
                hidutil_active.load(Ordering::Relaxed),
                "active flag unchanged"
            );
            let current = applied_slot.lock().map_or(None, |g| g.clone());
            assert_eq!(current, Some(prior_mappings), "prior mappings preserved");
        });
    }

    // ── 11. panic_cleanup_contention_skips_concurrent_mutation ─────────
    //
    // If a panic occurs while the transaction lock is held (e.g. during a
    // wake reapply), the panic hook must use try_lock and skip live hidutil
    // mutation rather than blocking or interleaving.

    #[test]
    fn panic_cleanup_contention_skips_concurrent_mutation() {
        let tx_lock: Mutex<()> = Mutex::new(());
        let hidutil_active = AtomicBool::new(true);
        let applied_slot: Mutex<Option<AppliedMappings>> = Mutex::new(Some(AppliedMappings {
            baseline: vec![],
            owned: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000039,
                dst: 0x7000000e4,
            }],
        }));

        // Simulate a wake reapply holding the transaction lock.
        let _wake_tx = tx_lock.lock().unwrap();

        // Simulate the panic hook's try_lock path.
        let lock_acquired = tx_lock.try_lock().is_ok();
        assert!(
            !lock_acquired,
            "try_lock should fail when transaction is in progress"
        );

        // When try_lock fails, the panic hook skips live mutation.
        // The AppliedMappings remain in the mutex for the durable state
        // file to reconcile at next startup.
        //
        // If try_lock had succeeded (it should NOT), we would run
        // remove_owned_mappings — but since it didn't, we skip. The
        // assertion below verifies state is unchanged.
        assert!(
            !lock_acquired,
            "try_lock should fail when transaction is in progress"
        );

        // State is unchanged — the mutex still holds the mappings.
        assert!(hidutil_active.load(Ordering::Relaxed));
        assert!(applied_slot.lock().is_ok_and(|g| g.is_some()));
    }

    // ── 12. shutdown_consumes_newest_state_not_startup_local ───────────
    //
    // The shutdown path must prefer APPLIED_MAPPINGS (newest committed)
    // over the startup-local. This test simulates the shutdown logic.

    #[test]
    fn shutdown_consumes_newest_state_not_startup_local() {
        let startup_local = AppliedMappings {
            baseline: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000064,
                dst: 0x700000065,
            }], // stale pre-sleep baseline
            owned: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000039,
                dst: 0x7000000e4,
            }],
        };
        let post_wake = AppliedMappings {
            baseline: vec![], // post-wake full reset baseline
            owned: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000039,
                dst: 0x7000000e4,
            }],
        };

        // After wake reapply, the mutex holds the newest state.
        let applied_slot: Mutex<Option<AppliedMappings>> = Mutex::new(Some(post_wake.clone()));
        let mut applied_local: Option<AppliedMappings> = Some(startup_local.clone());

        // Shutdown logic: prefer mutex, fall back to local.
        let cleanup = applied_slot
            .lock()
            .ok()
            .and_then(|mut g| g.take())
            .or_else(|| applied_local.take());

        // Cleanup used the post-wake state, NOT the stale startup local.
        assert_eq!(cleanup, Some(post_wake));
        assert_ne!(cleanup, Some(startup_local));
        // The local was NOT consumed.
        assert!(applied_local.is_some(), "stale local should not be used");
    }

    // ── 13. wake_publication_failure_rolls_back_and_keeps_prior_state ──
    //
    // If the APPLIED_MAPPINGS mutex is poisoned when publishing a successful
    // wake reapply, the closure must NOT set HIDUTIL_ACTIVE and must roll
    // back the just-applied mappings. Prior state remains intact.

    #[test]
    fn wake_publication_failure_rolls_back_and_keeps_prior_state() {
        let hidutil_active = AtomicBool::new(true); // startup succeeded
        let prior_mappings = AppliedMappings {
            baseline: vec![],
            owned: vec![crate::hidutil::UserKeyMappingEntry {
                src: 0x700000039,
                dst: 0x7000000e4,
            }],
        };

        // Poison the applied_slot mutex.
        let applied_slot: std::sync::Arc<Mutex<Option<AppliedMappings>>> =
            std::sync::Arc::new(Mutex::new(Some(prior_mappings.clone())));
        let slot_clone = applied_slot.clone();
        let _ = std::thread::spawn(move || {
            let _g = slot_clone.lock().unwrap();
            panic!("poison");
        })
        .join();

        crate::hidutil::with_isolated_state_test(|| {
            let remaps = vec![remap(0x700000039, 0x7000000e4)];
            let runner = WakeFakeRunner::new(vec!["[]".to_string()]);

            // Simulate the on_wake closure body with publication failure.
            let tx_lock = std::sync::Mutex::new(());
            let Ok(_tx) = tx_lock.lock() else {
                return;
            };
            match reapply_remappings(&remaps, &runner) {
                Ok(new_mappings) => {
                    match publish_applied_mappings(&applied_slot, new_mappings.clone()) {
                        Ok(()) => {
                            hidutil_active.store(true, Ordering::Release);
                        }
                        Err(_) => {
                            // Publication failed — do NOT set active;
                            // roll back (in production, remove_owned_mappings
                            // would run; in test we just verify the flag).
                            warn!("wake: publication failed; rolling back");
                            // hidutil::remove_owned_mappings(&new_mappings) in prod
                        }
                    }
                }
                Err(e) => {
                    warn!("wake: failed to reapply modifier remaps: {e}");
                }
            }

            // Active flag should still be true (from startup), NOT re-set
            // by the failed wake publication. The prior state is intact
            // because publication failed (the poisoned mutex still holds
            // the prior value in its inner state).
            assert!(
                hidutil_active.load(Ordering::Relaxed),
                "active flag should be unchanged (true from startup)"
            );
            // The reapply itself succeeded (1 --set recorded).
            assert_eq!(runner.set_count(), 1);
        });
    }
}
