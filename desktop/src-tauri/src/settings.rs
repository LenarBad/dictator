use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const DEFAULT_HOTKEY: &str = "ctrl+shift+d";
pub const DEFAULT_MODEL: &str = "v3_e2e_rnnt";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub hotkey: String,
    pub paste_enabled: bool,
    pub microphone_name: Option<String>,
    pub model_name: String,
    pub preload_model: bool,
    /// Old settings files omit this field and stay off.
    #[serde(default)]
    pub diarization_enabled: bool,
    pub max_recording_seconds: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: DEFAULT_HOTKEY.to_string(),
            paste_enabled: true,
            microphone_name: None,
            model_name: DEFAULT_MODEL.to_string(),
            preload_model: true,
            diarization_enabled: false,
            max_recording_seconds: 180.0,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        let Ok(raw) = fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&raw) {
            Ok(mut settings) => {
                if settings.hotkey.trim().is_empty() {
                    settings.hotkey = DEFAULT_HOTKEY.to_string();
                }
                if settings.model_name.trim().is_empty() {
                    settings.model_name = DEFAULT_MODEL.to_string();
                }
                if is_macos_conflict_hotkey(&settings.hotkey) {
                    settings.hotkey = DEFAULT_HOTKEY.to_string();
                }
                settings
            }
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<PathBuf, String> {
        let path = settings_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let body = serde_json::to_string_pretty(self).map_err(|err| err.to_string())?;
        fs::write(&path, body + "\n").map_err(|err| err.to_string())?;
        Ok(path)
    }

    /// If the user has not picked a mic, persist the built-in one instead of
    /// following macOS system default (often a Bluetooth headset).
    pub fn apply_microphone_preference(&mut self, names: &[String]) -> Option<String> {
        let chosen = crate::recorder::preferred_microphone(names, self.microphone_name.as_deref());
        let saved_missing = self
            .microphone_name
            .as_ref()
            .is_some_and(|saved| !names.is_empty() && !names.iter().any(|name| name == saved));
        if (self.microphone_name.is_none() || saved_missing) && self.microphone_name != chosen {
            self.microphone_name = chosen.clone();
            let _ = self.save();
        }
        self.microphone_name.clone().or(chosen)
    }
}

pub fn settings_path() -> PathBuf {
    let mut dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    dir.push("dictator");
    dir.join("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_diarization_field_stays_off() {
        let raw = r#"{
            "hotkey": "ctrl+alt+d",
            "paste_enabled": false,
            "microphone_name": "Built-in",
            "model_name": "v3_e2e_rnnt",
            "preload_model": false,
            "max_recording_seconds": 90.0
        }"#;
        let settings: Settings = serde_json::from_str(raw).expect("settings");
        assert!(!settings.diarization_enabled);
        assert_eq!(settings.hotkey, "ctrl+alt+d");
        assert!(!settings.paste_enabled);
        assert_eq!(settings.max_recording_seconds, 90.0);
    }
}

pub fn is_macos_conflict_hotkey(combo: &str) -> bool {
    matches!(
        combo.trim().to_lowercase().replace(' ', "").as_str(),
        "ctrl+shift+space"
            | "control+shift+space"
            | "ctrl+space"
            | "control+space"
            | "<ctrl>+<shift>+<space>"
            | "<ctrl>+<space>"
    )
}
