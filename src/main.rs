mod audio;
mod event_loop;
mod input;
mod model;
mod output;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

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

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();
    let hotkey = input::parse_hotkey(&args.key)?;
    let model_path = model::ensure_model(args.model)?;
    let engine = model::load_engine(&model_path)?;
    let keyboards = input::find_keyboards()?;

    println!(
        "Found {} keyboard(s). Listening for {:?}...",
        keyboards.len(),
        args.key
    );
    println!("Hold the key to record, release to transcribe.");

    event_loop::run(engine, keyboards, hotkey, args.clipboard)
}
