use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use windows::Win32::System::Registry::*;

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

/// Reads a DWORD value from the registry under `HKCU\<subkey>\<name>`.
pub fn read_registry_dword(subkey: &str, name: &str) -> Option<u32> {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey_wide = to_wide(subkey);
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            to_pcwstr(&subkey_wide),
            None,
            KEY_READ,
            &mut hkey,
        );
        if result.is_err() {
            return None;
        }

        let name_wide = to_wide(name);
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let result = RegQueryValueExW(
            hkey,
            to_pcwstr(&name_wide),
            None,
            None,
            Some(std::ptr::from_mut(&mut data).cast()),
            Some(&mut size),
        );
        let _ = RegCloseKey(hkey);

        if result.is_ok() {
            Some(data)
        } else {
            None
        }
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn to_pcwstr(s: &[u16]) -> windows::core::PCWSTR {
    windows::core::PCWSTR(s.as_ptr())
}
