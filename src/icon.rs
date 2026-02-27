use windows::Win32::UI::WindowsAndMessaging::HICON;

/// Renders a dynamic tray icon showing the battery percentage as text.
pub fn render_battery_icon(_percent: u8) -> windows::core::Result<HICON> {
    // TODO: CreateCompatibleDC from screen DC
    // TODO: Create 16x16 DIB section
    // TODO: Fill background (green/yellow/red based on percent)
    // TODO: DrawTextW with battery number
    // TODO: CreateIconIndirect from bitmap
    todo!()
}

/// Renders a disconnected/unknown state icon.
pub fn render_disconnected_icon() -> windows::core::Result<HICON> {
    // TODO: Render "?" or "X" icon indicating no headset connection
    todo!()
}
