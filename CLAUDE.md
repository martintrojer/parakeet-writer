# CLAUDE.md

## Project Overview

Minimal push-to-talk transcriber using Parakeet v3. Records audio when hotkey is held, transcribes on release, outputs via wtype or clipboard.

## Build

```bash
cargo build --release
```

## Run

```bash
# Requires input group membership or sudo
./target/release/parakeet-writer
```

## Code Quality

All commits must pass:

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

Run before committing:

```bash
cargo fmt
cargo clippy --fix --allow-dirty
```

## Architecture

- `main()` - minimal entry point
- Model Management - download, verify, load Parakeet model
- Input Handling - keyboard detection, hotkey parsing
- Audio Recording - cpal-based 16kHz mono capture
- Event Loop - keyboard event processing, record/transcribe flow

## Dependencies

- `transcribe-rs` v0.2.1 (pinned) with `ort` v2.0.0-rc.10 (pinned)
- Model auto-downloads to `~/.cache/parakeet-writer/`
