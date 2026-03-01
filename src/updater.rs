//! Auto-update: check GitHub releases, show update dialog, download and replace exe.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::WaitForSingleObject;
use windows::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const GITHUB_API_URL: &str = "https://api.github.com/repos/LuMa2003/HyperXTools/releases/latest";
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Allowed URL prefixes for download URLs (security: prevent redirects to arbitrary hosts).
const ALLOWED_URL_PREFIXES: &[&str] = &[
    "https://github.com/",
    "https://objects.githubusercontent.com/",
];

// Win32 edit control styles — not exported by the `windows` crate.
// See: https://learn.microsoft.com/en-us/windows/win32/controls/edit-control-styles
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
    pub expected_sha256: Option<String>,
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

/// Posts a `WM_UPDATE_AVAILABLE` message with a heap-allocated `UpdateInfo`.
/// Handles `Box::into_raw` + `PostMessageW`, and cleans up on failure.
///
/// # Safety
/// `hwnd` must be a valid window handle for `PostMessageW`.
pub unsafe fn post_update_info(hwnd: HWND, msg: u32, info: UpdateInfo) {
    let boxed_ptr = Box::into_raw(Box::new(info));
    unsafe {
        if PostMessageW(
            Some(hwnd),
            msg,
            WPARAM(0),
            LPARAM(boxed_ptr as isize),
        )
        .is_err()
        {
            let _ = Box::from_raw(boxed_ptr);
        }
    }
}

/// Removes the leftover `.old` file from a previous update, if it exists.
pub fn cleanup_old_exe() {
    if let Ok(current) = std::env::current_exe() {
        let old_path = current.with_extension("exe.old");
        let _ = std::fs::remove_file(old_path);
    }
}

/// Checks GitHub for a newer release.
/// Returns `Ok(None)` if up-to-date or skipped, `Ok(Some(...))` if an update is available,
/// or `Err(...)` on network/parse errors.
pub fn check_for_update(skipped_version: Option<&str>) -> Result<Option<UpdateInfo>, String> {
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
        .map_err(|e| match e {
            ureq::Error::StatusCode(403) => {
                "GitHub API rate limit exceeded. Please try again in a few minutes.".to_string()
            }
            ureq::Error::StatusCode(code) => format!("GitHub API returned HTTP {code}"),
            _ => format!("Network error: {e}"),
        })?;

    let body = resp
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {e}"))?;
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("Invalid JSON: {e}"))?;

    let tag = json["tag_name"]
        .as_str()
        .ok_or("Missing tag_name in release")?;
    let version_str = tag.strip_prefix('v').unwrap_or(tag);

    let remote: semver::Version = version_str
        .parse()
        .map_err(|e| format!("Invalid version '{version_str}': {e}"))?;
    let current: semver::Version = CURRENT_VERSION
        .parse()
        .map_err(|e| format!("Invalid current version: {e}"))?;

    if remote <= current {
        return Ok(None);
    }

    if let Some(skipped) = skipped_version
        && skipped == version_str
    {
        return Ok(None);
    }

    let changelog = json["body"].as_str().unwrap_or("").to_string();

    let assets = json["assets"]
        .as_array()
        .ok_or("Missing assets in release")?;
    let exe_asset = assets
        .iter()
        .find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".exe")))
        .ok_or("No .exe asset found in release")?;
    let download_url = exe_asset["browser_download_url"]
        .as_str()
        .ok_or("Missing download URL for exe asset")?
        .to_string();

    // Extract SHA-256 digest from the API response (format: "sha256:<hex>")
    let expected_sha256 = exe_asset["digest"]
        .as_str()
        .and_then(|d| d.strip_prefix("sha256:"))
        .map(|h| h.to_lowercase());

    Ok(Some(UpdateInfo {
        version: version_str.to_string(),
        changelog,
        download_url,
        expected_sha256,
    }))
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
        let cl = {
            const MAX_CHARS: usize = 500;
            let mut chars = info.changelog.chars();
            let truncated: String = chars.by_ref().take(MAX_CHARS).collect();
            if chars.next().is_some() {
                format!("{truncated}...")
            } else {
                truncated
            }
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

/// Creates a small topmost popup showing "Downloading update...".
/// Returns the window handle so it can be dismissed later.
pub fn show_download_progress() -> Option<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let w = 260;
        let h = 60;
        let x = (screen_w - w) / 2;
        let y = (screen_h - h) / 2;

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            w!("STATIC"),
            w!("  Downloading update..."),
            WS_POPUP | WS_VISIBLE | WS_BORDER,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;

        let font = GetStockObject(DEFAULT_GUI_FONT);
        SendMessageW(
            hwnd,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        let _ = UpdateWindow(hwnd);
        Some(hwnd)
    }
}

/// Dismisses the download progress popup, if one is showing.
pub fn dismiss_download_progress(hwnd: Option<HWND>) {
    if let Some(h) = hwnd {
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
}

/// Validates that a download URL points to an expected GitHub host.
fn validate_download_url(url: &str) -> Result<(), String> {
    if ALLOWED_URL_PREFIXES.iter().any(|prefix| url.starts_with(prefix)) {
        Ok(())
    } else {
        Err(format!(
            "Download URL has unexpected host (expected github.com): {url}"
        ))
    }
}

/// Verifies the SHA-256 hash of a file against an expected hex digest.
fn verify_sha256(path: &Path, expected: &str) -> Result<(), String> {
    use sha2::Digest;

    let mut file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open file for hash: {e}"))?;
    let mut hasher = sha2::Sha256::new();
    std::io::copy(&mut file, &mut hasher).map_err(|e| format!("Failed to read file for hash: {e}"))?;
    let actual = format!("{:x}", hasher.finalize());

    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "SHA-256 mismatch: expected {expected}, got {actual}"
        ))
    }
}

/// Downloads the new exe and replaces the running one via rename swap.
pub fn download_and_replace(
    download_url: &str,
    expected_sha256: Option<&str>,
) -> Result<PathBuf, String> {
    validate_download_url(download_url)?;

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

    // Verify SHA-256 hash if one was published
    if let Some(expected) = expected_sha256
        && let Err(e) = verify_sha256(&new_path, expected)
    {
        let _ = std::fs::remove_file(&new_path);
        return Err(e);
    }

    // Rename swap: current → .old, then .new → current
    match do_rename_swap(&current_exe, &new_path, &old_path) {
        Ok(()) => Ok(current_exe),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Need elevation (e.g. Program Files) — re-run with --replace-exe via runas
            match elevate_replace(&current_exe, &new_path, &old_path) {
                Ok(()) => Ok(current_exe),
                Err(err) => {
                    let _ = std::fs::remove_file(&new_path);
                    Err(err)
                }
            }
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

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(exe_w.as_ptr()),
        lpParameters: PCWSTR(params_w.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    unsafe {
        ShellExecuteExW(&mut sei).map_err(|_| "UAC elevation was cancelled or failed".to_string())?;
    }

    let handle = sei.hProcess;
    if handle.is_invalid() {
        return Err("Failed to get elevated process handle".to_string());
    }

    // Wait up to 30 seconds for the elevated process to finish
    let wait_result = unsafe { WaitForSingleObject(handle, 30_000) };
    unsafe {
        let _ = CloseHandle(handle);
    }

    if wait_result == WAIT_OBJECT_0 {
        // Verify the swap actually happened
        if !new_path.exists() && old_path.exists() {
            Ok(())
        } else {
            Err("Elevated process completed but rename swap did not succeed".to_string())
        }
    } else {
        Err("Elevated rename timed out".to_string())
    }
}

/// Handles the `--replace-exe <new> <target> <old>` CLI mode (runs elevated).
pub fn handle_replace_exe(args: &[String]) -> Result<(), String> {
    if args.len() < 3 {
        return Err("Usage: --replace-exe <new_path> <target_path> <old_path>".to_string());
    }
    let new_path = Path::new(&args[0]);
    let target_path = Path::new(&args[1]);
    let old_path = Path::new(&args[2]);

    // Validate all paths resolve within the current exe's directory
    validate_replace_paths(new_path, target_path, old_path)?;

    do_rename_swap(target_path, new_path, old_path)
        .map_err(|e| format!("Elevated rename failed: {e}"))
}

/// Validates that all `--replace-exe` paths are within the current exe's directory.
fn validate_replace_paths(new_path: &Path, target_path: &Path, old_path: &Path) -> Result<(), String> {
    let current_exe =
        std::env::current_exe().map_err(|e| format!("Failed to get current exe: {e}"))?;
    let exe_dir = current_exe
        .parent()
        .ok_or("Failed to get exe directory")?;
    let exe_dir_canon = exe_dir
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize exe dir: {e}"))?;

    for (label, path) in [("new", new_path), ("target", target_path)] {
        let canon = path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize {label} path: {e}"))?;
        if !canon.starts_with(&exe_dir_canon) {
            return Err(format!(
                "Rejected {label} path outside exe directory: {}",
                path.display()
            ));
        }
    }

    // old_path may not exist yet — canonicalize its parent directory instead
    let old_parent = old_path
        .parent()
        .ok_or("old_path has no parent directory")?;
    let old_parent_canon = old_parent
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize old path parent: {e}"))?;
    if !old_parent_canon.starts_with(&exe_dir_canon) {
        return Err(format!(
            "Rejected old path outside exe directory: {}",
            old_path.display()
        ));
    }

    Ok(())
}

/// Spawns the updated exe and exits the current process.
pub fn relaunch_and_exit(exe_path: &Path) -> ! {
    let _ = std::process::Command::new(exe_path).spawn();
    std::process::exit(0)
}
