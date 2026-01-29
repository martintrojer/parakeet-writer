use crate::audio::AudioRecorder;
use crate::output::{output_text, OutputMode};
use crate::post_process::PostProcessor;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, Receiver, Sender};
use transcribe_rs::engines::parakeet::ParakeetEngine;
use transcribe_rs::TranscriptionEngine;

// Linux-specific imports
#[cfg(target_os = "linux")]
use crate::input::find_keyboards;
#[cfg(target_os = "linux")]
use anyhow::Context;
#[cfg(target_os = "linux")]
use evdev::{Device, InputEventKind, Key};
#[cfg(target_os = "linux")]
use nix::fcntl::{fcntl, FcntlArg, OFlag};
#[cfg(target_os = "linux")]
use std::os::unix::io::AsRawFd;

// macOS-specific imports
#[cfg(target_os = "macos")]
use rdev::{listen, Event, EventType, Key};

#[derive(Debug)]
enum HotkeyEvent {
    Pressed,
    Released,
}

// Linux: set non-blocking mode on keyboard devices
#[cfg(target_os = "linux")]
fn set_nonblocking(keyboards: &[Device]) -> Result<()> {
    for kb in keyboards {
        let fd = kb.as_raw_fd();
        let flags = fcntl(fd, FcntlArg::F_GETFL).context("Failed to get fd flags")?;
        let flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
        fcntl(fd, FcntlArg::F_SETFL(flags)).context("Failed to set non-blocking")?;
    }
    Ok(())
}

// Linux: start keyboard listener thread
#[cfg(target_os = "linux")]
fn start_keyboard_listener(
    keyboards: Vec<Device>,
    hotkey: Key,
    running: Arc<AtomicBool>,
    tx: Sender<HotkeyEvent>,
) -> Result<()> {
    set_nonblocking(&keyboards)?;

    std::thread::spawn(move || {
        let mut keyboards = keyboards;
        let mut last_rescan = std::time::Instant::now();
        let mut had_error = false;

        // Minimum interval between keyboard rescans
        const RESCAN_INTERVAL: Duration = Duration::from_secs(10);

        while running.load(Ordering::SeqCst) {
            // Check if we need to rescan keyboards (after error and interval passed)
            if had_error && last_rescan.elapsed() >= RESCAN_INTERVAL {
                log::info!("Keyboard error detected, rescanning devices...");
                match find_keyboards() {
                    Ok(new_keyboards) => {
                        log::info!(
                            "Keyboards reconnected: found {} device(s)",
                            new_keyboards.len()
                        );
                        if set_nonblocking(&new_keyboards).is_ok() {
                            keyboards = new_keyboards;
                            had_error = false;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to rescan keyboards: {}", e);
                    }
                }
                last_rescan = std::time::Instant::now();
            }

            let mut any_error = false;

            for keyboard in &mut keyboards {
                match keyboard.fetch_events() {
                    Ok(events) => {
                        for event in events {
                            if let InputEventKind::Key(key) = event.kind() {
                                if key == hotkey {
                                    let hotkey_event = match event.value() {
                                        1 => Some(HotkeyEvent::Pressed),
                                        0 => Some(HotkeyEvent::Released),
                                        _ => None,
                                    };
                                    if let Some(e) = hotkey_event {
                                        let _ = tx.blocking_send(e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // EAGAIN/EWOULDBLOCK is expected for non-blocking reads
                        if e.raw_os_error() != Some(libc::EAGAIN)
                            && e.raw_os_error() != Some(libc::EWOULDBLOCK)
                        {
                            log::debug!("Keyboard read error: {}", e);
                            any_error = true;
                        }
                    }
                }
            }

            if any_error {
                had_error = true;
            }

            std::thread::sleep(Duration::from_millis(10));
        }
    });

    Ok(())
}

// macOS: start keyboard listener thread
#[cfg(target_os = "macos")]
fn start_keyboard_listener(hotkey: Key, running: Arc<AtomicBool>, tx: Sender<HotkeyEvent>) {
    std::thread::spawn(move || {
        let callback = move |event: Event| match event.event_type {
            EventType::KeyPress(key) if key == hotkey => {
                let _ = tx.blocking_send(HotkeyEvent::Pressed);
            }
            EventType::KeyRelease(key) if key == hotkey => {
                let _ = tx.blocking_send(HotkeyEvent::Released);
            }
            _ => {}
        };

        if let Err(e) = listen(callback) {
            log::error!("Error listening to keyboard events: {:?}", e);
            running.store(false, Ordering::SeqCst);
        }
    });
}

// Linux entry point
#[cfg(target_os = "linux")]
pub async fn run(
    engine: ParakeetEngine,
    keyboards: Vec<Device>,
    hotkey: Key,
    output_mode: OutputMode,
    post_processor: Option<PostProcessor>,
) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let (tx, rx) = mpsc::channel(32);
    start_keyboard_listener(keyboards, hotkey, Arc::clone(&running), tx)?;

    run_event_loop(engine, rx, output_mode, post_processor, running).await
}

// macOS entry point
#[cfg(target_os = "macos")]
pub async fn run(
    engine: ParakeetEngine,
    hotkey: Key,
    output_mode: OutputMode,
    post_processor: Option<PostProcessor>,
) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let (tx, rx) = mpsc::channel(32);
    start_keyboard_listener(hotkey, Arc::clone(&running), tx);

    run_event_loop(engine, rx, output_mode, post_processor, running).await
}

// Unified async event loop for both platforms
async fn run_event_loop(
    engine: ParakeetEngine,
    mut rx: Receiver<HotkeyEvent>,
    output_mode: OutputMode,
    post_processor: Option<PostProcessor>,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let engine = Arc::new(std::sync::Mutex::new(engine));
    let mut recorder = AudioRecorder::new();
    let mut is_recording = false;

    println!("Press Ctrl+C to exit.");

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(HotkeyEvent::Pressed) if !is_recording => {
                        println!("Recording...");
                        if let Err(e) = recorder.start() {
                            log::error!("Failed to start recording: {}", e);
                            continue;
                        }
                        is_recording = true;
                    }
                    Some(HotkeyEvent::Released) if is_recording => {
                        // Continue recording briefly to capture trailing audio
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        println!("Transcribing...");
                        is_recording = false;
                        handle_transcription(
                            &mut recorder,
                            Arc::clone(&engine),
                            output_mode,
                            &post_processor,
                        ).await;
                    }
                    Some(_) => {}
                    None => break,
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    engine.lock().unwrap().unload_model();
    println!("\nExiting.");
    Ok(())
}

async fn handle_transcription(
    recorder: &mut AudioRecorder,
    engine: Arc<std::sync::Mutex<ParakeetEngine>>,
    output_mode: OutputMode,
    post_processor: &Option<PostProcessor>,
) {
    match recorder.stop().await {
        Ok(wav_path) => {
            let start = Instant::now();
            let path = wav_path.clone();

            // Run sync transcription in blocking task
            let result = tokio::task::spawn_blocking(move || {
                let mut engine = engine.lock().unwrap();
                engine
                    .transcribe_file(&path, None)
                    .map_err(|e| e.to_string())
            })
            .await;

            match result {
                Ok(Ok(transcription)) => {
                    log::debug!("Transcribed in {:.2?}", start.elapsed());
                    let text = transcription.text.trim();
                    if !text.is_empty() {
                        let final_text = if let Some(processor) = post_processor {
                            println!("Post-processing...");
                            match processor.process(text).await {
                                Ok(processed) => processed,
                                Err(e) => {
                                    log::error!("Post-processing failed: {}", e);
                                    text.to_string()
                                }
                            }
                        } else {
                            text.to_string()
                        };

                        if let Err(e) = output_text(&final_text, output_mode).await {
                            log::error!("Failed to output text: {}", e);
                        }
                    } else {
                        println!("(no speech detected)");
                    }
                }
                Ok(Err(e)) => log::error!("Transcription failed: {}", e),
                Err(e) => log::error!("Transcription task failed: {}", e),
            }
            let _ = std::fs::remove_file(wav_path);
        }
        Err(e) => log::error!("Failed to stop recording: {}", e),
    }
}
