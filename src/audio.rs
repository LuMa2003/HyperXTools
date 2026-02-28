//! Manages audio-related actions for mic mute sync and device switching.

use std::mem;

use com_policy_config::{IPolicyConfig, PolicyConfigClient};
use windows::core::PWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::System::Com::STGM_READ;

/// A discovered audio input device.
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

/// Simulates an F13 keypress to toggle Discord mute via keybind.
pub fn sync_mic_mute() {
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_F13,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VK_F13,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];
    unsafe {
        SendInput(&inputs, mem::size_of::<INPUT>() as i32);
    }
}

/// Enumerates all active audio capture (input) devices.
pub fn enumerate_input_devices() -> Vec<AudioDevice> {
    debug_log!("[audio] enumerate_input_devices() called");
    unsafe {
        let mut devices = Vec::new();

        let Ok(enumerator) = CoCreateInstance::<_, IMMDeviceEnumerator>(
            &MMDeviceEnumerator,
            None,
            CLSCTX_ALL,
        ) else {
            debug_log!("[audio] ERROR: failed to create IMMDeviceEnumerator");
            return devices;
        };
        debug_log!("[audio] created IMMDeviceEnumerator OK");

        let Ok(collection) = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else {
            debug_log!("[audio] ERROR: EnumAudioEndpoints failed");
            return devices;
        };

        let Ok(count) = collection.GetCount() else {
            debug_log!("[audio] ERROR: GetCount failed");
            return devices;
        };
        debug_log!("[audio] found {} active capture device(s)", count);

        for i in 0..count {
            let Ok(device) = collection.Item(i) else {
                debug_log!("[audio] ERROR: collection.Item({}) failed", i);
                continue;
            };

            let id = match device.GetId() {
                Ok(id_ptr) => {
                    let id_str = pwstr_to_string(id_ptr);
                    CoTaskMemFree(Some(id_ptr.0 as *const _));
                    id_str
                }
                Err(e) => {
                    debug_log!("[audio] ERROR: GetId failed for device {}: {:?}", i, e);
                    continue;
                }
            };

            let name = match device.OpenPropertyStore(STGM_READ) {
                Ok(store) => match store.GetValue(&PKEY_Device_FriendlyName) {
                    Ok(prop) => {
                        let pwstr = prop.Anonymous.Anonymous.Anonymous.pwszVal;
                        if pwstr.is_null() {
                            debug_log!("[audio] device {} has null friendly name", i);
                            String::new()
                        } else {
                            pwstr_to_string(PWSTR(pwstr.0))
                        }
                    }
                    Err(e) => {
                        debug_log!("[audio] ERROR: GetValue(FriendlyName) failed for device {}: {:?}", i, e);
                        String::new()
                    }
                },
                Err(e) => {
                    debug_log!("[audio] ERROR: OpenPropertyStore failed for device {}: {:?}", i, e);
                    String::new()
                }
            };

            debug_log!("[audio] device[{}]: name={:?} id={:?}", i, name, id);
            devices.push(AudioDevice { id, name });
        }

        debug_log!("[audio] returning {} device(s)", devices.len());
        devices
    }
}

/// Finds the device ID of the HyperX headset mic (case-insensitive name match).
pub fn find_hyperx_device_id() -> Option<String> {
    enumerate_input_devices()
        .into_iter()
        .find(|d| d.name.to_lowercase().contains("hyperx"))
        .map(|d| d.id)
}

/// Sets the default audio endpoint for both Console and Communications roles.
pub fn set_default_endpoint(device_id: &str) {
    unsafe {
        let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let pcwstr = windows::core::PCWSTR(wide.as_ptr());

        let Ok(policy_config) = CoCreateInstance::<_, IPolicyConfig>(
            &PolicyConfigClient,
            None,
            CLSCTX_ALL,
        ) else {
            return;
        };

        let _ = policy_config.SetDefaultEndpoint(pcwstr, eConsole);
        let _ = policy_config.SetDefaultEndpoint(pcwstr, eCommunications);
    }
}

/// Core mic switching logic: swaps the default input device based on mute state.
///
/// - `muted == true`  → switch to the user's main mic
/// - `muted == false` → switch to the HyperX headset mic
pub fn switch_mic_on_mute(muted: bool, main_mic_id: &str) {
    if muted {
        // Verify the main mic still exists before switching
        let exists = enumerate_input_devices()
            .iter()
            .any(|d| d.id == main_mic_id);
        if exists {
            set_default_endpoint(main_mic_id);
        }
    } else {
        // Switch back to HyperX
        if let Some(hyperx_id) = find_hyperx_device_id() {
            set_default_endpoint(&hyperx_id);
        }
    }
}

/// Initializes COM for the current thread (must be called before audio functions).
pub fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
}

/// Converts a PWSTR to a Rust String.
unsafe fn pwstr_to_string(p: PWSTR) -> String {
    unsafe {
        let len = (0..).take_while(|&i| *p.0.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(p.0, len))
    }
}
