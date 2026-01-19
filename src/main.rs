use anyhow::{Context, Result};
use clap::Parser;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use evdev::{Device, InputEventKind, Key};
use flate2::read::GzDecoder;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tar::Archive;
use transcribe_rs::{
    engines::parakeet::{ParakeetEngine, ParakeetModelParams},
    TranscriptionEngine,
};

const MODEL_URL: &str = "https://blob.handy.computer/parakeet-v3-int8.tar.gz";
const MODEL_DIR_NAME: &str = "parakeet-tdt-0.6b-v3-int8";

#[derive(Parser)]
#[command(name = "parakeet-writer")]
#[command(about = "Push-to-talk transcriber using Parakeet v3")]
struct Args {
    /// Path to the parakeet model directory (auto-downloads if not specified)
    #[arg(short, long)]
    model: Option<PathBuf>,

    /// Hotkey to trigger recording (e.g., F9, ScrollLock)
    #[arg(short, long, default_value = "F9")]
    key: String,

    /// Copy transcription to clipboard instead of typing
    #[arg(long)]
    clipboard: bool,
}

// ============================================================================
// Model Management
// ============================================================================

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("parakeet-writer")
}

fn default_model_path() -> PathBuf {
    cache_dir().join(MODEL_DIR_NAME)
}

fn verify_model(path: &Path) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }
    let encoder = path.join("encoder-model.int8.onnx");
    let decoder = path.join("decoder_joint-model.int8.onnx");
    let vocab = path.join("vocab.txt");
    encoder.exists() && decoder.exists() && vocab.exists()
}

fn download_model(dest_dir: &Path) -> Result<()> {
    println!("Downloading Parakeet v3 model (~478 MB)...");

    std::fs::create_dir_all(dest_dir.parent().unwrap_or(dest_dir))
        .context("Failed to create cache directory")?;

    let response = ureq::get(MODEL_URL)
        .call()
        .context("Failed to start download")?;

    let total_size = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let temp_path = dest_dir.with_extension("tar.gz.tmp");
    let mut file = File::create(&temp_path).context("Failed to create temp file")?;

    let mut reader = response.into_body().into_reader();
    let mut buffer = [0u8; 8192];
    let mut downloaded: u64 = 0;
    let mut last_percent = 0;

    loop {
        let bytes_read = reader.read(&mut buffer).context("Download interrupted")?;
        if bytes_read == 0 {
            break;
        }
        file.write_all(&buffer[..bytes_read])
            .context("Failed to write to file")?;
        downloaded += bytes_read as u64;

        if total_size > 0 {
            let percent = (downloaded * 100 / total_size) as usize;
            if percent != last_percent {
                eprint!("\r[");
                for i in 0..20 {
                    if i < percent / 5 {
                        eprint!("=");
                    } else {
                        eprint!(" ");
                    }
                }
                eprint!("] {}%", percent);
                std::io::stderr().flush().ok();
                last_percent = percent;
            }
        }
    }

    eprintln!(
        "\r[+] Download complete: {:.1} MB                    ",
        downloaded as f64 / 1_000_000.0
    );

    println!("Extracting model...");
    let tar_gz = File::open(&temp_path).context("Failed to open archive")?;
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);
    archive
        .unpack(dest_dir.parent().unwrap_or(dest_dir))
        .context("Failed to extract archive")?;

    std::fs::remove_file(&temp_path).ok();
    println!("[+] Model ready!");

    Ok(())
}

fn ensure_model(model_path: Option<PathBuf>) -> Result<PathBuf> {
    let user_provided = model_path.is_some();
    let path = model_path.unwrap_or_else(default_model_path);

    if verify_model(&path) {
        return Ok(path);
    }

    if user_provided {
        anyhow::bail!("Model not found at {:?}", path);
    }

    download_model(&path)?;

    if !verify_model(&path) {
        anyhow::bail!("Model verification failed after download");
    }

    Ok(path)
}

fn load_engine(model_path: &PathBuf) -> Result<ParakeetEngine> {
    println!("Loading Parakeet model from {:?}...", model_path);
    let load_start = Instant::now();
    let mut engine = ParakeetEngine::new();
    engine
        .load_model_with_params(model_path, ParakeetModelParams::int8())
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    println!("Model loaded in {:.2?}", load_start.elapsed());
    Ok(engine)
}

// ============================================================================
// Input Handling
// ============================================================================

fn parse_hotkey(key: &str) -> Result<Key> {
    let key_upper = key.to_uppercase();
    match key_upper.as_str() {
        "F1" => Ok(Key::KEY_F1),
        "F2" => Ok(Key::KEY_F2),
        "F3" => Ok(Key::KEY_F3),
        "F4" => Ok(Key::KEY_F4),
        "F5" => Ok(Key::KEY_F5),
        "F6" => Ok(Key::KEY_F6),
        "F7" => Ok(Key::KEY_F7),
        "F8" => Ok(Key::KEY_F8),
        "F9" => Ok(Key::KEY_F9),
        "F10" => Ok(Key::KEY_F10),
        "F11" => Ok(Key::KEY_F11),
        "F12" => Ok(Key::KEY_F12),
        "SCROLLLOCK" | "SCROLL_LOCK" => Ok(Key::KEY_SCROLLLOCK),
        "PAUSE" => Ok(Key::KEY_PAUSE),
        "INSERT" => Ok(Key::KEY_INSERT),
        _ => anyhow::bail!("Unknown hotkey: {}", key),
    }
}

fn find_keyboards() -> Result<Vec<Device>> {
    let mut keyboards = Vec::new();
    for path in std::fs::read_dir("/dev/input")? {
        let path = path?.path();
        if let Some(name) = path.file_name() {
            if name.to_string_lossy().starts_with("event") {
                if let Ok(device) = Device::open(&path) {
                    if device
                        .supported_keys()
                        .is_some_and(|keys| keys.contains(Key::KEY_A))
                    {
                        log::debug!(
                            "Found keyboard: {} ({:?})",
                            device.name().unwrap_or("unknown"),
                            path
                        );
                        keyboards.push(device);
                    }
                }
            }
        }
    }
    if keyboards.is_empty() {
        anyhow::bail!("No keyboards found. Try running with sudo or add user to input group.");
    }
    Ok(keyboards)
}

// ============================================================================
// Audio Recording
// ============================================================================

struct AudioRecorder {
    samples: Arc<Mutex<Vec<i16>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
}

impl AudioRecorder {
    fn new() -> Result<Self> {
        Ok(Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            sample_rate: 16000,
        })
    }

    fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("No input device available")?;

        log::debug!("Using input device: {}", device.name()?);

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        self.samples.lock().unwrap().clear();
        let samples = Arc::clone(&self.samples);

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut samples = samples.lock().unwrap();
                for &sample in data {
                    samples.push((sample * i16::MAX as f32) as i16);
                }
            },
            |err| log::error!("Audio stream error: {}", err),
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) -> Result<PathBuf> {
        self.stream = None;
        std::thread::sleep(std::time::Duration::from_millis(100));

        let samples = self.samples.lock().unwrap();
        let temp_file = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()?
            .into_temp_path()
            .keep()?;

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let file = File::create(&temp_file)?;
        let mut writer = hound::WavWriter::new(BufWriter::new(file), spec)?;
        for &sample in samples.iter() {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;

        log::debug!(
            "Recorded {} samples ({:.2}s)",
            samples.len(),
            samples.len() as f64 / self.sample_rate as f64
        );

        Ok(temp_file)
    }
}

// ============================================================================
// Output
// ============================================================================

fn output_text(text: &str, clipboard: bool) -> Result<()> {
    if clipboard {
        std::process::Command::new("wl-copy")
            .arg(text)
            .status()
            .context("Failed to copy to clipboard (is wl-clipboard installed?)")?;
        println!("Copied to clipboard: {}", text);
    } else {
        std::process::Command::new("wtype")
            .arg(text)
            .status()
            .context("Failed to type text (is wtype installed?)")?;
    }
    Ok(())
}

// ============================================================================
// Event Loop
// ============================================================================

fn run_event_loop(
    mut engine: ParakeetEngine,
    keyboards: Vec<Device>,
    hotkey: Key,
    clipboard: bool,
) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    // Set keyboards to non-blocking mode
    for kb in &keyboards {
        let fd = kb.as_raw_fd();
        let flags = fcntl(fd, FcntlArg::F_GETFL).context("Failed to get fd flags")?;
        let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
        fcntl(fd, FcntlArg::F_SETFL(flags)).context("Failed to set non-blocking")?;
    }

    let mut keyboards = keyboards;
    let mut recorder = AudioRecorder::new()?;
    let mut is_recording = false;
    let mut ctrl_held = false;

    println!("Press Ctrl+C to exit.");

    while running.load(Ordering::SeqCst) {
        for keyboard in &mut keyboards {
            while let Ok(events) = keyboard.fetch_events() {
                for event in events {
                    if let InputEventKind::Key(key) = event.kind() {
                        // Track Ctrl key state for Ctrl+C detection
                        if key == Key::KEY_LEFTCTRL || key == Key::KEY_RIGHTCTRL {
                            ctrl_held = event.value() != 0;
                        }

                        // Handle Ctrl+C to exit
                        if key == Key::KEY_C && event.value() == 1 && ctrl_held {
                            running.store(false, Ordering::SeqCst);
                            break;
                        }

                        // Handle hotkey press/release
                        if key == hotkey {
                            handle_hotkey_event(
                                event.value(),
                                &mut is_recording,
                                &mut recorder,
                                &mut engine,
                                clipboard,
                            );
                        }
                    }
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    engine.unload_model();
    println!("\nExiting.");
    Ok(())
}

fn handle_hotkey_event(
    value: i32,
    is_recording: &mut bool,
    recorder: &mut AudioRecorder,
    engine: &mut ParakeetEngine,
    clipboard: bool,
) {
    match value {
        1 if !*is_recording => {
            println!("Recording...");
            if let Err(e) = recorder.start() {
                log::error!("Failed to start recording: {}", e);
                return;
            }
            *is_recording = true;
        }
        0 if *is_recording => {
            println!("Transcribing...");
            *is_recording = false;
            handle_transcription(recorder, engine, clipboard);
        }
        _ => {}
    }
}

fn handle_transcription(
    recorder: &mut AudioRecorder,
    engine: &mut ParakeetEngine,
    clipboard: bool,
) {
    match recorder.stop() {
        Ok(wav_path) => {
            let start = Instant::now();
            match engine.transcribe_file(&wav_path, None) {
                Ok(result) => {
                    log::debug!("Transcribed in {:.2?}", start.elapsed());
                    let text = result.text.trim();
                    if !text.is_empty() {
                        if let Err(e) = output_text(text, clipboard) {
                            log::error!("Failed to output text: {}", e);
                        }
                    } else {
                        println!("(no speech detected)");
                    }
                }
                Err(e) => log::error!("Transcription failed: {}", e),
            }
            let _ = std::fs::remove_file(wav_path);
        }
        Err(e) => log::error!("Failed to stop recording: {}", e),
    }
}

// ============================================================================
// Main
// ============================================================================

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();
    let hotkey = parse_hotkey(&args.key)?;
    let model_path = ensure_model(args.model)?;
    let engine = load_engine(&model_path)?;
    let keyboards = find_keyboards()?;

    println!(
        "Found {} keyboard(s). Listening for {:?}...",
        keyboards.len(),
        args.key
    );
    println!("Hold the key to record, release to transcribe.");

    run_event_loop(engine, keyboards, hotkey, args.clipboard)
}
