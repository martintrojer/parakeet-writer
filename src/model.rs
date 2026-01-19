use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tar::Archive;
use transcribe_rs::engines::parakeet::{ParakeetEngine, ParakeetModelParams};
use transcribe_rs::TranscriptionEngine;

const MODEL_URL: &str = "https://blob.handy.computer/parakeet-v3-int8.tar.gz";
const MODEL_DIR_NAME: &str = "parakeet-tdt-0.6b-v3-int8";

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
                let filled = percent / 5;
                let bar = "=".repeat(filled) + &" ".repeat(20 - filled);
                eprint!("\r[{}] {}%", bar, percent);
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

pub fn ensure_model(model_path: Option<PathBuf>) -> Result<PathBuf> {
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

pub fn load_engine(model_path: &Path) -> Result<ParakeetEngine> {
    println!("Loading Parakeet model from {:?}...", model_path);
    let load_start = Instant::now();
    let mut engine = ParakeetEngine::new();
    engine
        .load_model_with_params(model_path, ParakeetModelParams::int8())
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    println!("Model loaded in {:.2?}", load_start.elapsed());
    Ok(engine)
}
