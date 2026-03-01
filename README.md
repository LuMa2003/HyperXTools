<p align="center">
  <img src="assets/logo.png" alt="HyperXTools logo" width="128">
</p>

# HyperXTools

A lightweight Windows system tray application for the **HyperX Cloud Alpha Wireless** headset. It communicates directly with the USB dongle via HID to provide features that HyperX's own software doesn't:

- **Battery monitoring** — live battery percentage and charging status in the tray icon
- **Mic mute sync** — sends an F13 keypress on hardware mute toggle (bind it to Discord/Teams mute)
- **Automatic mic switching** — swaps the Windows default mic between the HyperX and a main microphone when you mute/unmute the headset

Should work for both Kingston (`0951:1743`) and HP (`03F0:098D`) branded dongles, but I have only tested it with the HP variant.

## Installation

Download the `.msi` installer from the [Releases](https://github.com/LuMa2003/HyperXTools/releases) page.

## Usage

HyperXTools runs in the system tray. Right-click the tray icon to:

- Toggle mic mute sync (F13 keypress)
- Toggle automatic mic switching
- Select your main microphone
- Enable/disable launch at startup

## Setting up Discord mute sync

HyperXTools sends an **F13** keypress every time you press the mute button on your headset. To make Discord respond to it:

1. Open **Discord Settings** > **Keybinds**
2. Click **Add a Keybind** and set the action to **Toggle Mute**
3. Click the keybind field and press the **mute button on your headset** — Discord will register it as `F13`
4. Save and close settings

Now pressing the headset mute button will toggle your Discord mute in sync with the hardware.

> **Tip:** This works with any app that supports custom keybinds (Teams, OBS, etc.) — just bind F13 to the mute/unmute action.

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- Windows 10/11

### Build

```sh
cargo build            # debug build
cargo build --release  # optimized release build (LTO + size-optimized)
```

### Run

```sh
cargo run                    # run the tray app
cargo run --bin hid_logger   # run the HID debug logger (prints raw dongle reports)
```

### Lint & Format

```sh
cargo clippy   # lint
cargo fmt      # format
```

### Building the installer

Requires [WiX Toolset v3](https://wixtoolset.org/) and `cargo-wix`:

```sh
cargo install cargo-wix
cargo wix
```

## TODO

<details>
<summary>Maybe</summary>

- **Populate a WiX ListBox via custom action DLL** — Write a C/Rust DLL custom action that enumerates audio devices and fills a WiX ListBox property. This keeps everything inside the installer wizard but is significantly more work (separate DLL project, WiX custom action interface, etc.).

</details>

## License

MIT
