use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Application settings persisted to disk.
#[derive(Serialize, Deserialize)]
pub struct Config {
    /// Sync headset hardware mute to F13 keypress (for Discord keybinds).
    #[serde(default = "default_true")]
    pub mic_mute_sync: bool,
    /// Swap default Windows mic on mute/unmute.
    #[serde(default)]
    pub mic_switching: bool,
    /// Device ID of the user's main mic (set via picker).
    #[serde(default)]
    pub main_mic_id: Option<String>,
    /// Friendly name of the user's main mic (for display).
    #[serde(default)]
    pub main_mic_name: Option<String>,
    /// Version the user chose to skip (auto-update won't prompt again for this version).
    #[serde(default)]
    pub skipped_version: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mic_mute_sync: true,
            mic_switching: false,
            main_mic_id: None,
            main_mic_name: None,
            skipped_version: None,
        }
    }
}

impl Config {
    /// Returns the path to the config file: `%APPDATA%/HyperXTools/config.toml`.
    fn config_path() -> Option<PathBuf> {
        std::env::var("APPDATA")
            .ok()
            .map(|dir| PathBuf::from(dir).join("HyperXTools").join("config.toml"))
    }

    /// Loads config from disk, returning defaults if the file doesn't exist or is invalid.
    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&contents).unwrap_or_default()
    }

    /// Saves current config to disk.
    pub fn save(&self) {
        let Some(path) = Self::config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(contents) = toml::to_string_pretty(self) {
            let _ = std::fs::write(&path, contents);
        }
    }
}
