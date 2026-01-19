use crate::audio::AudioRecorder;
use crate::output::{output_text, OutputMode};
use crate::post_process::PostProcessor;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use transcribe_rs::engines::parakeet::ParakeetEngine;
use transcribe_rs::TranscriptionEngine;

// Linux-specific imports
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
#[cfg(target_os = "macos")]
use std::sync::mpsc::{self, Receiver, Sender};

#[cfg(target_os = "macos")]
#[derive(Debug)]
enum HotkeyEvent {
    Pressed,
    Released,
}

// Linux implementation using evdev
#[cfg(target_os = "linux")]
pub fn run(
    mut engine: ParakeetEngine,
    keyboards: Vec<Device>,
    hotkey: Key,
    output_mode: OutputMode,
    post_processor: Option<PostProcessor>,
) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;

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
    let mut recorder = AudioRecorder::new();
    let mut is_recording = false;

    println!("Press Ctrl+C to exit.");

    while running.load(Ordering::SeqCst) {
        for keyboard in &mut keyboards {
            while let Ok(events) = keyboard.fetch_events() {
                for event in events {
                    if let InputEventKind::Key(key) = event.kind() {
                        if key == hotkey {
                            handle_hotkey_event_linux(
                                event.value(),
                                &mut is_recording,
                                &mut recorder,
                                &mut engine,
                                output_mode,
                                &post_processor,
                                &runtime,
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

// macOS implementation using rdev
#[cfg(target_os = "macos")]
pub fn run(
    mut engine: ParakeetEngine,
    hotkey: Key,
    output_mode: OutputMode,
    post_processor: Option<PostProcessor>,
) -> Result<()> {
    let runtime = tokio::runtime::Runtime::new()?;

    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    let (tx, rx): (Sender<HotkeyEvent>, Receiver<HotkeyEvent>) = mpsc::channel();

    // Spawn keyboard listener thread
    let running_clone = Arc::clone(&running);
    std::thread::spawn(move || {
        let callback = move |event: Event| match event.event_type {
            EventType::KeyPress(key) if key == hotkey => {
                let _ = tx.send(HotkeyEvent::Pressed);
            }
            EventType::KeyRelease(key) if key == hotkey => {
                let _ = tx.send(HotkeyEvent::Released);
            }
            _ => {}
        };

        if let Err(e) = listen(callback) {
            log::error!("Error listening to keyboard events: {:?}", e);
            running_clone.store(false, Ordering::SeqCst);
        }
    });

    let mut recorder = AudioRecorder::new();
    let mut is_recording = false;

    println!("Press Ctrl+C to exit.");

    while running.load(Ordering::SeqCst) {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(HotkeyEvent::Pressed) if !is_recording => {
                println!("Recording...");
                if let Err(e) = recorder.start() {
                    log::error!("Failed to start recording: {}", e);
                    continue;
                }
                is_recording = true;
            }
            Ok(HotkeyEvent::Released) if is_recording => {
                println!("Transcribing...");
                is_recording = false;
                handle_transcription(&mut recorder, &mut engine, output_mode, &post_processor, &runtime);
            }
            Ok(_) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    engine.unload_model();
    println!("\nExiting.");
    Ok(())
}

#[cfg(target_os = "linux")]
fn handle_hotkey_event_linux(
    value: i32,
    is_recording: &mut bool,
    recorder: &mut AudioRecorder,
    engine: &mut ParakeetEngine,
    output_mode: OutputMode,
    post_processor: &Option<PostProcessor>,
    runtime: &tokio::runtime::Runtime,
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
            handle_transcription(recorder, engine, output_mode, post_processor, runtime);
        }
        _ => {}
    }
}

fn handle_transcription(
    recorder: &mut AudioRecorder,
    engine: &mut ParakeetEngine,
    output_mode: OutputMode,
    post_processor: &Option<PostProcessor>,
    runtime: &tokio::runtime::Runtime,
) {
    match runtime.block_on(recorder.stop()) {
        Ok(wav_path) => {
            let start = Instant::now();
            match engine.transcribe_file(&wav_path, None) {
                Ok(result) => {
                    log::debug!("Transcribed in {:.2?}", start.elapsed());
                    let text = result.text.trim();
                    if !text.is_empty() {
                        let final_text = if let Some(processor) = post_processor {
                            println!("Post-processing...");
                            match runtime.block_on(processor.process(text)) {
                                Ok(processed) => processed,
                                Err(e) => {
                                    log::error!("Post-processing failed: {}", e);
                                    text.to_string()
                                }
                            }
                        } else {
                            text.to_string()
                        };

                        if let Err(e) = runtime.block_on(output_text(&final_text, output_mode)) {
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
