use std::path::PathBuf;

/// Application settings persisted to disk.
pub struct Config {
    /// Sync headset hardware mute to Windows mic mute.
    pub mic_mute_sync: bool,
    /// Automatically swap default mic when headset connects/disconnects.
    pub auto_mic_swap: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mic_mute_sync: true,
            auto_mic_swap: false,
        }
    }
}

impl Config {
    /// Returns the path to the config file in the user's AppData directory.
    fn config_path() -> Option<PathBuf> {
        // TODO: Use %APPDATA%/HyperXTools/config.toml
        std::env::var("APPDATA").ok().map(|dir| {
            PathBuf::from(dir).join("HyperXTools").join("config.toml")
        })
    }

    /// Loads config from disk, returning defaults if the file doesn't exist.
    pub fn load() -> Self {
        // TODO: Read and parse TOML file
        // TODO: Fall back to Config::default() on error
        Self::default()
    }

    /// Saves current config to disk.
    pub fn save(&self) {
        // TODO: Serialize to TOML and write to config_path()
        let _ = Self::config_path();
    }
}
