# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

HyperXTools is a **Windows system tray application** for the **HyperX Cloud Alpha Wireless** headset. It communicates with the USB dongle via vendor-specific HID reports to provide battery monitoring, mic mute sync, automatic mic swapping, and auto-updates — features not exposed through standard Windows audio APIs.

The project uses **Rust** (edition 2024) with **`windows-rs`** (raw Win32 APIs) for the tray icon/GDI rendering and **`hidapi`** for HID communication. Protocol documentation lives in `hyperx-hid-protocol.md`.

**Current version: 1.0.0**

## Key Technical Context

- **Dongle VID/PIDs**: Kingston `0x0951:0x1743` (pre-2022) and HP `0x03F0:0x098D` (2022+) — both use the same protocol
- **HID reports are 96 bytes**: only the first 4 bytes carry data, rest is zero-padded
- **Vendor-specific usage pages** (`0xFF00`, `0xFFC0`, `0xFF73`) — not standard HID Telephony
- **Mute is push-based**: the dongle sends an interrupt IN report on button press, no polling needed
- **Windows audio APIs don't reflect hardware mute** — `GetMute()` stays FALSE, no events fire
- **Fallback detection**: hardware mute produces exact digital silence (peak = 0.0) vs. unmuted noise floor (~0.001–0.01)
- **Shared HID access**: Windows delivers interrupt IN reports to only one reader; feature/output reports can be shared

## Protocol Byte Patterns

Command format: `[0x21, 0xBB, CMD, DATA, ...]` — responses echo the same CMD or use higher push codes.

Key report patterns:
- **Mute**: `[0x21, 0xBB, 0x0A/0x23/0x10, 0x01, ...]` (muted) / `[..., 0x00, ...]` (unmuted)
- **Battery**: `[0x21, 0xBB, 0x0B/0x25, PERCENT, ...]`
- **Charging**: `[0x21, 0xBB, 0x0C/0x26, 0x00/0x01, ...]`
- **Connection**: `[0x21, 0xBB, 0x03/0x24, 0x01/0x02, ...]` (disconnected/connected)
- **Cloud Flight (legacy)**: `[0x65, 0x04, ...]` (muted) / `[0x65, 0x01, ...]` (unmuted)

Exact bytes depend on firmware version — always verify with a USB capture.

## Project Structure

```
src/
├── main.rs              — Entry point: single-instance mutex, CLI args, Win32 message loop
├── tray.rs              — System tray icon lifecycle, context menu, message handling
├── hid.rs               — HID device discovery, connection, report parsing (via hidapi)
├── icon.rs              — Dynamic tray icon rendering via GDI (battery %, colored icons)
├── audio.rs             — Windows Core Audio integration (mic mute sync, mic swapping)
├── config.rs            — Configuration persistence (%APPDATA%/HyperXTools/config.toml)
├── autostart.rs         — Windows registry Run key management for launch-on-boot
├── mic_picker.rs        — Modal Win32 dialog for selecting the main microphone
├── updater.rs           — Auto-update via GitHub Releases (download, SHA-256 verify, UAC elevate)
└── bin/
    └── hid_logger.rs    — Standalone HID logger for debugging dongle communication

assets/
├── logo.ico             — App icon (embedded in exe via build.rs)
├── logo.png             — Documentation logo
├── installer-banner.bmp — WiX installer top banner
└── installer-dialog.bmp — WiX installer background

wix/
└── main.wxs             — WiX v3 installer definition (per-machine, Program Files)
```

## Architecture

- **Win32 message loop** in `main.rs` drives the app — no async runtime, all sync
- **Tray icon** uses `Shell_NotifyIconW` with a hidden message-only HWND
- **Dynamic icons** rendered via GDI (`CreateCompatibleDC`, `CreateDIBSection`, `CreateIconIndirect`)
- **HID reads** on a background thread; posts custom messages (`WM_HID_BATTERY`, `WM_HID_MUTE`, `WM_HID_CONNECTION`) to the tray HWND
- **Update check** on a separate background thread; posts `WM_UPDATE_AVAILABLE`
- **Mute sync vs mic switching** are mutually exclusive features — enabling one disables the other
- **CLI modes**: `--select-mic` (mic picker dialog), `--replace-exe` (elevated file swap for auto-update)
- **No console window** — `#![windows_subsystem = "windows"]` suppresses it

## Key Dependencies

- `hidapi` — HID device communication
- `windows` (0.61) — Raw Win32 APIs (Shell, GDI, Audio, Registry, Threading, etc.)
- `serde` + `toml` — Config serialization to `%APPDATA%/HyperXTools/config.toml`
- `serde_json` — GitHub API response parsing
- `ureq` — Sync HTTP client (no async runtime needed)
- `semver` — Version comparison for auto-update
- `sha2` — SHA-256 verification of downloaded updates
- `com-policy-config` — Undocumented Windows API for changing default audio endpoint
- `winresource` (build) — Embeds app icon into exe

## Build & Development

- `cargo build` — debug build
- `cargo build --release` — optimized release build (LTO + size-optimized)
- `cargo run` — build and run (main tray app)
- `cargo run --bin hid_logger` — run the HID debug logger (console, interactive device selection)
- `cargo test` — run tests
- `cargo clippy` — lint checks
- `cargo fmt` — format code
- `cargo wix --no-build` — build MSI installer (requires cargo-wix 0.3.4+)

## Release Process

Tags matching `v*` trigger `.github/workflows/release.yml`:
1. Builds release binary (`cargo build --release`)
2. Builds MSI installer (`cargo wix`)
3. Creates GitHub Release with exe and msi artifacts

## Related Projects

Seven community implementations exist in Rust, C#, Node.js, C, and Go — all listed in `hyperx-hid-protocol.md` under References. These serve as implementation references.
