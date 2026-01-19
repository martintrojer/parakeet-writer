# CLAUDE.md

## Project Overview

Minimal push-to-talk transcriber using Parakeet v3. Records audio when hotkey is held, transcribes on release, outputs via typing or clipboard. Supports both Linux and macOS.

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
| X11 (keyboard) | `libX11-devel` | `libx11-dev` |
| Xi (input) | `libXi-devel` | `libxi-dev` |
| Xtst (testing) | `libXtst-devel` | `libxtst-dev` |
| ALSA (audio) | `alsa-lib-devel` | `libasound2-dev` |

**Linux Runtime Dependencies**: `wtype` and `wl-clipboard` for Wayland text output

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
- Input Handling - cross-platform hotkey parsing (rdev)
- Audio Recording - cpal-based 16kHz mono capture
- Output - platform-specific text output (osascript/pbcopy on macOS, wtype/wl-copy on Linux)
- Event Loop - keyboard event processing, record/transcribe flow

## Dependencies

- `transcribe-rs` v0.2.1 (pinned) with `ort` v2.0.0-rc.10 (pinned)
- `rdev` for cross-platform keyboard event handling
- Model auto-downloads to `~/.cache/parakeet-writer/`
