#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// Prints to console only in debug builds (when console is visible).
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        println!($($arg)*);
    };
}

mod audio;
mod autostart;
mod config;
mod hid;
mod icon;
mod mic_picker;
mod tray;
mod updater;

use hid::HeadsetEvent;
use std::thread;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// Wrapper to send HWND across thread boundaries.
/// SAFETY: HWND is a numeric handle; PostMessageW is explicitly thread-safe.
struct SendHwnd(HWND);
unsafe impl Send for SendHwnd {}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    debug_log!("[main] args: {:?}", args);

    // Handle --replace-exe CLI arg: elevated file swap for auto-update
    if let Some(pos) = args.iter().position(|a| a == "--replace-exe") {
        let remaining = &args[pos + 1..];
        match updater::handle_replace_exe(remaining) {
            Ok(()) => std::process::exit(0),
            Err(_) => std::process::exit(1),
        }
    }

    // Handle --select-mic CLI arg: show picker, save config, exit
    if args.iter().any(|a| a == "--select-mic") {
        debug_log!("[main] --select-mic mode");
        let _com = audio::init_com();
        if let Some(device) = mic_picker::show_mic_picker() {
            debug_log!(
                "[main] user selected: name={:?} id={:?}",
                device.name,
                device.id
            );
            let mut config = config::Config::load();
            config.main_mic_id = Some(device.id);
            config.main_mic_name = Some(device.name);
            config.save();
            debug_log!("[main] config saved");
        } else {
            debug_log!("[main] user cancelled mic picker");
        }
        return;
    }

    // Initialize COM for the main thread — kept alive for the app's lifetime
    // so that mic switching and the mic picker can use audio APIs directly.
    let _com = audio::init_com();

    let config = config::Config::load();
    let skipped_version = config.skipped_version.clone();
    debug_log!(
        "[main] config loaded: mic_switching={}, main_mic_id={:?}",
        config.mic_switching,
        config.main_mic_id
    );

    // Look up the HyperX audio device ID once at startup.
    // Exits with an error dialog if no HyperX dongle is found.
    let hyperx_mic_id = audio::require_hyperx_device();
    debug_log!("[main] hyperx_mic_id={:?}", hyperx_mic_id);

    let tray =
        Box::new(tray::TrayIcon::new(config, hyperx_mic_id).expect("Failed to create tray icon"));
    let hwnd = tray.hwnd();

    // Leak TrayIcon into a raw pointer for wndproc access via GWLP_USERDATA
    let tray_ptr = Box::into_raw(tray);
    unsafe {
        SetWindowLongPtrW((*tray_ptr).hwnd(), GWLP_USERDATA, tray_ptr as isize);
    }

    // Spawn background HID communication thread
    let send_hwnd = SendHwnd(hwnd);
    thread::spawn(move || hid_thread(send_hwnd));

    // Spawn background update check (5-second delay to avoid slowing startup)
    let update_hwnd = hwnd.0 as usize;
    thread::spawn(move || {
        thread::sleep(std::time::Duration::from_secs(5));
        if let Some(info) = updater::check_for_update(skipped_version.as_deref()) {
            let boxed = Box::new(info);
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(update_hwnd as *mut _)),
                    tray::WM_UPDATE_AVAILABLE,
                    WPARAM(0),
                    LPARAM(Box::into_raw(boxed) as isize),
                );
            }
        }
    });

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
