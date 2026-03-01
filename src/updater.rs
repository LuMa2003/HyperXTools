//! Auto-update: check GitHub releases, show update dialog, download and replace exe.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const GITHUB_API_URL: &str = "https://api.github.com/repos/LuMa2003/HyperXTools/releases/latest";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Edit control styles
const ES_MULTILINE: u32 = 0x0004;
const ES_READONLY: u32 = 0x0800;
const ES_AUTOVSCROLL: u32 = 0x0040;

// Dialog layout
const DLG_WIDTH: i32 = 420;
const DLG_HEIGHT: i32 = 300;
const ID_UPDATE_NOW: i32 = 200;
const ID_SKIP_VERSION: i32 = 201;
const ID_REMIND_LATER: i32 = 202;
const ID_CHANGELOG: i32 = 203;
const ID_TITLE_LABEL: i32 = 204;

/// Information about an available update.
pub struct UpdateInfo {
    pub version: String,
    pub changelog: String,
    pub download_url: String,
}

/// User's choice from the update dialog.
pub enum UpdateChoice {
    UpdateNow,
    SkipVersion,
    RemindLater,
}

/// State stored in GWLP_USERDATA during the update dialog's lifetime.
struct DialogState {
    choice: UpdateChoice,
    done: bool,
}

/// Checks GitHub for a newer release.
/// Returns `None` if up-to-date, skipped, or on any error (silent failure).
pub fn check_for_update(skipped_version: Option<&str>) -> Option<UpdateInfo> {
    use std::time::Duration;

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();

    let mut resp = agent
        .get(GITHUB_API_URL)
        .header("User-Agent", &format!("HyperXTools/{CURRENT_VERSION}"))
        .header("Accept", "application/vnd.github.v3+json")
        .call()
        .ok()?;

    let body = resp.body_mut().read_to_string().ok()?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;

    let tag = json["tag_name"].as_str()?;
    let version_str = tag.strip_prefix('v').unwrap_or(tag);

    let remote: semver::Version = version_str.parse().ok()?;
    let current: semver::Version = CURRENT_VERSION.parse().ok()?;

    if remote <= current {
        return None;
    }

    if let Some(skipped) = skipped_version
        && skipped == version_str
    {
        return None;
    }

    let changelog = json["body"].as_str().unwrap_or("").to_string();

    let assets = json["assets"].as_array()?;
    let exe_asset = assets
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".exe")))?;
    let download_url = exe_asset["browser_download_url"].as_str()?.to_string();

    Some(UpdateInfo {
        version: version_str.to_string(),
        changelog,
        download_url,
    })
}

/// Shows a modal update dialog. Returns the user's choice.
pub fn show_update_dialog(info: &UpdateInfo) -> UpdateChoice {
    unsafe {
        let Some(instance) = GetModuleHandleW(None).ok() else {
            return UpdateChoice::RemindLater;
        };
        let class_name = w!("HyperXToolsUpdater");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(update_wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as *mut _),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - DLG_WIDTH) / 2;
        let y = (screen_h - DLG_HEIGHT) / 2;

        let Ok(hwnd) = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_name,
            w!("HyperXTools \u{2014} Update Available"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            x,
            y,
            DLG_WIDTH,
            DLG_HEIGHT,
            None,
            None,
            Some(instance.into()),
            None,
        ) else {
            return UpdateChoice::RemindLater;
        };

        let font = GetStockObject(DEFAULT_GUI_FONT);

        // Title label
        let title = format!(
            "Update available: v{} (current: v{CURRENT_VERSION})",
            info.version
        );
        let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        if let Ok(label) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            PCWSTR(title_w.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            12,
            10,
            390,
            20,
            Some(hwnd),
            Some(HMENU(ID_TITLE_LABEL as *mut _)),
            Some(instance.into()),
            None,
        ) {
            SendMessageW(
                label,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }

        // Changelog (read-only multiline edit)
        let cl = if info.changelog.len() > 500 {
            format!("{}...", &info.changelog[..500])
        } else {
            info.changelog.clone()
        };
        let cl_crlf = cl.replace('\n', "\r\n");
        let cl_w: Vec<u16> = cl_crlf.encode_utf16().chain(std::iter::once(0)).collect();
        if let Ok(edit) = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            PCWSTR(cl_w.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL
                | WINDOW_STYLE(ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL),
            12,
            36,
            390,
            170,
            Some(hwnd),
            Some(HMENU(ID_CHANGELOG as *mut _)),
            Some(instance.into()),
            None,
        ) {
            SendMessageW(
                edit,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }

        // Buttons
        let btn_y = 220;
        let btn_w = 120;
        let btn_h = 30;
        let gap = 10;

        // Update Now (default)
        if let Ok(btn) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Update Now"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            12,
            btn_y,
            btn_w,
            btn_h,
            Some(hwnd),
            Some(HMENU(ID_UPDATE_NOW as *mut _)),
            Some(instance.into()),
            None,
        ) {
            SendMessageW(
                btn,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }

        // Skip Version
        if let Ok(btn) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Skip Version"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            12 + btn_w + gap,
            btn_y,
            btn_w,
            btn_h,
            Some(hwnd),
            Some(HMENU(ID_SKIP_VERSION as *mut _)),
            Some(instance.into()),
            None,
        ) {
            SendMessageW(
                btn,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }

        // Remind Me Later
        if let Ok(btn) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Remind Me Later"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            12 + (btn_w + gap) * 2,
            btn_y,
            btn_w,
            btn_h,
            Some(hwnd),
            Some(HMENU(ID_REMIND_LATER as *mut _)),
            Some(instance.into()),
            None,
        ) {
            SendMessageW(
                btn,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }

        // Store state
        let state = Box::new(DialogState {
            choice: UpdateChoice::RemindLater,
            done: false,
        });
        let state_ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        // Modal message loop
        loop {
            let mut msg = MSG::default();
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if !ret.as_bool() {
                PostQuitMessage(msg.wParam.0 as i32);
                let _ = DestroyWindow(hwnd);
                break;
            }
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if (*state_ptr).done {
                break;
            }
        }

        let state = Box::from_raw(state_ptr);
        let _ = UnregisterClassW(class_name, Some(instance.into()));
        state.choice
    }
}

unsafe extern "system" fn update_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut DialogState;

    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            if !ptr.is_null() {
                let state = unsafe { &mut *ptr };
                match id {
                    ID_UPDATE_NOW => {
                        state.choice = UpdateChoice::UpdateNow;
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                    ID_SKIP_VERSION => {
                        state.choice = UpdateChoice::SkipVersion;
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                    ID_REMIND_LATER => {
                        state.choice = UpdateChoice::RemindLater;
                        unsafe {
                            let _ = DestroyWindow(hwnd);
                        }
                    }
                    _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // Close button (X) = Remind Me Later
            if !ptr.is_null() {
                unsafe {
                    (*ptr).choice = UpdateChoice::RemindLater;
                }
            }
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            if !ptr.is_null() {
                unsafe {
                    (*ptr).done = true;
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Downloads the new exe and replaces the running one via rename swap.
pub fn download_and_replace(download_url: &str) -> Result<PathBuf, String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("Failed to get current exe path: {e}"))?;
    let dir = current_exe.parent().ok_or("Failed to get exe directory")?;
    let new_path = dir.join("hyperxtools.exe.new");
    let old_path = dir.join("hyperxtools.exe.old");

    // Clean up leftover from previous update
    let _ = std::fs::remove_file(&old_path);

    // Download new exe
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .into();

    let mut resp = agent
        .get(download_url)
        .header("User-Agent", &format!("HyperXTools/{CURRENT_VERSION}"))
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;

    let mut file =
        std::fs::File::create(&new_path).map_err(|e| format!("Failed to create temp file: {e}"))?;
    let mut reader = resp.body_mut().as_reader();
    std::io::copy(&mut reader, &mut file).map_err(|e| format!("Failed to write download: {e}"))?;
    drop(file);

    // Rename swap: current → .old, then .new → current
    match do_rename_swap(&current_exe, &new_path, &old_path) {
        Ok(()) => Ok(current_exe),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Need elevation (e.g. Program Files) — re-run with --replace-exe via runas
            elevate_replace(&current_exe, &new_path, &old_path)?;
            Ok(current_exe)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&new_path);
            Err(format!("Failed to replace exe: {e}"))
        }
    }
}

/// Renames current → .old, then .new → current. Rolls back on failure.
fn do_rename_swap(current: &Path, new_path: &Path, old_path: &Path) -> std::io::Result<()> {
    std::fs::rename(current, old_path)?;
    if let Err(e) = std::fs::rename(new_path, current) {
        // Rollback: restore original
        let _ = std::fs::rename(old_path, current);
        return Err(e);
    }
    Ok(())
}

/// Runs the current binary elevated with `--replace-exe` to perform the swap.
fn elevate_replace(current: &Path, new_path: &Path, old_path: &Path) -> Result<(), String> {
    let params = format!(
        "--replace-exe \"{}\" \"{}\" \"{}\"",
        new_path.display(),
        current.display(),
        old_path.display()
    );
    let exe_w: Vec<u16> = current
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let params_w: Vec<u16> = params.encode_utf16().chain(std::iter::once(0)).collect();

    let result = unsafe {
        ShellExecuteW(
            None,
            w!("runas"),
            PCWSTR(exe_w.as_ptr()),
            PCWSTR(params_w.as_ptr()),
            None,
            SW_HIDE,
        )
    };

    if (result.0 as isize) <= 32 {
        return Err("UAC elevation was cancelled or failed".to_string());
    }

    // Poll for the elevated process to complete the rename
    for _ in 0..30 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if !new_path.exists() && old_path.exists() {
            return Ok(());
        }
    }
    Err("Elevated rename timed out".to_string())
}

/// Handles the `--replace-exe <new> <target> <old>` CLI mode (runs elevated).
pub fn handle_replace_exe(args: &[String]) -> Result<(), String> {
    if args.len() < 3 {
        return Err("Usage: --replace-exe <new_path> <target_path> <old_path>".to_string());
    }
    let new_path = Path::new(&args[0]);
    let target_path = Path::new(&args[1]);
    let old_path = Path::new(&args[2]);

    do_rename_swap(target_path, new_path, old_path)
        .map_err(|e| format!("Elevated rename failed: {e}"))
}

/// Spawns the updated exe and exits the current process.
pub fn relaunch_and_exit(exe_path: &Path) -> ! {
    let _ = std::process::Command::new(exe_path).spawn();
    std::process::exit(0)
}
