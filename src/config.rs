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
    /// Tone loudness as a percentage (0–100). Stored as an int so the menu
    /// events stay `Eq`; convert to a 0.0–1.0 amplitude with `amplitude()`.
    #[serde(default = "default_volume_percent")]
    pub volume_percent: u8,
    /// Tone frequency in Hz; older configurations retain 50 Hz.
    #[serde(default = "default_frequency_hz")]
    pub frequency_hz: u16,
}

fn default_frequency_hz() -> u16 {
    50
}

fn default_volume_percent() -> u8 {
    80
}

impl Settings {
    /// Tone amplitude in the 0.0–1.0 range the audio code expects.
    pub fn amplitude(&self) -> f32 {
        self.volume_percent.min(100) as f32 / 100.0
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            enabled: true,
            mode: Mode::Interval,
            interval_secs: 60,
            device_name: None,
            volume_percent: default_volume_percent(),
            frequency_hz: default_frequency_hz(),
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
