use hidapi::HidApi;

/// Known HyperX Cloud Alpha Wireless dongle vendor/product IDs.
const DONGLE_IDS: &[(u16, u16)] = &[
    (0x0951, 0x1743), // Kingston (pre-2022)
    (0x03F0, 0x098D), // HP (2022+)
];

/// Represents a connection to the HyperX headset dongle.
pub struct HidDevice {
    inner: hidapi::HidDevice,
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

/// Discovers and opens the HyperX dongle's vendor-specific HID interface.
pub fn find_dongle() -> Option<HidDevice> {
    let api = HidApi::new().ok()?;

    for info in api.device_list() {
        let vid = info.vendor_id();
        let pid = info.product_id();

        if !DONGLE_IDS.iter().any(|&(v, p)| v == vid && p == pid) {
            continue;
        }

        // Only open vendor-specific usage pages (0xFF00+), skip standard HID interfaces
        let usage_page = info.usage_page();
        if usage_page < 0xFF00 {
            continue;
        }

        println!(
            "Found dongle: VID={:#06X} PID={:#06X} usage_page={:#06X} interface={}",
            vid,
            pid,
            usage_page,
            info.interface_number(),
        );

        match info.open_device(&api) {
            Ok(device) => {
                println!("Opened HID device successfully");
                return Some(HidDevice { inner: device });
            }
            Err(e) => {
                println!("Failed to open device: {e}");
            }
        }
    }

    None
}

impl HidDevice {
    /// Reads one raw 96-byte HID report with a timeout (milliseconds).
    /// Returns the number of bytes read and the buffer.
    pub fn read_raw(&self, timeout_ms: i32) -> Option<(usize, [u8; 96])> {
        let mut buf = [0u8; 96];
        match self.inner.read_timeout(&mut buf, timeout_ms) {
            Ok(0) => None,        // timeout, no data
            Ok(n) => Some((n, buf)),
            Err(e) => {
                eprintln!("HID read error: {e}");
                None
            }
        }
    }

    /// Reads the next HID report and attempts to parse it as a known event.
    pub fn read_event(&self) -> Option<HeadsetEvent> {
        let (_n, buf) = self.read_raw(5000)?;

        // Cloud Alpha / Cloud II mute: [0x21, 0xBB, sub, state]
        // sub varies by firmware: 0x10 or 0x23
        if buf[0] == 0x21 && buf[1] == 0xBB {
            return Some(HeadsetEvent::Mute(if buf[3] == 0x01 {
                MuteState::Muted
            } else {
                MuteState::Unmuted
            }));
        }

        // Cloud Flight mute: [0x65, state, ...]
        if buf[0] == 0x65 {
            return Some(HeadsetEvent::Mute(if buf[1] == 0x04 {
                MuteState::Muted
            } else {
                MuteState::Unmuted
            }));
        }

        // Battery: [0x21, 0xFF, percent, charging_state]
        if buf[0] == 0x21 && buf[1] == 0xFF {
            return Some(HeadsetEvent::Battery(BatteryStatus {
                percent: buf[2],
                charging: buf[3] != 0x00,
            }));
        }

        None
    }

    /// Sends a feature report to query battery status.
    pub fn request_battery(&self) {
        // TODO: Send feature report to request battery info
    }
}
