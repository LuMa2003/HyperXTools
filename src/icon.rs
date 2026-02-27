use std::mem;
use windows::core::Result;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const ICON_SIZE: i32 = 16;

/// Returns fill color as (R, G, B) based on battery percentage.
fn battery_color(percent: u8) -> (u8, u8, u8) {
    match percent {
        50..=255 => (0, 180, 0),  // green
        20..=49 => (240, 180, 0), // amber
        _ => (220, 30, 0),        // red
    }
}

/// Returns true if (x, y) is part of the battery outline (body border + terminal nub).
fn is_outline(x: i32, y: i32) -> bool {
    // Body rectangle border: x=1..12, y=3..11 (1px thick)
    let in_body = (1..=12).contains(&x) && (3..=11).contains(&y);
    let in_interior = (2..=11).contains(&x) && (4..=10).contains(&y);
    let body_border = in_body && !in_interior;

    // Terminal nub outline: top/bottom at y=5,9 for x=13..14; right edge x=14, y=6..8
    let nub = ((y == 5 || y == 9) && (13..=14).contains(&x)) || (x == 14 && (6..=8).contains(&y));

    body_border || nub
}

/// Sets a BGRA pixel in the 16x16 buffer.
#[inline]
unsafe fn set_pixel(buf: *mut u8, x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    unsafe {
        let off = ((y * ICON_SIZE + x) * 4) as isize;
        *buf.offset(off) = b;
        *buf.offset(off + 1) = g;
        *buf.offset(off + 2) = r;
        *buf.offset(off + 3) = a;
    }
}

/// Renders a 16x16 battery-shaped tray icon with fill proportional to charge.
pub fn render_battery_icon(percent: u8) -> Result<HICON> {
    let fill_cols = (percent as i32 * 10 / 100).max(1);
    render_icon((255, 255, 255), Some(battery_color(percent)), fill_cols)
}

/// Renders a gray battery outline with no fill (disconnected state).
pub fn render_disconnected_icon() -> Result<HICON> {
    render_icon((128, 128, 128), None, 0)
}

fn render_icon(
    outline: (u8, u8, u8),
    fill: Option<(u8, u8, u8)>,
    fill_cols: i32,
) -> Result<HICON> {
    unsafe {
        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));

        // 32bpp top-down DIB section
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: ICON_SIZE,
                biHeight: -ICON_SIZE, // negative = top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let color_bmp =
            CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;

        let buf = bits as *mut u8;

        // Clear entire buffer to transparent (BGRA 0,0,0,0)
        std::ptr::write_bytes(buf, 0, (ICON_SIZE * ICON_SIZE * 4) as usize);

        let (or, og, ob) = outline;

        // Draw battery shape pixel by pixel
        for y in 0..ICON_SIZE {
            for x in 0..ICON_SIZE {
                if is_outline(x, y) {
                    set_pixel(buf, x, y, or, og, ob, 255);
                } else if let Some((fr, fg, fb)) = fill {
                    // Fill area: x=2..11, y=4..10, left-aligned
                    if x >= 2 && x < 2 + fill_cols && (4..=10).contains(&y) {
                        set_pixel(buf, x, y, fr, fg, fb, 255);
                    }
                }
            }
        }

        // Monochrome mask — all zeros lets the 32bpp alpha channel control transparency
        let mask_data = [0u8; 32]; // 16 rows x 2 bytes/row (1bpp, WORD-aligned)
        let mask_bmp =
            CreateBitmap(ICON_SIZE, ICON_SIZE, 1, 1, Some(mask_data.as_ptr().cast()));

        let icon_info = ICONINFO {
            fIcon: TRUE,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask_bmp,
            hbmColor: color_bmp,
        };
        let icon = CreateIconIndirect(&icon_info)?;

        // Cleanup GDI objects
        let _ = DeleteObject(color_bmp.into());
        let _ = DeleteObject(mask_bmp.into());
        let _ = DeleteDC(mem_dc);
        ReleaseDC(None, screen_dc);

        Ok(icon)
    }
}
