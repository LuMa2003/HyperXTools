# HyperX Cloud Alpha Wireless — USB HID Communication Protocol

## Overview

The HyperX Cloud Alpha Wireless headset communicates with the PC through a proprietary 2.4GHz USB dongle. The dongle appears as a USB composite device exposing both USB Audio Class interfaces (for actual audio streaming) and multiple HID interfaces for control and status. All headset control — mute state, battery level, EQ, sidetone, mic monitoring — travels over vendor-specific HID reports on these control interfaces, not through the audio stream.

## USB Device Identification

The dongle has been manufactured under two brands, resulting in two VID/PID pairs:

| Era | USB VID | USB PID | Notes |
|-----|---------|---------|-------|
| Kingston (pre-2022) | `0x0951` | `0x1743` | Original Kingston branding |
| HP (2022+) | `0x03F0` | `0x098D` | After HP acquired HyperX |

Both use the same protocol. Check for both when scanning for devices.

## USB Interface Layout

The dongle enumerates as a composite device with multiple interfaces:

- **Interface 0–1**: USB Audio Class (isochronous endpoints for mic/speaker audio)
- **Interface 2+**: HID interfaces with different usage pages

The control interface uses a **vendor-specific HID usage page** — not the standard HID Telephony page (`0x0B`) that enterprise headsets use. Known vendor usage pages:

| Usage Page | Notes |
|------------|-------|
| `0xFF00` | Most common for Cloud Alpha Wireless |
| `0xFFC0` | Seen on some firmware versions |
| `0xFF73` | Seen on some firmware versions |

To find the right interface, enumerate all HID interfaces for the VID/PID and look for one with a usage page in the `0xFF00+` range. Ignore interfaces with standard usage pages (keyboard, consumer control, etc.).

## HID Report Format

Communication uses **96-byte HID reports**. The meaningful data is in the first 4 bytes; the remaining bytes are zero-padded.

```
Byte 0: Command category / report identifier
Byte 1: Command type
Byte 2: Sub-command / parameter
Byte 3: Value
Bytes 4–95: Zero padding
```

The protocol is bidirectional:
- **Host → Dongle**: Send a feature/output report to set a value
- **Dongle → Host**: The dongle echoes the same 4-byte prefix as acknowledgment
- **Dongle → Host (unsolicited)**: The dongle pushes interrupt IN reports when headset state changes (mute button press, battery level change, power on/off)

## Mute Button Communication

This is the most important part for mute detection. The mute button is handled **in firmware on the headset itself** — the headset toggles its own mute LED and silences the mic signal before wirelessly transmitting to the dongle. The dongle then notifies the host via an unsolicited HID input report.

### What Windows Sees (and Doesn't See)

When the physical mute button is pressed:

- ✅ The dongle sends a HID interrupt IN report with the new mute state
- ✅ The audio stream switches to true digital silence (every sample = 0)
- ❌ Windows `IAudioEndpointVolume::GetMute()` remains `FALSE`
- ❌ No Windows audio event fires
- ❌ The device does not disappear or change state in Windows Sound settings
- ❌ No standard HID Telephony mute usage is sent

### Mute HID Report Patterns

The exact byte values vary by headset model and firmware version. Known patterns from reverse engineering:

**Cloud Flight pattern:**
```
Muted:   [0x65, 0x04, 0x00, ...]
Unmuted: [0x65, 0x01, 0x00, ...]
```
Report identifier `0x65` with the mute state in byte 1.

**Cloud Alpha / Cloud II pattern:**
```
Muted:   [0x21, 0xBB, 0x10, 0x01, 0x00, ...]
Unmuted: [0x21, 0xBB, 0x10, 0x00, 0x00, ...]
```
Command category `0x21`, command type `0xBB`, with the state in byte 3.

**Important**: Your specific dongle firmware may use different bytes. The reliable way to determine your pattern is a USB capture (see "Identifying Your Specific Pattern" below).

### Mute Reports Are Push-Based

The dongle sends a report **only when the mute button is pressed** — there is no periodic polling needed. Open the HID device, block on read, and you'll receive a report the instant the user presses the button. This makes detection effectively zero-latency.

## Request-Response Protocol

The dongle supports a command-response pattern. To query a value, send an **output report** (via `hid_write()`) with the command byte, and the dongle responds with an **input report** containing the result. All commands use the same envelope:

```
Request:  [0x21, 0xBB, CMD,  0x00, 0x00, ...]  (zero-padded to report size)
Response: [0x21, 0xBB, RESP, DATA, 0x00, ...]
```

Response codes are either the same as the request command (direct reply) or a higher "push" code for unsolicited notifications:

| Purpose | Request CMD | Direct Reply | Push Code | Data (byte 3+) |
|---------|-------------|-------------|-----------|----------------|
| Connection status | `0x03` | `0x03` | `0x24` | `0x01` = disconnected, `0x02` = connected |
| Pairing info | `0x04` | `0x04` | — | Device ID bytes (e.g., `C8 5A CF 59 ED`) |
| Sidetone on/off | `0x05` | `0x05` | — | `0x00` = off, `0x01` = on |
| Sidetone volume | `0x06` | `0x06` | — | Volume level (observed `FF FC` = max/unset) |
| Auto shutdown | `0x07` | `0x07` | — | Minutes (e.g., `0x0A` = 10 min) |
| Mic connected | `0x08` | `0x08` | — | `0x00` = no, `0x01` = yes |
| Voice prompts | `0x09` | `0x09` | — | `0x00` = off, `0x01` = on |
| Mute state | `0x0A` | `0x0A` | `0x23` | `0x00` = unmuted, `0x01` = muted |
| Battery level | `0x0B` | `0x0B` | `0x25` | Byte 3 = percent (0–100). Bytes 4–5 = voltage in mV (u16 BE, e.g., `0F AE` = 4014 mV). Byte 6 = cell count (`0x01`) |
| Charging state | `0x0C` | `0x0C` | `0x26` | `0x00` = not charging, `0x01` = charging |
| Device info | `0x0D` | `0x0D` | — | Device ID bytes (e.g., `C8 5A CF 59 D9`) — related to pairing info |
| Product color | `0x0E` | `0x0E` | — | Color variant (e.g., `0x02`) |

### Set Commands

| Purpose | CMD | Data (byte 3) |
|---------|-----|----------------|
| Set sidetone | `0x10` | `0x00` = off, `0x01` = on |
| Set sidetone volume | `0x11` | Volume level |
| Set auto shutdown | `0x12` | Minutes |
| Set voice prompts | `0x13` | `0x00` = off, `0x01` = on |
| Set mute | `0x15` | `0x00` = unmute, `0x01` = mute |

### Battery Level

To request battery, send `[0x21, 0xBB, 0x0B, 0x00, ...]` via `hid_write()` (output report, NOT feature report). The dongle responds with `[0x21, 0xBB, 0x0B, PERCENT, ...]` and may also send an unsolicited push `[0x21, 0xBB, 0x25, PERCENT, ...]`.

Best practice: first send `0x03` (connection check), then `0x0C` (charging), then `0x0B` (battery).

### Volume Control

The headset's volume wheel sends HID consumer control reports through a separate HID interface (standard Consumer usage page `0x0C`, not the vendor-specific one). These are handled natively by Windows and don't need custom parsing.

## Fallback: Digital Silence Detection

If HID communication fails (driver conflict, permissions, unknown firmware), there's a reliable fallback. When the headset is hardware-muted, the dongle continues streaming USB audio but fills every sample with **exact zeros**. This is distinguishable from an unmuted-but-quiet mic because:

- A hardware-muted mic produces peak meter value = exactly `0.0`
- An unmuted mic in a silent room still has ADC noise floor at ~`0.001–0.01`

Any audio API that exposes peak/RMS metering can detect this. The tradeoff is latency — you need several consecutive zero readings (~1–2 seconds) to distinguish intentional mute from a brief pause in speech.

## Identifying Your Specific Pattern

### Method 1: Wireshark + USBPcap (Recommended)

1. Install [USBPcap](https://desowin.org/usbpcap/) and [Wireshark](https://www.wireshark.org/)
2. Start a Wireshark capture on the USBPcap interface for your dongle's USB root hub
3. Apply display filter: `usb.idVendor == 0x03f0 && usb.idProduct == 0x098d` (adjust for your VID/PID)
4. Press the mute button 3–4 times, waiting a few seconds between presses
5. Look for **interrupt IN transfers** that only appear on mute toggle
6. Compare payloads between mute-on and mute-off — the differing byte is your mute indicator

### Method 2: Raw HID Dump

Open the vendor-specific HID interface in any language, read reports in a loop, and print them. Press the mute button and look for the report that appears:

```
# Pseudocode
device = hid_open(VID, PID, vendor_usage_page)
while true:
    data = device.read(96, timeout=5000ms)
    if data:
        print(hex(data[0:8]))
```

### Method 3: Decompile NGENUITY

NGENUITY is a .NET UWP app. Its assemblies can be decompiled with dnSpy, ILSpy, or dotPeek. Look for HID report parsing logic to find the exact byte offsets and values for every command. The app is installed under:

```
C:\Program Files\WindowsApps\2BBB767C.HyperXNGENUITY_*
```

## Shared HID Access

Windows allows multiple processes to open the same HID device simultaneously for reading. However, HID **input reports** (interrupt IN) are delivered to only one reader — whichever process reads first gets it. If NGENUITY is consuming reports, your listener may miss events.

Solutions:
- Close NGENUITY (it's not needed for the headset to function)
- Use `HidD_GetInputReport()` (Windows) for polled reads instead of interrupt-based reads
- Use the digital silence fallback as a secondary detection method

**Feature reports** and **output reports** (used for sending commands like setting EQ) can be accessed by multiple processes without conflict.

## References

| Project | Language | What It Does |
|---------|----------|-------------|
| [HyperHeadset](https://github.com/LennardKittner/HyperHeadset) | Rust | Full CLI/tray app, supports Cloud Alpha Wireless mute, battery, sidetone |
| [hyperxrebutton](https://github.com/TizianGuth/hyperxrebutton) | C# | Detects HID mute events on Cloud II, triggers keypresses |
| [hyperx-cloud-flight-wireless](https://github.com/srn/hyperx-cloud-flight-wireless) | Node.js | Clean reference implementation of mute/battery detection |
| [hyperx-mute-on-taskbar](https://github.com/sanraith/hyperx-mute-on-taskbar) | C# | Reads mute state via HID, shows in Windows taskbar |
| [hypermicmonitor](https://github.com/acidiclight/hypermicmonitor) | C# | Documents the 96-byte protocol for Cloud Alpha Wireless |
| [HeadsetControl](https://github.com/Sapd/HeadsetControl) | C | Multi-headset support, Cloud Alpha Wireless battery/sidetone (not mute) |
| [alphabat](https://github.com/csmith/alphabat) | Go | Battery status for Cloud Alpha Wireless, confirms VID/PID |
