use crate::audio::AudioRecorder;
use crate::output::{output_text, OutputMode};
use crate::post_process::PostProcessor;
use anyhow::Result;
use hotkey_listener::{HotkeyEvent, HotkeyListenerHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use transcribe_rs::engines::parakeet::ParakeetEngine;
use transcribe_rs::TranscriptionEngine;

pub async fn run(
    engine: ParakeetEngine,
    handle: HotkeyListenerHandle,
    output_mode: OutputMode,
    post_processor: Option<PostProcessor>,
) -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = Arc::clone(&running);
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })?;

    run_event_loop(engine, handle, output_mode, post_processor, running).await
}

async fn run_event_loop(
    engine: ParakeetEngine,
    handle: HotkeyListenerHandle,
    output_mode: OutputMode,
    post_processor: Option<PostProcessor>,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let engine = Arc::new(std::sync::Mutex::new(engine));
    let mut recorder = AudioRecorder::new();
    let mut is_recording = false;

    println!("Press Ctrl+C to exit.");

    while running.load(Ordering::SeqCst) {
        match handle.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => match event {
                HotkeyEvent::Pressed(0) if !is_recording => {
                    println!("Recording...");
                    if let Err(e) = recorder.start() {
                        log::error!("Failed to start recording: {}", e);
                        continue;
                    }
                    is_recording = true;
                }
                HotkeyEvent::Released(0) if is_recording => {
                    // Continue recording briefly to capture trailing audio
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    println!("Transcribing...");
                    is_recording = false;
                    handle_transcription(
                        &mut recorder,
                        Arc::clone(&engine),
                        output_mode,
                        &post_processor,
                    )
                    .await;
                }
                _ => {}
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // No event, continue loop
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                log::debug!("Keyboard listener disconnected");
                break;
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
