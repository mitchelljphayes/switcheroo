mod config;
mod engine;
mod event_tap;
mod hidutil;
mod keycode;
mod macos_ffi;

use core_foundation::runloop::CFRunLoop;
use log::info;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

/// Whether we applied hidutil remaps and need to clean them up on exit.
static HIDUTIL_ACTIVE: AtomicBool = AtomicBool::new(false);

fn find_config() -> PathBuf {
    // Check command line argument first
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        return PathBuf::from(&args[1]);
    }

    // Check standard locations
    let candidates = [
        dirs::config_dir().map(|d| d.join("switcheroo/config.toml")),
        dirs::home_dir().map(|d| d.join(".config/switcheroo/config.toml")),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    // Fall back to current directory
    PathBuf::from("config.toml")
}

/// Install a panic hook that clears hidutil remaps before aborting.
///
/// This ensures we don't leave stale kernel-level remaps if Switcheroo
/// hits an unexpected panic.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if HIDUTIL_ACTIVE.load(Ordering::Relaxed) {
            hidutil::clear_modifier_remaps();
        }
        default_hook(info);
    }));
}

/// Spawn a background thread that listens for SIGINT/SIGTERM and stops
/// the `CFRunLoop`, allowing the main thread to proceed to cleanup.
fn install_signal_handler() -> Result<(), String> {
    let mut signals =
        Signals::new([SIGINT, SIGTERM]).map_err(|e| format!("Failed to register signals: {e}"))?;

    thread::spawn(move || {
        if let Some(sig) = signals.forever().next() {
            let name = match sig {
                SIGINT => "SIGINT",
                SIGTERM => "SIGTERM",
                _ => "unknown",
            };
            // Use eprintln for reliability — logger may not flush in signal context
            #[allow(clippy::print_stderr)]
            {
                eprintln!("\nReceived {name}, shutting down...");
            }

            // Stop the CFRunLoop on the main thread — this causes
            // CFRunLoop::run_current() to return so we can clean up.
            CFRunLoop::get_main().stop();
        }
    });

    Ok(())
}

fn run() -> Result<(), String> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    install_panic_hook();
    install_signal_handler()?;

    let config_path = find_config();
    info!("Loading config from: {}", config_path.display());

    let config = config::Config::load(&config_path)?;

    info!("Loaded {} modifier remaps", config.modifier_remaps.len());
    info!("Loaded {} remaps", config.remaps.len());
    info!("Loaded {} tap-holds", config.tap_holds.len());
    info!(
        "Loaded {} conditional remaps",
        config.conditional_remaps.len()
    );
    info!("Loaded {} chords", config.chords.len());

    // Apply kernel-level modifier remaps via hidutil before starting the event tap
    if !config.modifier_remaps.is_empty() {
        match hidutil::apply_modifier_remaps(&config.modifier_remaps) {
            Ok(()) => {
                HIDUTIL_ACTIVE.store(true, Ordering::Relaxed);
                info!(
                    "Applied {} modifier remap(s) via hidutil",
                    config.modifier_remaps.len()
                );
            }
            Err(e) => log::warn!("Failed to apply modifier remaps: {e}"),
        }
    }

    let engine = engine::Engine::new(config);

    // This blocks until a signal stops the run loop
    event_tap::run(engine)?;

    // Clean up hidutil remaps so the keyboard returns to normal
    if HIDUTIL_ACTIVE.load(Ordering::Relaxed) {
        hidutil::clear_modifier_remaps();
        HIDUTIL_ACTIVE.store(false, Ordering::Relaxed);
    }

    info!("Switcheroo stopped cleanly");
    Ok(())
}

#[allow(clippy::print_stderr)] // last-resort error reporting when logger may not be initialized
fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");

        // Best-effort cleanup even on error path
        if HIDUTIL_ACTIVE.load(Ordering::Relaxed) {
            hidutil::clear_modifier_remaps();
        }

        std::process::exit(1);
    }
}
