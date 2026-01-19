use crate::audio::AudioRecorder;
use crate::output::{output_text, OutputMode};
use anyhow::{Context, Result};
use evdev::{Device, InputEventKind, Key};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use transcribe_rs::engines::parakeet::ParakeetEngine;
use transcribe_rs::TranscriptionEngine;

pub fn run(
    mut engine: ParakeetEngine,
    keyboards: Vec<Device>,
    hotkey: Key,
    output_mode: OutputMode,
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
                                output_mode,
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
    output_mode: OutputMode,
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
            handle_transcription(recorder, engine, output_mode);
        }
        _ => {}
    }
}

fn handle_transcription(
    recorder: &mut AudioRecorder,
    engine: &mut ParakeetEngine,
    output_mode: OutputMode,
) {
    match recorder.stop() {
        Ok(wav_path) => {
            let start = Instant::now();
            match engine.transcribe_file(&wav_path, None) {
                Ok(result) => {
                    log::debug!("Transcribed in {:.2?}", start.elapsed());
                    let text = result.text.trim();
                    if !text.is_empty() {
                        if let Err(e) = output_text(text, output_mode) {
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
