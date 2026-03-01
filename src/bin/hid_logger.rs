//! Standalone HID logger for debugging HyperX dongle communication.
//! Prints every raw report with hex bytes and labels for confirmed sub-commands.
//!
//! Usage: cargo run --bin hid_logger

use hidapi::HidApi;
use std::ffi::CString;
use std::io::{self, Write};

/// Known dongle VID/PID pairs.
const DONGLE_IDS: &[(u16, u16)] = &[
    (0x0951, 0x1743), // Kingston (pre-2022)
    (0x03F0, 0x098D), // HP (2022+)
];

const REPORT_SIZE: usize = 96;
const WRITE_BUF_SIZE: usize = 64;
const READ_TIMEOUT_MS: i32 = 1000;

struct DeviceEntry {
    vid: u16,
    pid: u16,
    usage_page: u16,
    usage: u16,
    interface: i32,
    product: String,
    manufacturer: String,
    path: CString,
    is_hyperx: bool,
}

fn main() {
    println!("HyperX HID Logger");
    println!("==================\n");

    let api = HidApi::new().expect("Failed to initialize HID API");

    let devices = enumerate_devices(&api);
    if devices.is_empty() {
        eprintln!("No HID devices found.");
        return;
    }

    print_device_list(&devices);

    let selection = prompt_selection(devices.len());
    let entry = &devices[selection];

    println!(
        "\nOpening: {} (VID={:#06X} PID={:#06X})...",
        if entry.product.is_empty() {
            "Unknown Device"
        } else {
            &entry.product
        },
        entry.vid,
        entry.pid,
    );

    let device = api
        .open_path(entry.path.as_c_str())
        .expect("Failed to open selected device");

    if entry.is_hyperx {
        println!("Detected HyperX dongle — sending initial state requests...");
        send_command(&device, 0x03, "connection");
        send_command(&device, 0x0B, "battery");
        send_command(&device, 0x0C, "charging");
    }

    println!("\nListening for reports (Ctrl+C to quit)...");
    println!("------------------------------------------");

    loop {
        let mut buf = [0u8; REPORT_SIZE];
        match device.read_timeout(&mut buf, READ_TIMEOUT_MS) {
            Ok(0) => continue,
            Ok(n) => print_report(&buf, n, entry.is_hyperx),
            Err(e) => {
                eprintln!("HID read error: {e}");
                break;
            }
        }
    }
}

fn enumerate_devices(api: &HidApi) -> Vec<DeviceEntry> {
    let mut devices = Vec::new();

    for info in api.device_list() {
        let vid = info.vendor_id();
        let pid = info.product_id();
        let is_hyperx = DONGLE_IDS.iter().any(|&(v, p)| v == vid && p == pid)
            && info.usage_page() >= 0xFF00;

        devices.push(DeviceEntry {
            vid,
            pid,
            usage_page: info.usage_page(),
            usage: info.usage(),
            interface: info.interface_number(),
            product: info
                .product_string()
                .unwrap_or_default()
                .to_string(),
            manufacturer: info
                .manufacturer_string()
                .unwrap_or_default()
                .to_string(),
            path: info.path().to_owned(),
            is_hyperx,
        });
    }

    // Sort: HyperX devices first, then by VID/PID
    devices.sort_by(|a, b| {
        b.is_hyperx
            .cmp(&a.is_hyperx)
            .then(a.vid.cmp(&b.vid))
            .then(a.pid.cmp(&b.pid))
            .then(a.interface.cmp(&b.interface))
    });

    devices
}

fn print_device_list(devices: &[DeviceEntry]) {
    println!("Available HID devices:");
    println!("{:-<90}", "");

    for (i, dev) in devices.iter().enumerate() {
        let name = if dev.product.is_empty() {
            "Unknown Device".to_string()
        } else if dev.manufacturer.is_empty() {
            dev.product.clone()
        } else {
            format!("{} {}", dev.manufacturer, dev.product)
        };

        let tag = if dev.is_hyperx { " [HyperX]" } else { "" };

        println!(
            "  [{:>2}] {}{}\n       VID={:#06X} PID={:#06X}  usage_page={:#06X} usage={:#06X}  interface={}",
            i + 1,
            name,
            tag,
            dev.vid,
            dev.pid,
            dev.usage_page,
            dev.usage,
            dev.interface,
        );
    }

    println!("{:-<90}", "");
}

fn prompt_selection(count: usize) -> usize {
    loop {
        print!("\nSelect device [1-{count}]: ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        if let Ok(n) = input.trim().parse::<usize>()
            && n >= 1
            && n <= count
        {
            return n - 1;
        }
        println!("Invalid selection. Enter a number between 1 and {count}.");
    }
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

fn print_report(buf: &[u8], n: usize, decode: bool) {
    let show = n.min(8);
    let hex: String = buf[..show]
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ");

    if decode {
        let label = decode_label(buf, n);
        println!("[raw] {hex:<23} ({label})");
    } else {
        let full_hex: String = buf[..n]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ");
        println!("[raw] [{n} bytes] {full_hex}");
    }
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
