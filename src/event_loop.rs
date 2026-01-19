use crate::audio::AudioRecorder;
use crate::output::{output_text, OutputMode};
use crate::post_process::PostProcessor;
use anyhow::Result;
use rdev::{listen, Event, EventType, Key};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;
use transcribe_rs::engines::parakeet::ParakeetEngine;
use transcribe_rs::TranscriptionEngine;

#[derive(Debug)]
enum HotkeyEvent {
    Pressed,
    Released,
}

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
                handle_transcription(
                    &mut recorder,
                    &mut engine,
                    output_mode,
                    post_processor.as_ref(),
                    &runtime,
                );
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

fn handle_transcription(
    recorder: &mut AudioRecorder,
    engine: &mut ParakeetEngine,
    output_mode: OutputMode,
    post_processor: Option<&PostProcessor>,
    runtime: &tokio::runtime::Runtime,
) {
    match recorder.stop() {
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

                        if let Err(e) = output_text(&final_text, output_mode) {
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
