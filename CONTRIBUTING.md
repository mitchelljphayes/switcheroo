# Contributing to Switcheroo

Thanks for your interest in contributing! Here's how to get started.

## Getting Started

1. Fork the repo and clone your fork
2. Install Rust via [rustup](https://rustup.rs/)
3. Build the project: `cargo build`
4. Run tests: `cargo test`

## Development

```bash
# Build and run with debug logging
RUST_LOG=debug cargo run

# Run the release build
cargo build --release
```

Switcheroo requires **Accessibility permission** on macOS to intercept keyboard events. Grant it in System Settings > Privacy & Security > Accessibility.

## Making Changes

1. Create a branch from `main`
2. Make your changes
3. Run `cargo test` and `cargo clippy` before submitting
4. Open a pull request against `main`

### Commit Messages

Use clear, concise commit messages in present tense:

- `Add tap-hold timeout configuration`
- `Fix chord detection for modifier keys`
- `Update README with new config options`

## Pull Requests

- Keep PRs focused — one feature or fix per PR
- Include a description of what changed and why
- Update the README if you're adding or changing config options

## Raycast Extension

The Raycast extension lives in `raycast-extension/`. If you're making changes there:

```bash
cd raycast-extension
npm install
npm run dev
```

## Reporting Issues

Open a GitHub issue with:

- macOS version
- What you expected vs what happened
- Your config (redact anything personal)
- Debug logs if possible (`RUST_LOG=debug switcheroo`)

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
