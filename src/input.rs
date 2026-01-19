use anyhow::Result;
use rdev::Key;

pub fn parse_hotkey(key: &str) -> Result<Key> {
    let key_upper = key.to_uppercase();
    match key_upper.as_str() {
        "F1" => Ok(Key::F1),
        "F2" => Ok(Key::F2),
        "F3" => Ok(Key::F3),
        "F4" => Ok(Key::F4),
        "F5" => Ok(Key::F5),
        "F6" => Ok(Key::F6),
        "F7" => Ok(Key::F7),
        "F8" => Ok(Key::F8),
        "F9" => Ok(Key::F9),
        "F10" => Ok(Key::F10),
        "F11" => Ok(Key::F11),
        "F12" => Ok(Key::F12),
        "SCROLLLOCK" | "SCROLL_LOCK" => Ok(Key::ScrollLock),
        "PAUSE" => Ok(Key::Pause),
        "INSERT" => Ok(Key::Insert),
        _ => anyhow::bail!("Unknown hotkey: {}", key),
    }
}
