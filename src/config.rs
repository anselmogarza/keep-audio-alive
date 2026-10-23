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
    /// Tone amplitude as a fractional percentage (0-100).
    #[serde(default = "default_volume_percent")]
    pub volume_percent: f32,
    #[serde(default = "default_force_mixer")]
    pub force_mixer_volume: bool,
    #[serde(default)]
    pub muted: bool,
    /// Tone frequency in Hz; older configurations retain 50 Hz.
    #[serde(default = "default_frequency_hz")]
    pub frequency_hz: u16,
}

fn default_frequency_hz() -> u16 {
    50
}

fn default_volume_percent() -> f32 {
    80.0
}

fn default_force_mixer() -> bool {
    true
}

impl Settings {
    /// Tone amplitude in the 0.0–1.0 range the audio code expects.
    pub fn amplitude(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume_percent.clamp(0.0, 100.0) / 100.0
        }
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
            force_mixer_volume: default_force_mixer(),
            muted: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_integer_volume_keeps_all_existing_settings() {
        let s: Settings = serde_json::from_str(
            r#"{
            "enabled":false,"mode":"Continuous","interval_secs":120,
            "device_name":"My monitors","volume_percent":1,"frequency_hz":250
        }"#,
        )
        .unwrap();
        assert_eq!(s.volume_percent, 1.0);
        assert_eq!(s.frequency_hz, 250);
        assert_eq!(s.device_name.as_deref(), Some("My monitors"));
        assert!(!s.enabled);
        assert_eq!(s.mode, Mode::Continuous);
        assert!(s.force_mixer_volume);
        assert!(!s.muted);
    }

    #[test]
    fn fractional_volume_survives_save_and_load() {
        let s = Settings {
            volume_percent: 0.01,
            ..Settings::default()
        };
        let loaded: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(loaded.volume_percent, 0.01);
        assert!((loaded.amplitude() - 0.0001).abs() < 0.0000001);
    }

    #[test]
    fn mute_preserves_the_selected_volume() {
        let mut s = Settings {
            volume_percent: 0.05,
            muted: true,
            ..Settings::default()
        };
        assert_eq!(s.amplitude(), 0.0);
        s.muted = false;
        assert!((s.amplitude() - 0.0005).abs() < 0.0000001);
    }
}
