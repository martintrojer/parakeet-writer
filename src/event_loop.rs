use crate::audio::AudioRecorder;
use crate::output::{output_text, OutputMode};
use crate::post_process::PostProcessor;
use anyhow::Result;
use rdev::{listen, Event, EventType, Key};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;
use transcribe_rs::engines::parakeet::ParakeetEngine;
use transcribe_rs::TranscriptionEngine;

#[derive(Debug)]
enum HotkeyEvent {
    Pressed,
    Released,
}

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

    let (tx, mut rx) = mpsc::unbounded_channel::<HotkeyEvent>();

    // Spawn keyboard listener thread (rdev::listen is inherently blocking)
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

    // Wrap engine in Arc<Mutex> for spawn_blocking
    let engine = Arc::new(std::sync::Mutex::new(engine));

    let mut recorder = AudioRecorder::new();
    let mut is_recording = false;

    println!("Press Ctrl+C to exit.");

    let mut interval = tokio::time::interval(std::time::Duration::from_millis(100));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                if !running.load(Ordering::SeqCst) {
                    break;
                }
            }
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
                        println!("Transcribing...");
                        is_recording = false;
                        handle_transcription(
                            &mut recorder,
                            Arc::clone(&engine),
                            output_mode,
                            post_processor.as_ref(),
                        ).await;
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
    }

    // Unload model
    if let Ok(mut eng) = engine.lock() {
        eng.unload_model();
    }
    println!("\nExiting.");
    Ok(())
}

async fn handle_transcription(
    recorder: &mut AudioRecorder,
    engine: Arc<std::sync::Mutex<ParakeetEngine>>,
    output_mode: OutputMode,
    post_processor: Option<&PostProcessor>,
) {
    match recorder.stop().await {
        Ok(wav_path) => {
            let start = Instant::now();

            // Transcription is blocking, run in spawn_blocking
            let transcription_result = transcribe_file(engine, wav_path.clone()).await;

            match transcription_result {
                Ok(result) => {
                    log::debug!("Transcribed in {:.2?}", start.elapsed());
                    let text = result.text.trim();
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
                Err(e) => log::error!("Transcription failed: {}", e),
            }
            let _ = tokio::fs::remove_file(wav_path).await;
        }
        Err(e) => log::error!("Failed to stop recording: {}", e),
    }
}

async fn transcribe_file(
    engine: Arc<std::sync::Mutex<ParakeetEngine>>,
    wav_path: PathBuf,
) -> Result<transcribe_rs::TranscriptionResult> {
    tokio::task::spawn_blocking(move || {
        let mut eng = engine
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        eng.transcribe_file(&wav_path, None)
            .map_err(|e| anyhow::anyhow!("{}", e))
    })
    .await?
}
