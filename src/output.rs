use anyhow::{Context, Result};
use clap::ValueEnum;
use tokio::process::Command;

#[cfg(target_os = "macos")]
use std::process::Stdio;
#[cfg(target_os = "macos")]
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputMode {
    /// Type text directly
    Typing,
    /// Copy text to clipboard
    Clipboard,
    /// Both type and copy to clipboard
    #[default]
    Both,
}

pub async fn output_text(text: &str, mode: OutputMode) -> Result<()> {
    match mode {
        OutputMode::Typing => {
            type_text(text).await?;
        }
        OutputMode::Clipboard => {
            copy_to_clipboard(text).await?;
            println!("Copied to clipboard: {}", text);
        }
        OutputMode::Both => {
            let (type_result, clip_result) = tokio::join!(type_text(text), copy_to_clipboard(text));
            type_result?;
            clip_result?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
async fn type_text(text: &str) -> Result<()> {
    // Use osascript to type text on macOS
    let script = format!(
        r#"tell application "System Events" to keystroke "{}""#,
        text.replace('\\', "\\\\").replace('"', "\\\"")
    );
    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .await
        .context("Failed to type text via osascript")?;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn type_text(text: &str) -> Result<()> {
    Command::new("wtype")
        .arg(text)
        .status()
        .await
        .context("Failed to type text (is wtype installed?)")?;
    Ok(())
}

#[cfg(target_os = "macos")]
async fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to run pbcopy")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .await
            .context("Failed to write to pbcopy")?;
    }
    child.wait().await.context("Failed to wait for pbcopy")?;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn copy_to_clipboard(text: &str) -> Result<()> {
    Command::new("wl-copy")
        .arg(text)
        .status()
        .await
        .context("Failed to copy to clipboard (is wl-clipboard installed?)")?;
    Ok(())
}
