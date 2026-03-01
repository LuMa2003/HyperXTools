use hidapi::HidApi;

/// Known HyperX Cloud Alpha Wireless dongle vendor/product IDs.
const DONGLE_IDS: &[(u16, u16)] = &[
    (0x0951, 0x1743), // Kingston (pre-2022)
    (0x03F0, 0x098D), // HP (2022+)
];

/// HID report size (only first few bytes carry data, rest is zero-padded).
const REPORT_SIZE: usize = 96;

/// Write buffer size (community projects use 31–64; dongle accepts any size).
const WRITE_BUF_SIZE: usize = 64;

// -- Request commands (Host → Dongle): [0x21, 0xBB, CMD, 0x00, ...] --
const CMD_PREFIX: [u8; 2] = [0x21, 0xBB];
const CMD_GET_CONNECTION: u8 = 0x03;
const CMD_GET_BATTERY: u8 = 0x0B;
const CMD_GET_CHARGING: u8 = 0x0C;

// -- Response sub-commands (Dongle → Host): [0x21, 0xBB, SUB, DATA, ...] --
// Direct replies echo the request command byte:
//   0x03 → connection, 0x0A → mute, 0x0B → battery, 0x0C → charging
// Push notifications use higher codes:
//   0x23 → mute, 0x25 → battery, 0x26 → charging
const SUB_CONNECTION: u8 = 0x03;
const SUB_CONNECTION_PUSH: u8 = 0x24;
const SUB_MUTE: u8 = 0x0A;
const SUB_MUTE_PUSH: u8 = 0x23;
const SUB_MUTE_SET: u8 = 0x10; // legacy mute report (some firmware versions)
const SUB_BATTERY: u8 = 0x0B;
const SUB_BATTERY_PUSH: u8 = 0x25;
const SUB_CHARGING: u8 = 0x0C;
const SUB_CHARGING_PUSH: u8 = 0x26;

/// Represents a connection to the HyperX headset dongle.
pub struct HidDevice {
    inner: hidapi::HidDevice,
}

/// Battery status reported by the headset.
#[derive(Debug, Clone, Copy)]
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
#[derive(Debug)]
pub enum HeadsetEvent {
    Battery(BatteryStatus),
    Mute(MuteState),
    /// Headset wireless connection state (true = connected).
    Connection(bool),
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
    /// Reads one raw HID report with a timeout (milliseconds).
    /// Returns the number of bytes read and the buffer.
    pub fn read_raw(&self, timeout_ms: i32) -> Option<(usize, [u8; REPORT_SIZE])> {
        let mut buf = [0u8; REPORT_SIZE];
        match self.inner.read_timeout(&mut buf, timeout_ms) {
            Ok(0) => None, // timeout, no data
            Ok(n) => {
                // Print first 8 bytes (matches protocol's meaningful prefix)
                let show = n.min(8);
                let hex: Vec<String> = buf[..show].iter().map(|b| format!("{b:02X}")).collect();
                println!("<< [{n} bytes] {}", hex.join(" "));
                Some((n, buf))
            }
            Err(e) => {
                eprintln!("HID read error: {e}");
                None
            }
        }
    }

    /// Reads the next HID report and attempts to parse it as a known event.
    pub fn read_event(&self, timeout_ms: i32) -> Option<HeadsetEvent> {
        let (n, buf) = self.read_raw(timeout_ms)?;

        // Cloud Alpha / Cloud II: all vendor reports start with [0x21, 0xBB, sub, ...]
        if n >= 4 && buf[0] == CMD_PREFIX[0] && buf[1] == CMD_PREFIX[1] {
            let sub = buf[2];
            let value = buf[3];

            return match sub {
                // Mute state
                SUB_MUTE | SUB_MUTE_PUSH | SUB_MUTE_SET => {
                    Some(HeadsetEvent::Mute(if value == 0x01 {
                        MuteState::Muted
                    } else {
                        MuteState::Unmuted
                    }))
                }

                // Battery level (percent in byte 3)
                SUB_BATTERY | SUB_BATTERY_PUSH => Some(HeadsetEvent::Battery(BatteryStatus {
                    percent: value,
                    charging: false, // charging state comes from a separate report
                })),

                // Charging state
                SUB_CHARGING | SUB_CHARGING_PUSH => Some(HeadsetEvent::Battery(BatteryStatus {
                    percent: 0, // caller should merge with last known percent
                    charging: value == 0x01,
                })),

                // Connection status
                SUB_CONNECTION | SUB_CONNECTION_PUSH => {
                    // 0x01 = not connected, 0x02 = connected (from HeadsetControl)
                    Some(HeadsetEvent::Connection(value != 0x01))
                }

                _ => {
                    println!(
                        "Unknown report: {:02X} {:02X} {:02X} {:02X} (+ {} bytes)",
                        buf[0],
                        buf[1],
                        buf[2],
                        buf[3],
                        n.saturating_sub(4),
                    );
                    None
                }
            };
        }

        // Cloud Flight mute: [0x65, state, ...]
        if n >= 2 && buf[0] == 0x65 {
            return Some(HeadsetEvent::Mute(if buf[1] == 0x04 {
                MuteState::Muted
            } else {
                MuteState::Unmuted
            }));
        }

        None
    }

    /// Sends a command to the dongle: [0x21, 0xBB, cmd, 0x00, ...]
    pub fn send_command(&self, cmd: u8) -> bool {
        let mut buf = [0u8; WRITE_BUF_SIZE];
        buf[0] = CMD_PREFIX[0];
        buf[1] = CMD_PREFIX[1];
        buf[2] = cmd;
        println!(
            ">> [{} bytes] {:02X} {:02X} {:02X}",
            WRITE_BUF_SIZE, buf[0], buf[1], buf[2]
        );
        match self.inner.write(&buf) {
            Ok(_) => true,
            Err(e) => {
                eprintln!("HID write error (cmd {:#04X}): {e}", cmd);
                false
            }
        }
    }

    /// Requests battery level from the dongle.
    pub fn request_battery(&self) -> bool {
        self.send_command(CMD_GET_BATTERY)
    }

    /// Requests charging status from the dongle.
    pub fn request_charging(&self) -> bool {
        self.send_command(CMD_GET_CHARGING)
    }

    /// Requests connection status from the dongle.
    pub fn request_connection(&self) -> bool {
        self.send_command(CMD_GET_CONNECTION)
    }

}
