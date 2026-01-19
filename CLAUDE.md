# CLAUDE.md

## Project Overview

Minimal push-to-talk transcriber using Parakeet v3. Records audio when hotkey is held, transcribes on release, outputs via typing or clipboard. Supports Linux (Wayland) and macOS.

## Build

```bash
cargo build --release
```

## Run

```bash
./target/release/parakeet-writer
```

### Platform Notes

**macOS**: Requires Accessibility permissions for keyboard monitoring and typing simulation (System Settings > Privacy & Security > Accessibility)

**Linux Build Dependencies**:

| Purpose | Fedora | Debian/Ubuntu |
|---------|--------|---------------|
| ALSA (audio) | `alsa-lib-devel` | `libasound2-dev` |

**Linux Runtime Dependencies**: `wtype` and `wl-clipboard` for Wayland text output

**Linux Keyboard Access**: Requires `/dev/input` access - add user to `input` group or run with sudo

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
- Input Handling - evdev (Linux) or rdev (macOS) for keyboard events
- Audio Recording - cpal-based 16kHz mono capture
- Output - platform-specific text output (osascript/pbcopy on macOS, wtype/wl-copy on Linux)
- Event Loop - keyboard event processing, record/transcribe flow

## Dependencies

- `transcribe-rs` v0.2.1 (pinned) with `ort` v2.0.0-rc.10 (pinned)
- `evdev` for Linux keyboard input via /dev/input
- `rdev` for macOS keyboard input
- Model auto-downloads to `~/.cache/parakeet-writer/`
