use anyhow::{Context, Result};

pub fn output_text(text: &str, clipboard: bool) -> Result<()> {
    if clipboard {
        std::process::Command::new("wl-copy")
            .arg(text)
            .status()
            .context("Failed to copy to clipboard (is wl-clipboard installed?)")?;
        println!("Copied to clipboard: {}", text);
    } else {
        std::process::Command::new("wtype")
            .arg(text)
            .status()
            .context("Failed to type text (is wtype installed?)")?;
    }
    Ok(())
}
