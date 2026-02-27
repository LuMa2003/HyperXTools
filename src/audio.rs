//! Manages audio-related actions for mic mute sync and device switching.

use std::mem;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

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

/// Switches the default microphone to/from the HyperX headset.
pub fn set_default_mic(_to_hyperx: bool) {
    // TODO: Use IPolicyConfig (undocumented) or AudioDeviceCmdlets approach
}

/// Detects if the headset mic is producing digital silence (hardware mute fallback).
pub fn is_digital_silence() -> bool {
    // TODO: Use IAudioMeterInformation::GetPeakValue
    false
}
