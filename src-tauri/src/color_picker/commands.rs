//! Tauri command adapters for the color-picker domain.
//!
//! The screen-capture helpers are pure Win32 GDI: `BitBlt` from each monitor
//! DC into a compatible memory DC, then `GetDIBits` to extract raw BGRA. We
//! cache captures in a `OnceLock<Mutex<Option<...>>>` for the lifetime of an
//! eyedropper session — freed by `end_eyedropper`.
//!
//! `read_live_pixel` does a fresh 1×1 BitBlt for maximum accuracy at pick
//! time, bypassing the cache entirely.

use std::sync::{Mutex, OnceLock};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use windows::core::w;
use windows::Win32::Foundation::{LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CAPTUREBLT, CreateCompatibleBitmap, CreateCompatibleDC, CreateDCW, DeleteDC,
    DeleteObject, EnumDisplayMonitors, GetDIBits, GetObjectW, SelectObject, SRCCOPY, BITMAP,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, HDC, HMONITOR,
};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

use crate::window;

const COLOR_PICKER_LABEL: &str = "color-picker";
const EYEDROPPER_LABEL: &str = "eyedropper";

/// One monitor's captured pixels (BGRA, bottom-up DIB rows).
struct MonitorCapture {
    /// Virtual-screen origin (OS pixels; can be negative on multi-mon).
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// `w * h * 4` bytes, BGRA order. The desktop is opaque so alpha reads
    /// back as 255.
    pixels: Vec<u8>,
}

static CAPTURES: OnceLock<Mutex<Option<Vec<MonitorCapture>>>> = OnceLock::new();

fn captures() -> &'static Mutex<Option<Vec<MonitorCapture>>> {
    CAPTURES.get_or_init(|| Mutex::new(None))
}

/// Encoded PNG frames for the active eyedropper session. The overlay renders
/// these as opaque images (one per monitor) instead of relying on a
/// transparent window — a transparent fullscreen window over hardware-
/// accelerated video (MPO) makes the video plane render as black. Showing a
/// frozen screenshot sidesteps that entirely.
static FRAMES: OnceLock<Mutex<Option<Vec<MonitorBoundsDto>>>> = OnceLock::new();

fn frames() -> &'static Mutex<Option<Vec<MonitorBoundsDto>>> {
    FRAMES.get_or_init(|| Mutex::new(None))
}

/// Description of one captured monitor, sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorBoundsDto {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// PNG data URL of the captured frame. Empty only if the capture failed.
    pub png_data_url: String,
}

/// Capture every monitor into the static cache, then return the bounds so
/// the frontend overlay can render the screenshot at the correct virtual
/// coordinates. The cache stays alive until `end_eyedropper` runs.
#[tauri::command]
pub fn start_eyedropper() -> Result<Vec<MonitorBoundsDto>, String> {
    let caps = capture_all_monitors()?;
    let dtos = encode_frames_parallel(&caps)?;
    if let Ok(mut g) = captures().lock() {
        *g = Some(caps);
    }
    Ok(dtos)
}

/// Read the cached pixel at virtual-screen `(x, y)`. Returns `[r, g, b, a]`.
#[tauri::command]
pub fn eyedropper_pixel(x: i32, y: i32) -> Result<[u8; 4], String> {
    let guard = captures().lock().map_err(|e| e.to_string())?;
    let caps = guard.as_ref().ok_or("no active eyedropper capture")?;
    for c in caps {
        let lx = x - c.x;
        let ly = y - c.y;
        if lx < 0 || ly < 0 || lx >= c.w || ly >= c.h {
            continue;
        }
        let stride = c.w as usize * 4;
        // The DIB rows are bottom-up (`biHeight` positive), so flip Y.
        let row = (c.h as usize - 1) - ly as usize;
        let idx = row * stride + lx as usize * 4;
        let b = c.pixels[idx];
        let g = c.pixels[idx + 1];
        let r = c.pixels[idx + 2];
        let a = c.pixels.get(idx + 3).copied().unwrap_or(255);
        return Ok([r, g, b, a]);
    }
    Err(format!("point ({x}, {y}) outside every captured monitor"))
}

/// Return the frozen PNG frames captured for the active eyedropper session.
/// The overlay renders these as opaque per-monitor images so no live
/// (possibly hardware-composited) content shows through the window.
#[tauri::command]
pub fn get_eyedropper_frames() -> Result<Vec<MonitorBoundsDto>, String> {
    let guard = frames().lock().map_err(|e| e.to_string())?;
    Ok(guard.clone().unwrap_or_default())
}

/// Drop the cached captures. Safe to call when no session is active.
#[tauri::command]
pub fn end_eyedropper() -> Result<(), String> {
    if let Ok(mut g) = captures().lock() {
        *g = None;
    }
    if let Ok(mut g) = frames().lock() {
        *g = None;
    }
    Ok(())
}

/// Open the fullscreen eyedropper overlay. Captures every monitor (caching
/// the pixels for the session and encoding a frozen PNG per monitor),
/// computes the virtual-screen bounding box, and creates an always-on-top
/// window that spans all monitors. The frontend renders the frozen frames as
/// opaque images (so hardware-composited video doesn't black out), plus the
/// crosshair + live color preview, and samples via `eyedropper_pixel`.
#[tauri::command]
pub async fn open_eyedropper(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(EYEDROPPER_LABEL) {
        let _ = win.close();
        return Ok(());
    }

    // Capture now so the window can read the virtual bounding box before the
    // overlay appears (and so the first hover sample is instant). Keep the
    // captures cached for `eyedropper_pixel` too (no second capture needed).
    let bounds = capture_all_monitors()?;
    let vx0 = bounds.iter().map(|c| c.x).min().unwrap_or(0);
    let vy0 = bounds.iter().map(|c| c.y).min().unwrap_or(0);
    let vx1 = bounds.iter().map(|c| c.x + c.w).max().unwrap_or(0);
    let vy1 = bounds.iter().map(|c| c.y + c.h).max().unwrap_or(0);
    let vw = (vx1 - vx0).max(1);
    let vh = (vy1 - vy0).max(1);

    // Encode PNG frames (one per monitor) *before* the overlay is shown, so
    // the capture never includes the overlay window. The frontend renders
    // these opaque frames — this is what avoids the transparent-over-video
    // (MPO) black-out bug. Encoded in parallel with fast compression to keep
    // the open latency low.
    let dtos = encode_frames_parallel(&bounds)?;
    if let Ok(mut g) = frames().lock() {
        *g = Some(dtos);
    }
    if let Ok(mut g) = captures().lock() {
        *g = Some(bounds);
    }

    tauri::async_runtime::spawn_blocking(move || {
        create_eyedropper_window(&app, vx0, vy0, vw, vh)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn create_eyedropper_window(
    app: &tauri::AppHandle,
    vx0: i32,
    vy0: i32,
    vw: i32,
    vh: i32,
) -> Result<(), String> {
    let win = tauri::WebviewWindowBuilder::new(
        app,
        EYEDROPPER_LABEL,
        tauri::WebviewUrl::App("widgets/color_picker/window/eyedropper.html".into()),
    )
    .title("Pick Color")
    .inner_size(vw as f64, vh as f64)
    .position(vx0 as f64, vy0 as f64)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(true)
    .always_on_top(true)
    .additional_browser_args("--default-background-color=00000000")
    .build()
    .map_err(|e| e.to_string())?;

    // No acrylic/rounded on the eyedropper overlay — it must stay fully
    // transparent so the live desktop shows through untouched (only the
    // custom dropper cursor + color preview float above it).
    let _ = window::set_disable_transitions(&win);

    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    };
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE,
        )
    };
    // No sleep needed — the overlay has no acrylic to settle.
    let _ = win.set_focus();

    Ok(())
}

/// Open the right-click color-picker window anchored under the widget.
#[tauri::command]
pub async fn open_color_picker(
    app: tauri::AppHandle,
    x: f64,
    y: f64,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(COLOR_PICKER_LABEL) {
        let _ = win.close();
        return Ok(());
    }

    tauri::async_runtime::spawn_blocking(move || create_color_picker_window(&app, x, y))
        .await
        .map_err(|e| e.to_string())?
}

/// Helper to read the current OS cursor position in virtual-screen pixels.
/// Exposed as a Tauri command so the picker window can poll the live cursor
/// during its eyedropper mode and look up the pixel from the cached canvas.
#[tauri::command]
pub fn get_cursor_position() -> Result<(i32, i32), String> {
    unsafe {
        let mut pt = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut pt).is_err() {
            return Err("GetCursorPos failed".into());
        }
        Ok((pt.x, pt.y))
    }
}

/// Read a single pixel at the given virtual-screen coordinate via a fresh
/// 1×1 BitBlt. This is slower than the cached `eyedropper_pixel` but
/// guaranteed to return the live desktop pixel at the moment of the call.
/// Used by the eyedropper overlay's `pick()` to ensure perfect accuracy.
#[tauri::command]
pub fn read_live_pixel(x: i32, y: i32) -> Result<[u8; 4], String> {
    unsafe {
        let screen_dc = CreateDCW(
            w!("DISPLAY"),
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            None,
        );
        if screen_dc.is_invalid() {
            return Err("CreateDCW DISPLAY failed".into());
        }

        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = DeleteDC(screen_dc);
            return Err("CreateCompatibleDC failed".into());
        }

        let bmp = CreateCompatibleBitmap(screen_dc, 1, 1);
        if bmp.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = DeleteDC(screen_dc);
            return Err("CreateCompatibleBitmap failed".into());
        }

        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));

        let blit = BitBlt(mem_dc, 0, 0, 1, 1, Some(screen_dc), x, y, SRCCOPY | CAPTUREBLT);
        if blit.is_err() {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);
            let _ = DeleteDC(screen_dc);
            return Err("BitBlt failed".into());
        }

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 1,
                biHeight: 1,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut px = [0u8; 4];
        let rows = GetDIBits(
            mem_dc,
            bmp,
            0,
            1,
            Some(px.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );
        if rows == 0 {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);
            let _ = DeleteDC(screen_dc);
            return Err("GetDIBits failed".into());
        }

        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = DeleteDC(screen_dc);

        // DIB returns BGRA; reorder to RGBA.
        Ok([px[2], px[1], px[0], px[3]])
    }
}

fn create_color_picker_window(app: &tauri::AppHandle, x: f64, y: f64) -> Result<(), String> {
    let win_w = 360.0_f64;
    let win_h = 510.0_f64;
    let (cx, cy, cw, ch) = window::monitor::clamp_to_monitor(
        x.round() as i32,
        y.round() as i32,
        win_w as i32,
        win_h as i32,
    );

    let win = tauri::WebviewWindowBuilder::new(
        app,
        COLOR_PICKER_LABEL,
        tauri::WebviewUrl::App("widgets/color_picker/window/color-picker.html".into()),
    )
    .title("Color Picker")
    .inner_size(cw as f64, ch as f64)
    .min_inner_size(320.0, 520.0)
    .max_inner_size(480.0, 720.0)
    .position(cx as f64, cy as f64)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(true)
    .always_on_top(true)
    .additional_browser_args("--default-background-color=00000000")
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window::apply_fixed_acrylic(app, COLOR_PICKER_LABEL);
    let _ = window::set_rounded_corners(&win);
    let _ = window::set_disable_transitions(&win);

    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW,
    };
    let hwnd = win.hwnd().map_err(|e| e.to_string())?;
    let _ = unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_SHOWWINDOW | SWP_NOZORDER | SWP_NOSIZE | SWP_NOMOVE,
        )
    };
    // Acrylic was already applied via `apply_fixed_acrylic` before show.
    let _ = win.set_focus();

    Ok(())
}

// ─── Win32 capture plumbing ────────────────────────────────────────────────

fn capture_all_monitors() -> Result<Vec<MonitorCapture>, String> {
    let monitors = enumerate_monitors()?;
    let mut out: Vec<MonitorCapture> = Vec::with_capacity(monitors.len());
    for rc in monitors {
        match capture_rect(&rc) {
            Ok(c) => out.push(c),
            Err(_) => continue,
        }
    }
    if out.is_empty() {
        // Fallback: capture just the primary screen.
        let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let rc = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        out.push(capture_rect(&rc)?);
    }
    Ok(out)
}

/// Walk every monitor in the virtual desktop and return each one's full
/// screen rect (not the work area — we want every pixel).
fn enumerate_monitors() -> Result<Vec<RECT>, String> {
    unsafe extern "system" fn enum_proc(
        _hmon: HMONITOR,
        _hdc: HDC,
        rc: *mut RECT,
        data: LPARAM,
    ) -> windows::core::BOOL {
        let out = data.0 as *mut Vec<RECT>;
        if !rc.is_null() && !out.is_null() {
            (*out).push(*rc);
        }
        windows::core::BOOL(1)
    }

    let mut out: Vec<RECT> = Vec::new();
    unsafe {
        let ok = EnumDisplayMonitors(
            None,
            None,
            Some(enum_proc),
            LPARAM(&mut out as *mut Vec<RECT> as isize),
        );
        if !ok.as_bool() {
            return Err("EnumDisplayMonitors failed".into());
        }
    }
    if out.is_empty() {
        let w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        out.push(RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        });
    }
    Ok(out)
}

/// Capture one monitor's rect via `BitBlt` + `GetDIBits`. Returns BGRA,
/// bottom-up (the way GDI delivers it).
fn capture_rect(rc: &RECT) -> Result<MonitorCapture, String> {
    let w = (rc.right - rc.left).max(1);
    let h = (rc.bottom - rc.top).max(1);
    unsafe {
        let screen_dc = windows::Win32::Graphics::Gdi::CreateDCW(
            w!("DISPLAY"),
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            None,
        );
        if screen_dc.is_invalid() {
            return Err("CreateDCW DISPLAY failed".into());
        }

        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = DeleteDC(screen_dc);
            return Err("CreateCompatibleDC failed".into());
        }

        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        if bmp.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = DeleteDC(screen_dc);
            return Err("CreateCompatibleBitmap failed".into());
        }

        let old = SelectObject(mem_dc, HGDIOBJ(bmp.0));

        // BitBlt with CAPTUREBLT so layered windows are included — without
        // it the bar's acrylic blur would render as solid black.
        let blit = BitBlt(mem_dc, 0, 0, w, h, Some(screen_dc), rc.left, rc.top, SRCCOPY | CAPTUREBLT);
        if blit.is_err() {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);
            let _ = DeleteDC(screen_dc);
            return Err("BitBlt failed".into());
        }

        // Verify the bitmap actually has the requested dimensions before
        // pulling pixels — DPI-aware callers can otherwise trip over a
        // premultiplied size mismatch.
        let mut bmp_info = BITMAP::default();
        if GetObjectW(
            HGDIOBJ(bmp.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp_info as *mut BITMAP as *mut _),
        ) == 0
        {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);
            let _ = DeleteDC(screen_dc);
            return Err("GetObjectW failed".into());
        }

        let real_w = bmp_info.bmWidth.max(1);
        let real_h = bmp_info.bmHeight.max(1);

        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: real_w,
                biHeight: real_h, // positive = bottom-up
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut pixels = vec![0u8; (real_w as usize) * (real_h as usize) * 4];
        let rows = GetDIBits(
            mem_dc,
            bmp,
            0,
            real_h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bi,
            DIB_RGB_COLORS,
        );
        if rows == 0 {
            let _ = SelectObject(mem_dc, old);
            let _ = DeleteObject(HGDIOBJ(bmp.0));
            let _ = DeleteDC(mem_dc);
            let _ = DeleteDC(screen_dc);
            return Err("GetDIBits failed".into());
        }

        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem_dc);
        let _ = DeleteDC(screen_dc);

        Ok(MonitorCapture {
            x: rc.left,
            y: rc.top,
            w: real_w,
            h: real_h,
            pixels,
        })
    }
}

/// Encode a captured BGRA buffer as a `data:image/png;base64,...` URL.
///
/// Uses `CompressionType::Fast` + a fixed `Up` filter: this is the frozen
/// backdrop only (sampling reads raw cached pixels via `eyedropper_pixel`, not
/// the PNG), so we optimise for encode speed over file size. The `Up` filter
/// is cheap and compresses screenshots well thanks to vertical row coherence.
fn encode_png_data_url(c: &MonitorCapture) -> Result<MonitorBoundsDto, String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::{ExtendedColorType, ImageEncoder};

    let mut rgba: Vec<u8> = Vec::with_capacity((c.w as usize) * (c.h as usize) * 4);
    let stride = c.w as usize * 4;
    // The DIB is bottom-up; emit top-down so the PNG encoder reads it naturally.
    for row in (0..c.h as usize).rev() {
        let s = row * stride;
        let chunk = &c.pixels[s..s + stride];
        for px in chunk.chunks_exact(4) {
            rgba.push(px[2]); // R
            rgba.push(px[1]); // G
            rgba.push(px[0]); // B
            rgba.push(255); // A — desktop is opaque
        }
    }

    let mut png_bytes = Vec::new();
    PngEncoder::new_with_quality(&mut png_bytes, CompressionType::Fast, FilterType::Up)
        .write_image(&rgba, c.w as u32, c.h as u32, ExtendedColorType::Rgba8)
        .map_err(|e| format!("png encode: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok(MonitorBoundsDto {
        x: c.x,
        y: c.y,
        w: c.w,
        h: c.h,
        png_data_url: format!("data:image/png;base64,{}", b64),
    })
}

/// Encode every monitor's frame concurrently (one thread per monitor).
/// Screenshot PNG encoding is CPU-bound, so this scales the total encode time
/// down to roughly the single slowest monitor instead of their sum.
fn encode_frames_parallel(caps: &[MonitorCapture]) -> Result<Vec<MonitorBoundsDto>, String> {
    std::thread::scope(|s| {
        let handles: Vec<_> = caps
            .iter()
            .map(|c| s.spawn(move || encode_png_data_url(c)))
            .collect();
        let mut out = Vec::with_capacity(handles.len());
        for h in handles {
            out.push(h.join().map_err(|_| "encode thread panicked".to_string())??);
        }
        Ok(out)
    })
}
