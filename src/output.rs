use anyhow::{Context, Result};
use clap::ValueEnum;
use std::process::Command;

#[cfg(target_os = "macos")]
use std::io::Write;
#[cfg(target_os = "macos")]
use std::process::Stdio;

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

pub fn output_text(text: &str, mode: OutputMode) -> Result<()> {
    match mode {
        OutputMode::Typing => {
            type_text(text)?;
        }
        OutputMode::Clipboard => {
            copy_to_clipboard(text)?;
            println!("Copied to clipboard: {}", text);
        }
        OutputMode::Both => {
            type_text(text)?;
            copy_to_clipboard(text)?;
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn type_text(text: &str) -> Result<()> {
    // Use osascript to type text on macOS
    let script = format!(
        r#"tell application "System Events" to keystroke "{}""#,
        text.replace('\\', "\\\\").replace('"', "\\\"")
    );
    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .status()
        .context("Failed to type text via osascript")?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn type_text(text: &str) -> Result<()> {
    Command::new("wtype")
        .arg(text)
        .status()
        .context("Failed to type text (is wtype installed?)")?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .context("Failed to run pbcopy")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .context("Failed to write to pbcopy")?;
    }
    child.wait().context("Failed to wait for pbcopy")?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn copy_to_clipboard(text: &str) -> Result<()> {
    Command::new("wl-copy")
        .arg(text)
        .status()
        .context("Failed to copy to clipboard (is wl-clipboard installed?)")?;
    Ok(())
}
