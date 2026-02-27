use std::mem;
use windows::core::{w, Result};
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{audio, icon};

// Menu item IDs
const ID_SYNC_MUTE: u16 = 1001;
const ID_EXIT: u16 = 1002;

// Custom window messages for HID thread → main thread communication
pub const WM_HID_BATTERY: u32 = WM_APP + 1;
pub const WM_HID_MUTE: u32 = WM_APP + 2;
pub const WM_HID_CONNECTION: u32 = WM_APP + 3;

// Tray icon callback message
const WM_TRAY_CALLBACK: u32 = WM_APP + 100;

/// Manages the system tray icon lifecycle and context menu.
pub struct TrayIcon {
    hwnd: HWND,
    mic_mute_sync: bool,
    battery_percent: Option<u8>,
    charging: bool,
    connected: bool,
    current_icon: Option<HICON>,
}

impl TrayIcon {
    /// Creates a new tray icon with a hidden message-only window.
    pub fn new() -> Result<Self> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class_name = w!("HyperXToolsTray");

            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassW(&wc);

            // HWND_MESSAGE (-3) creates a message-only window
            let hwnd_message = HWND(-3isize as *mut _);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("HyperXTools"),
                WINDOW_STYLE::default(),
                0,
                0,
                0,
                0,
                Some(hwnd_message),
                None,
                Some(instance.into()),
                None,
            )?;

            let initial_icon = icon::render_disconnected_icon()?;

            let mut nid = NOTIFYICONDATAW {
                cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAY_CALLBACK,
                hIcon: initial_icon,
                ..Default::default()
            };
            set_tooltip(&mut nid, "HyperX: Disconnected");
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);

            Ok(TrayIcon {
                hwnd,
                mic_mute_sync: true,
                battery_percent: None,
                charging: false,
                connected: false,
                current_icon: Some(initial_icon),
            })
        }
    }

    /// Returns the window handle for posting messages from other threads.
    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    fn show_context_menu(&self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            let sync_flags = if self.mic_mute_sync {
                MF_CHECKED
            } else {
                MF_UNCHECKED
            };
            let _ =
                AppendMenuW(menu, MF_STRING | sync_flags, ID_SYNC_MUTE as usize, w!("Sync Mute"));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
            let _ = AppendMenuW(menu, MF_STRING, ID_EXIT as usize, w!("Exit"));

            // Required before TrackPopupMenu for correct dismissal behavior
            let _ = SetForegroundWindow(self.hwnd);
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON, pt.x, pt.y, None, self.hwnd, None);
            let _ = DestroyMenu(menu);
        }
    }

    fn handle_menu_command(&mut self, id: u16) {
        match id {
            ID_SYNC_MUTE => self.mic_mute_sync = !self.mic_mute_sync,
            ID_EXIT => unsafe { PostQuitMessage(0) },
            _ => {}
        }
    }

    fn update_tray_icon(&mut self) {
        let new_icon = if self.connected {
            if let Some(pct) = self.battery_percent {
                icon::render_battery_icon(pct)
            } else {
                icon::render_disconnected_icon()
            }
        } else {
            icon::render_disconnected_icon()
        };

        let Ok(new_icon) = new_icon else {
            return;
        };

        let tooltip = if self.connected {
            if let Some(pct) = self.battery_percent {
                let suffix = if self.charging { " (charging)" } else { "" };
                format!("HyperX: {pct}%{suffix}")
            } else {
                "HyperX: Connected".to_string()
            }
        } else {
            "HyperX: Disconnected".to_string()
        };

        unsafe {
            let mut nid = NOTIFYICONDATAW {
                cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_TIP,
                hIcon: new_icon,
                ..Default::default()
            };
            set_tooltip(&mut nid, &tooltip);
            let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        }

        // Destroy the old icon, keep the new one
        if let Some(old) = self.current_icon.take() {
            unsafe {
                let _ = DestroyIcon(old);
            }
        }
        self.current_icon = Some(new_icon);
    }

    /// Called when a battery or charging report arrives from the HID thread.
    pub fn on_battery(&mut self, percent: u8, charging: bool) {
        if percent > 0 {
            self.battery_percent = Some(percent.min(100));
        }
        // Only update charging state from charging-specific reports (percent == 0)
        // to avoid battery-level reports incorrectly clearing the charging flag.
        if charging || percent == 0 {
            self.charging = charging;
        }
        self.connected = true;
        self.update_tray_icon();
    }

    /// Called when a mute toggle report arrives from the HID thread.
    pub fn on_mute(&mut self, _muted: bool) {
        if self.mic_mute_sync {
            audio::sync_mic_mute();
        }
    }

    /// Called when headset connection state changes.
    pub fn on_connection(&mut self, connected: bool) {
        self.connected = connected;
        if !connected {
            self.battery_percent = None;
            self.charging = false;
        }
        self.update_tray_icon();
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        unsafe {
            let nid = NOTIFYICONDATAW {
                cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..Default::default()
            };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);

            if let Some(icon) = self.current_icon.take() {
                let _ = DestroyIcon(icon);
            }
        }
    }
}

/// Copies a UTF-8 string into the tooltip field of a NOTIFYICONDATAW.
fn set_tooltip(nid: &mut NOTIFYICONDATAW, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    let len = wide.len().min(nid.szTip.len() - 1);
    nid.szTip[..len].copy_from_slice(&wide[..len]);
    nid.szTip[len] = 0;
}

/// Window procedure for the hidden tray message window.
unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut TrayIcon;
    if ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let tray = unsafe { &mut *ptr };

    match msg {
        WM_TRAY_CALLBACK => {
            let event = lparam.0 as u32;
            if event == WM_RBUTTONUP {
                tray.show_context_menu();
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as u16;
            tray.handle_menu_command(id);
            LRESULT(0)
        }
        WM_HID_BATTERY => {
            tray.on_battery(wparam.0 as u8, lparam.0 != 0);
            LRESULT(0)
        }
        WM_HID_MUTE => {
            tray.on_mute(wparam.0 != 0);
            LRESULT(0)
        }
        WM_HID_CONNECTION => {
            tray.on_connection(wparam.0 != 0);
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
