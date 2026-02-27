// Uncomment for release builds to hide console window:
// #![windows_subsystem = "windows"]

mod audio;
mod config;
mod hid;
mod icon;
mod tray;

fn main() {
    println!("HyperXTools — HID report logger");
    println!("Searching for HyperX dongle...\n");

    let device = match hid::find_dongle() {
        Some(d) => d,
        None => {
            eprintln!("No HyperX dongle found!");
            eprintln!("Make sure the dongle is plugged in and NGENUITY is closed.");
            eprintln!("\nPress Enter to exit...");
            let _ = std::io::stdin().read_line(&mut String::new());
            return;
        }
    };

    println!("\nListening for HID reports (press Ctrl+C to stop)...\n");

    loop {
        if let Some((n, buf)) = device.read_raw(5000) {
            // Print first 8 bytes as hex (only first 4 carry data, but show a few extra)
            print!("[{n:>2} bytes] ");
            for b in &buf[..8] {
                print!("{b:02X} ");
            }

            // Try to parse as a known event
            if buf[0] == 0x21 && buf[1] == 0xBB {
                let state = if buf[3] == 0x01 { "MUTED" } else { "UNMUTED" };
                print!(" -> Mute: {state}");
            } else if buf[0] == 0x65 {
                let state = if buf[1] == 0x04 { "MUTED" } else { "UNMUTED" };
                print!(" -> Mute (Flight): {state}");
            } else if buf[0] == 0x21 && buf[1] == 0xFF {
                let pct = buf[2];
                let charging = if buf[3] != 0 { "charging" } else { "discharging" };
                print!(" -> Battery: {pct}% ({charging})");
            } else {
                print!(" -> Unknown");
            }

            println!();
        }
    }
}
