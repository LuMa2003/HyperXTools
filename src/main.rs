#![windows_subsystem = "windows"]

mod audio;
mod config;
mod hid;
mod icon;
mod tray;

use hid::HeadsetEvent;
use std::thread;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Wrapper to send HWND across thread boundaries.
/// SAFETY: HWND is a numeric handle; PostMessageW is explicitly thread-safe.
struct SendHwnd(HWND);
unsafe impl Send for SendHwnd {}

fn main() {
    let tray = Box::new(tray::TrayIcon::new().expect("Failed to create tray icon"));
    let hwnd = tray.hwnd();

    // Leak TrayIcon into a raw pointer for wndproc access via GWLP_USERDATA
    let tray_ptr = Box::into_raw(tray);
    unsafe {
        SetWindowLongPtrW((*tray_ptr).hwnd(), GWLP_USERDATA, tray_ptr as isize);
    }

    // Spawn background HID communication thread
    let send_hwnd = SendHwnd(hwnd);
    thread::spawn(move || hid_thread(send_hwnd));

    // Win32 message loop — drives the entire application
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }

        // Retake ownership so TrayIcon::drop runs (removes tray icon)
        let _ = Box::from_raw(tray_ptr);
    }
}

/// Background thread: discovers the HyperX dongle and relays HID events to the tray window.
fn hid_thread(send_hwnd: SendHwnd) {
    let hwnd = send_hwnd.0;

    let device = match hid::find_dongle() {
        Some(d) => d,
        None => return, // No dongle; tray stays in "disconnected" state
    };

    // Request initial state
    device.request_connection();
    device.request_battery();
    device.request_charging();

    // Drain startup responses (up to 1.5 seconds)
    let drain_start = std::time::Instant::now();
    while drain_start.elapsed() < std::time::Duration::from_millis(1500) {
        if let Some(event) = device.read_event(500) {
            post_event(hwnd, &event);
        }
    }

    // Passive event loop — relay dongle reports to the tray window
    loop {
        if let Some(event) = device.read_event(5000) {
            // On reconnection, re-request battery info
            if matches!(&event, HeadsetEvent::Connection(true)) {
                device.request_battery();
                device.request_charging();
            }
            post_event(hwnd, &event);
        }
    }
}

/// Maps a HeadsetEvent to a PostMessageW call targeting the tray window.
fn post_event(hwnd: HWND, event: &HeadsetEvent) {
    unsafe {
        let _ = match event {
            HeadsetEvent::Battery(status) => PostMessageW(
                Some(hwnd),
                tray::WM_HID_BATTERY,
                WPARAM(status.percent as usize),
                LPARAM(status.charging as isize),
            ),
            HeadsetEvent::Mute(state) => {
                let muted = matches!(state, hid::MuteState::Muted);
                PostMessageW(
                    Some(hwnd),
                    tray::WM_HID_MUTE,
                    WPARAM(muted as usize),
                    LPARAM(0),
                )
            }
            HeadsetEvent::Connection(connected) => PostMessageW(
                Some(hwnd),
                tray::WM_HID_CONNECTION,
                WPARAM(*connected as usize),
                LPARAM(0),
            ),
        };
    }
}
