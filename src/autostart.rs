use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::System::Registry::*;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "HyperXTools";

/// Returns whether autostart is currently enabled in the registry.
pub fn is_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = to_wide(RUN_KEY);
        let result = RegOpenKeyExW(HKEY_CURRENT_USER, to_pcwstr(&subkey), None, KEY_READ, &mut hkey);
        if result.is_err() {
            return false;
        }
        let name = to_wide(VALUE_NAME);
        let exists =
            RegQueryValueExW(hkey, to_pcwstr(&name), None, None, None, None).is_ok();
        let _ = RegCloseKey(hkey);
        exists
    }
}

/// Enables or disables autostart by adding/removing the registry Run key.
pub fn set_enabled(enable: bool) {
    unsafe {
        let mut hkey = HKEY::default();
        let subkey = to_wide(RUN_KEY);
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            to_pcwstr(&subkey),
            None,
            KEY_WRITE,
            &mut hkey,
        );
        if result.is_err() {
            return;
        }

        let name = to_wide(VALUE_NAME);
        if enable {
            if let Ok(exe_path) = std::env::current_exe() {
                let path_str = exe_path.to_string_lossy();
                let value = to_wide(&path_str);
                let bytes: &[u8] = std::slice::from_raw_parts(
                    value.as_ptr() as *const u8,
                    value.len() * 2,
                );
                let _ = RegSetValueExW(
                    hkey,
                    to_pcwstr(&name),
                    None,
                    REG_SZ,
                    Some(bytes),
                );
            }
        } else {
            let _ = RegDeleteValueW(hkey, to_pcwstr(&name));
        }
        let _ = RegCloseKey(hkey);
    }
}

/// Converts a &str to a null-terminated wide string (Vec<u16>).
fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Creates a PCWSTR from a null-terminated wide string slice.
fn to_pcwstr(s: &[u16]) -> windows::core::PCWSTR {
    windows::core::PCWSTR(s.as_ptr())
}
