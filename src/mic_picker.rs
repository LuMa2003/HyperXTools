//! Modal Win32 dialog for selecting the user's main microphone.

use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::audio::{self, AudioDevice};

const DLG_WIDTH: i32 = 360;
const DLG_HEIGHT: i32 = 300;
const ID_LISTBOX: i32 = 100;
const ID_OK: i32 = 101;
const ID_CANCEL: i32 = 102;
const ID_LABEL: i32 = 103;

/// State stored in GWLP_USERDATA during the dialog's lifetime.
struct PickerState {
    devices: Vec<AudioDevice>,
    selected: Option<AudioDevice>,
}

/// Shows a modal mic picker dialog. Returns the selected device, or `None` if cancelled.
pub fn show_mic_picker() -> Option<AudioDevice> {
    debug_log!("[mic_picker] show_mic_picker() called");
    audio::init_com();
    debug_log!("[mic_picker] COM initialized");

    let devices = audio::enumerate_input_devices();
    debug_log!("[mic_picker] got {} device(s) from enumerate_input_devices", devices.len());
    for (i, d) in devices.iter().enumerate() {
        debug_log!("[mic_picker]   [{}] name={:?} id={:?}", i, d.name, d.id);
    }

    if devices.is_empty() {
        unsafe {
            MessageBoxW(
                None,
                w!("No microphones found.\nPlease connect a microphone and try again."),
                w!("HyperXTools — Mic Picker"),
                MB_OK | MB_ICONERROR,
            );
        }
        return None;
    }

    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class_name = w!("HyperXToolsMicPicker");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(picker_wndproc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as *mut _),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);

        // Center on screen
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - DLG_WIDTH) / 2;
        let y = (screen_h - DLG_HEIGHT) / 2;

        debug_log!("[mic_picker] creating dialog window at ({}, {})", x, y);
        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            class_name,
            w!("HyperXTools — Select Main Microphone"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            x,
            y,
            DLG_WIDTH,
            DLG_HEIGHT,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .ok()?;
        debug_log!("[mic_picker] dialog window created OK, hwnd={:?}", hwnd);

        let font = GetStockObject(DEFAULT_GUI_FONT);

        // Label
        let label = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("Select your main microphone:"),
            WS_CHILD | WS_VISIBLE,
            12,
            10,
            320,
            20,
            Some(hwnd),
            Some(HMENU(ID_LABEL as *mut _)),
            Some(instance.into()),
            None,
        );
        if let Ok(label) = label {
            SendMessageW(label, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }

        // Listbox
        let listbox = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("LISTBOX"),
            None,
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | WINDOW_STYLE(LBS_NOTIFY as u32),
            12,
            34,
            320,
            180,
            Some(hwnd),
            Some(HMENU(ID_LISTBOX as *mut _)),
            Some(instance.into()),
            None,
        );
        if let Ok(listbox) = listbox {
            debug_log!("[mic_picker] listbox created OK");
            SendMessageW(listbox, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));

            // Populate listbox
            for (i, device) in devices.iter().enumerate() {
                let wide: Vec<u16> = device.name.encode_utf16().chain(std::iter::once(0)).collect();
                let result = SendMessageW(
                    listbox,
                    LB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(wide.as_ptr() as isize)),
                );
                debug_log!("[mic_picker] LB_ADDSTRING [{}] {:?} -> result={}", i, device.name, result.0);
            }
        } else {
            debug_log!("[mic_picker] ERROR: failed to create listbox");
        }

        // OK button (disabled until selection)
        let ok_btn = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("OK"),
            WS_CHILD | WS_VISIBLE | WS_DISABLED | WINDOW_STYLE(BS_DEFPUSHBUTTON as u32),
            140,
            225,
            90,
            30,
            Some(hwnd),
            Some(HMENU(ID_OK as *mut _)),
            Some(instance.into()),
            None,
        );
        if let Ok(ok_btn) = ok_btn {
            SendMessageW(ok_btn, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
        }

        // Cancel button
        let cancel_btn = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Cancel"),
            WS_CHILD | WS_VISIBLE,
            240,
            225,
            90,
            30,
            Some(hwnd),
            Some(HMENU(ID_CANCEL as *mut _)),
            Some(instance.into()),
            None,
        );
        if let Ok(cancel_btn) = cancel_btn {
            SendMessageW(
                cancel_btn,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }

        // Store state
        let state = Box::new(PickerState {
            devices,
            selected: None,
        });
        let state_ptr = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        // Modal message loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Reclaim state
        let state = Box::from_raw(state_ptr);
        let _ = UnregisterClassW(class_name, Some(instance.into()));

        debug_log!("[mic_picker] dialog closed, selected={:?}", state.selected);
        state.selected
    }
}

unsafe extern "system" fn picker_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut PickerState;

    match msg {
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let notify = ((wparam.0 >> 16) & 0xFFFF) as u32;

            match id {
                ID_LISTBOX if notify == LBN_SELCHANGE => {
                    // Enable OK button when something is selected
                    if let Ok(ok_btn) = unsafe { GetDlgItem(Some(hwnd), ID_OK) } {
                        unsafe {
                            let _ = EnableWindow(ok_btn, true);
                        }
                    }
                    LRESULT(0)
                }
                ID_LISTBOX if notify == LBN_DBLCLK => {
                    // Double-click acts like OK
                    if !ptr.is_null() {
                        unsafe { accept_selection(hwnd, ptr) };
                    }
                    LRESULT(0)
                }
                ID_OK => {
                    if !ptr.is_null() {
                        unsafe { accept_selection(hwnd, ptr) };
                    }
                    LRESULT(0)
                }
                ID_CANCEL => {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    LRESULT(0)
                }
                _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
            }
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe fn accept_selection(hwnd: HWND, state_ptr: *mut PickerState) {
    unsafe {
        if let Ok(listbox) = GetDlgItem(Some(hwnd), ID_LISTBOX) {
            let index = SendMessageW(listbox, LB_GETCURSEL, None, None);
            debug_log!("[mic_picker] accept_selection: LB_GETCURSEL index={}", index.0);
            if index.0 >= 0 {
                let state = &mut *state_ptr;
                if let Some(device) = state.devices.get(index.0 as usize) {
                    debug_log!("[mic_picker] accepted: name={:?} id={:?}", device.name, device.id);
                    state.selected = Some(device.clone());
                } else {
                    debug_log!("[mic_picker] ERROR: index {} out of bounds (len={})", index.0, state.devices.len());
                }
            } else {
                debug_log!("[mic_picker] no selection (index < 0)");
            }
        } else {
            debug_log!("[mic_picker] ERROR: GetDlgItem for listbox failed");
        }
        let _ = DestroyWindow(hwnd);
    }
}
