use anyhow::Result;

#[cfg(target_os = "linux")]
use evdev::{Device, Key};

#[cfg(target_os = "macos")]
use rdev::Key;

#[cfg(target_os = "linux")]
pub fn parse_hotkey(key: &str) -> Result<Key> {
    let key_upper = key.to_uppercase();
    match key_upper.as_str() {
        "F1" => Ok(Key::KEY_F1),
        "F2" => Ok(Key::KEY_F2),
        "F3" => Ok(Key::KEY_F3),
        "F4" => Ok(Key::KEY_F4),
        "F5" => Ok(Key::KEY_F5),
        "F6" => Ok(Key::KEY_F6),
        "F7" => Ok(Key::KEY_F7),
        "F8" => Ok(Key::KEY_F8),
        "F9" => Ok(Key::KEY_F9),
        "F10" => Ok(Key::KEY_F10),
        "F11" => Ok(Key::KEY_F11),
        "F12" => Ok(Key::KEY_F12),
        "SCROLLLOCK" | "SCROLL_LOCK" => Ok(Key::KEY_SCROLLLOCK),
        "PAUSE" => Ok(Key::KEY_PAUSE),
        "INSERT" => Ok(Key::KEY_INSERT),
        _ => anyhow::bail!("Unknown hotkey: {}", key),
    }
}

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "linux")]
pub fn find_keyboards() -> Result<Vec<Device>> {
    let mut keyboards = Vec::new();
    for path in std::fs::read_dir("/dev/input")? {
        let path = path?.path();
        if let Some(name) = path.file_name() {
            if name.to_string_lossy().starts_with("event") {
                if let Ok(device) = Device::open(&path) {
                    if device
                        .supported_keys()
                        .is_some_and(|keys| keys.contains(Key::KEY_A))
                    {
                        log::debug!(
                            "Found keyboard: {} ({:?})",
                            device.name().unwrap_or("unknown"),
                            path
                        );
                        keyboards.push(device);
                    }
                }
            }
        }
    }
    if keyboards.is_empty() {
        anyhow::bail!("No keyboards found. Try running with sudo or add user to input group.");
    }
    Ok(keyboards)
}
