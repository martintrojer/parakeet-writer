# parakeet-writer

Minimal push-to-talk transcriber using Parakeet v3.

## Dependencies

### System packages

```bash
# Fedora
sudo dnf install alsa-lib-devel

# Debian/Ubuntu
sudo apt install libasound2-dev

# Arch
sudo pacman -S alsa-lib
```

For output, you need one of:
- `wtype` - for auto-typing (Wayland)
- `wl-clipboard` - for clipboard copy

### Rust libraries

These are pulled automatically via Cargo:
- `transcribe-rs` (parakeet feature) - Parakeet v3 transcription engine
- `evdev` - Linux keyboard input
- `cpal` - Cross-platform audio capture (requires ALSA dev library)
- `hound` - WAV file writing
- `clap` - CLI argument parsing
- `ureq`, `flate2`, `tar` - Model download and extraction

### Model

The Parakeet v3 model (~478 MB) is automatically downloaded on first run to:
```
~/.cache/parakeet-writer/parakeet-tdt-0.6b-v3-int8/
```

You can also specify a custom model path with `--model`.

## Build

```bash
cargo build --release
```

## Usage

```bash
# Run with default F9 hotkey (downloads model on first run)
./target/release/parakeet-writer

# Custom hotkey
./target/release/parakeet-writer --key ScrollLock

# Copy to clipboard instead of typing
./target/release/parakeet-writer --clipboard

# Use a custom model path
./target/release/parakeet-writer --model /path/to/model
```

### Keyboard access

Reading keyboard input requires access to `/dev/input/event*` devices. Either:

```bash
# Option 1: Add user to input group (recommended, then log out/in)
sudo usermod -aG input $USER

# Option 2: Run with sudo
sudo ./target/release/parakeet-writer
```

## Options

```
-m, --model <PATH>    Path to model directory (auto-downloads if not specified)
-k, --key <KEY>       Hotkey (F1-F12, ScrollLock, Pause, Insert) [default: F9]
    --clipboard       Copy to clipboard instead of typing
```
