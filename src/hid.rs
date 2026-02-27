use hidapi::HidApi;

/// Known HyperX Cloud Alpha Wireless dongle vendor/product IDs.
const DONGLE_IDS: &[(u16, u16)] = &[
    (0x0951, 0x1743), // Kingston (pre-2022)
    (0x03F0, 0x098D), // HP (2022+)
];

/// Represents a connection to the HyperX headset dongle.
pub struct HidDevice {
    // TODO: Store hidapi::HidDevice handle
}

/// Battery status reported by the headset.
pub struct BatteryStatus {
    pub percent: u8,
    pub charging: bool,
}

/// Mute state reported by the headset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteState {
    Muted,
    Unmuted,
}

/// Events received from the headset via HID reports.
pub enum HeadsetEvent {
    Battery(BatteryStatus),
    Mute(MuteState),
}

/// Discovers and opens the HyperX dongle HID device.
pub fn find_dongle() -> Option<HidDevice> {
    // TODO: Initialize HidApi
    // TODO: Iterate devices, match DONGLE_IDS
    // TODO: Open device and return HidDevice
    let _api = HidApi::new().ok()?;
    None
}

impl HidDevice {
    /// Reads the next HID report from the dongle (blocking).
    pub fn read_event(&self) -> Option<HeadsetEvent> {
        // TODO: Read 96-byte report
        // TODO: Parse battery: [0x21, 0xFF, percent, charging_state]
        // TODO: Parse mute: [0x21, 0xBB, 0x10, 0x01] / [0x21, 0xBB, 0x10, 0x00]
        None
    }

    /// Sends a feature report to query battery status.
    pub fn request_battery(&self) {
        // TODO: Send feature report to request battery info
    }
}
