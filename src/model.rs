use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use futures_util::StreamExt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tar::Archive;
use tokio::io::AsyncWriteExt;
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

async fn download_model(dest_dir: &Path) -> Result<()> {
    println!("Downloading Parakeet v3 model (~478 MB)...");

    tokio::fs::create_dir_all(dest_dir.parent().unwrap_or(dest_dir))
        .await
        .context("Failed to create cache directory")?;

    let response = reqwest::get(MODEL_URL)
        .await
        .context("Failed to start download")?;

    let total_size = response.content_length().unwrap_or(0);

    let temp_path = dest_dir.with_extension("tar.gz.tmp");
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .context("Failed to create temp file")?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_percent = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Download interrupted")?;
        file.write_all(&chunk)
            .await
            .context("Failed to write to file")?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percent = (downloaded * 100 / total_size) as usize;
            if percent != last_percent {
                let filled = percent / 5;
                let bar = "=".repeat(filled) + &" ".repeat(20 - filled);
                eprint!("\r[{}] {}%", bar, percent);
                use std::io::Write;
                std::io::stderr().flush().ok();
                last_percent = percent;
            }
        }
    }

    file.flush().await?;
    drop(file);

    eprintln!(
        "\r[+] Download complete: {:.1} MB                    ",
        downloaded as f64 / 1_000_000.0
    );

    println!("Extracting model...");

    // Archive extraction is blocking, run in spawn_blocking
    let temp_path_clone = temp_path.clone();
    let extract_dir = dest_dir.parent().unwrap_or(dest_dir).to_path_buf();
    tokio::task::spawn_blocking(move || {
        let tar_gz = File::open(&temp_path_clone).context("Failed to open archive")?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive
            .unpack(&extract_dir)
            .context("Failed to extract archive")?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .context("Extraction task failed")??;

    tokio::fs::remove_file(&temp_path).await.ok();
    println!("[+] Model ready!");

    Ok(())
}

pub async fn ensure_model(model_path: Option<PathBuf>) -> Result<PathBuf> {
    let user_provided = model_path.is_some();
    let path = model_path.unwrap_or_else(default_model_path);

    if verify_model(&path) {
        return Ok(path);
    }

    if user_provided {
        anyhow::bail!("Model not found at {:?}", path);
    }

    download_model(&path).await?;

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
