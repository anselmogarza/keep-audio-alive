use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum Mode {
    /// Play a short tone every `interval_secs`.
    Interval,
    /// Play the tone non-stop, no gaps, until disabled.
    Continuous,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: bool,
    pub mode: Mode,
    pub interval_secs: u64,
    /// `None` means "system default output device".
    pub device_name: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            mode: Mode::Interval,
            interval_secs: 60,
            device_name: None,
        }
    }
}

fn config_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(std::env::temp_dir);
    dir.push("KeepAudioAlive");
    let _ = std::fs::create_dir_all(&dir);
    dir.push("config.json");
    dir
}

pub fn load() -> Settings {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(config_path(), json);
    }
}
