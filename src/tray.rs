use windows::Win32::Foundation::HWND;

/// Manages the system tray icon lifecycle and context menu.
pub struct TrayIcon {
    hwnd: HWND,
}

impl TrayIcon {
    /// Creates a new tray icon with a hidden message-only window.
    pub fn new() -> windows::core::Result<Self> {
        // TODO: Register window class
        // TODO: Create message-only window
        // TODO: Call Shell_NotifyIconW with NIM_ADD
        todo!()
    }

    /// Updates the tray icon image (e.g. when battery level changes).
    pub fn update_icon(&self, _battery_percent: u8) {
        // TODO: Generate new icon via icon::render_battery_icon
        // TODO: Call Shell_NotifyIconW with NIM_MODIFY
    }

    /// Displays the right-click context menu.
    pub fn show_context_menu(&self) {
        // TODO: CreatePopupMenu, AppendMenuW, TrackPopupMenu
    }

    /// Processes window messages (WM_APP callbacks from tray icon).
    pub fn handle_message(&self, _msg: u32, _wparam: usize, _lparam: isize) {
        // TODO: Handle WM_RBUTTONUP -> show_context_menu
        // TODO: Handle menu item selections
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        // TODO: Shell_NotifyIconW with NIM_DELETE
        // TODO: DestroyWindow
    }
}
