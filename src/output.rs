use anyhow::{Context, Result};
use clap::ValueEnum;

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputMode {
    /// Type text using wtype
    Typing,
    /// Copy text to clipboard using wl-copy
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

fn type_text(text: &str) -> Result<()> {
    std::process::Command::new("wtype")
        .arg(text)
        .status()
        .context("Failed to type text (is wtype installed?)")?;
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<()> {
    std::process::Command::new("wl-copy")
        .arg(text)
        .status()
        .context("Failed to copy to clipboard (is wl-clipboard installed?)")?;
    Ok(())
}
