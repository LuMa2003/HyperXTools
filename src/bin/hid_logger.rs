//! Standalone HID logger for debugging HyperX dongle communication.
//! Prints every raw report with hex bytes and labels for confirmed sub-commands.
//!
//! Usage: cargo run --bin hid_logger

use hidapi::HidApi;

/// Known dongle VID/PID pairs.
const DONGLE_IDS: &[(u16, u16)] = &[
    (0x0951, 0x1743), // Kingston (pre-2022)
    (0x03F0, 0x098D), // HP (2022+)
];

const REPORT_SIZE: usize = 96;
const WRITE_BUF_SIZE: usize = 64;
const READ_TIMEOUT_MS: i32 = 1000;

fn main() {
    println!("HyperX HID Logger");
    println!("==================");
    println!("Searching for dongle...\n");

    let api = HidApi::new().expect("Failed to initialize HID API");
    let device = find_dongle(&api).expect("No HyperX dongle found");

    println!("\nSending initial state requests...");
    send_command(&device, 0x03, "connection");
    send_command(&device, 0x0B, "battery");
    send_command(&device, 0x0C, "charging");
    println!();

    println!("Listening for reports (Ctrl+C to quit)...");
    println!("------------------------------------------");

    loop {
        let mut buf = [0u8; REPORT_SIZE];
        match device.read_timeout(&mut buf, READ_TIMEOUT_MS) {
            Ok(0) => continue,
            Ok(n) => print_report(&buf, n),
            Err(e) => {
                eprintln!("HID read error: {e}");
                break;
            }
        }
    }
}

fn find_dongle(api: &HidApi) -> Option<hidapi::HidDevice> {
    for info in api.device_list() {
        let vid = info.vendor_id();
        let pid = info.product_id();

        if !DONGLE_IDS.iter().any(|&(v, p)| v == vid && p == pid) {
            continue;
        }

        if info.usage_page() < 0xFF00 {
            continue;
        }

        println!(
            "Found dongle: VID={vid:#06X} PID={pid:#06X} usage_page={:#06X} interface={}",
            info.usage_page(),
            info.interface_number(),
        );

        match info.open_device(api) {
            Ok(dev) => {
                println!("Opened HID device successfully");
                return Some(dev);
            }
            Err(e) => eprintln!("Failed to open device: {e}"),
        }
    }

    None
}

fn send_command(device: &hidapi::HidDevice, cmd: u8, label: &str) {
    let mut buf = [0u8; WRITE_BUF_SIZE];
    buf[0] = 0x21;
    buf[1] = 0xBB;
    buf[2] = cmd;
    print!(">> [req {label}] 21 BB {cmd:02X} 00 ... ");
    match device.write(&buf) {
        Ok(_) => println!("ok"),
        Err(e) => println!("error: {e}"),
    }
}

fn print_report(buf: &[u8], n: usize) {
    let show = n.min(8);
    let hex: String = buf[..show]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");

    let label = decode_label(buf, n);
    println!("[raw] {hex:<23} ({label})");
}

fn decode_label(buf: &[u8], n: usize) -> String {
    // Cloud Flight legacy mute: [0x65, state, ...]
    if n >= 2 && buf[0] == 0x65 {
        return if buf[1] == 0x04 {
            "Mute: Muted (Cloud Flight)".into()
        } else {
            format!("Mute: Unmuted (Cloud Flight, val={:#04X})", buf[1])
        };
    }

    // Cloud Alpha / Cloud II: [0x21, 0xBB, sub, data, ...]
    if n >= 4 && buf[0] == 0x21 && buf[1] == 0xBB {
        let sub = buf[2];
        let val = buf[3];

        return match sub {
            // Connection
            0x03 | 0x24 => {
                let push = if sub == 0x24 { " Push" } else { "" };
                if val == 0x01 {
                    format!("Connection{push}: Disconnected")
                } else {
                    format!("Connection{push}: Connected (val={val:#04X})")
                }
            }
            // Mic connected
            0x08 | 0x20 => {
                let push = if sub == 0x20 { " Push" } else { "" };
                if val == 0x01 {
                    format!("Mic{push}: Connected")
                } else {
                    format!("Mic{push}: Removed")
                }
            }
            // Mute
            0x0A | 0x23 => {
                let push = if sub == 0x23 { " Push" } else { "" };
                if val == 0x01 {
                    format!("Mute{push}: Muted")
                } else {
                    format!("Mute{push}: Unmuted (val={val:#04X})")
                }
            }
            // Mute (legacy/set)
            0x10 => {
                if val == 0x01 {
                    "Mute (legacy): Muted".into()
                } else {
                    format!("Mute (legacy): Unmuted (val={val:#04X})")
                }
            }
            // Battery
            0x0B | 0x25 => {
                let push = if sub == 0x25 { " Push" } else { "" };
                format!("Battery{push}: {val}%")
            }
            // Charging
            0x0C | 0x26 => {
                let push = if sub == 0x26 { " Push" } else { "" };
                if val == 0x01 {
                    format!("Charging{push}: Yes")
                } else {
                    format!("Charging{push}: No")
                }
            }
            _ => format!("unknown sub={sub:#04X}"),
        };
    }

    format!("unknown prefix={:#04X}", buf[0])
}
