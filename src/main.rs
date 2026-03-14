mod config;
mod engine;
mod event_tap;
mod hidutil;
mod keycode;

use log::info;
use std::path::PathBuf;

fn find_config() -> PathBuf {
    // Check command line argument first
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        return PathBuf::from(&args[1]);
    }

    // Check standard locations
    let candidates = [
        dirs::config_dir().map(|d| d.join("keytap/config.toml")),
        dirs::home_dir().map(|d| d.join(".config/keytap/config.toml")),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    // Fall back to current directory
    PathBuf::from("config.toml")
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_millis()
        .init();

    let config_path = find_config();
    info!("Loading config from: {}", config_path.display());

    let config = match config::Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    info!("Loaded {} modifier remaps", config.modifier_remaps.len());
    info!("Loaded {} tap-holds", config.tap_holds.len());
    info!(
        "Loaded {} conditional remaps",
        config.conditional_remaps.len()
    );
    info!("Loaded {} chords", config.chords.len());

    // Apply kernel-level modifier remaps via hidutil before starting the event tap
    if !config.modifier_remaps.is_empty() {
        match hidutil::apply_modifier_remaps(&config.modifier_remaps) {
            Ok(()) => info!(
                "Applied {} modifier remap(s) via hidutil",
                config.modifier_remaps.len()
            ),
            Err(e) => eprintln!("Warning: failed to apply modifier remaps: {e}"),
        }
    }

    let engine = engine::Engine::new(config);

    if let Err(e) = event_tap::run(engine) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
