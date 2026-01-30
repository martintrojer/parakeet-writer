mod audio;
mod event_loop;
mod model;
mod output;
mod post_process;

use anyhow::Result;
use clap::Parser;
use hotkey_listener::{parse_hotkey, HotkeyListenerBuilder};
use output::OutputMode;
use post_process::PostProcessor;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "parakeet-writer")]
#[command(about = "Push-to-talk transcriber using Parakeet v3")]
struct Args {
    /// Path to the parakeet model directory (auto-downloads if not specified)
    #[arg(short, long)]
    model: Option<PathBuf>,

    /// Hotkey to trigger recording (e.g., F9, ScrollLock)
    #[arg(short, long, default_value = "F9")]
    key: String,

    /// Output mode: typing, clipboard, or both
    #[arg(short, long, value_enum, default_value_t = OutputMode::Both)]
    output: OutputMode,

    /// Enable post-processing via Ollama to clean up transcripts
    #[arg(short, long)]
    post_process: bool,

    /// Ollama host
    #[arg(long, default_value = "http://localhost")]
    ollama_host: String,

    /// Ollama port
    #[arg(long, default_value_t = 11434)]
    ollama_port: u16,

    /// Ollama model for post-processing
    #[arg(long, default_value = "qwen3:1.7b")]
    ollama_model: String,

    /// Enable verbose (debug) logging
    #[arg(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let log_level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level)).init();

    log::debug!("Args: {:?}", args);

    let hotkey = parse_hotkey(&args.key)?;
    let model_path = model::ensure_model(args.model).await?;
    let engine = model::load_engine(&model_path)?;

    let post_processor = if args.post_process {
        println!(
            "Post-processing enabled via Ollama ({}:{}, model: {})",
            args.ollama_host, args.ollama_port, args.ollama_model
        );
        Some(PostProcessor::new(
            &args.ollama_host,
            args.ollama_port,
            &args.ollama_model,
        ))
    } else {
        None
    };

    // Build and start the hotkey listener
    let handle = HotkeyListenerBuilder::new()
        .add_hotkey(hotkey)
        .build()?
        .start()?;

    println!("Listening for {:?}...", args.key);
    println!("Hold the key to record, release to transcribe.");

    #[cfg(target_os = "macos")]
    println!("Note: You may need to grant Accessibility permissions.");

    event_loop::run(engine, handle, args.output, post_processor).await
}
