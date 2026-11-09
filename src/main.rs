// OffWorkCountdown: a frameless topmost window whose background is a Direct3D 11
// shader and whose card is drawn with Direct2D.

#![windows_subsystem = "windows"]

mod dock;
mod gfx;
mod texture;

use std::time::Instant;

use windows::core::{w, Result};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, ScreenToClient, PAINTSTRUCT};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetSystemMetrics, LoadCursorW,
    GetCursorPos, GetWindowLongPtrW, GetWindowRect, MSG, PeekMessageW, PostQuitMessage, RegisterClassW,
    SPI_GETWORKAREA, SPI_SETWORKAREA, SendMessageW, SetWindowLongPtrW, SetWindowPos, SystemParametersInfoW,
    TranslateMessage, GWLP_USERDATA, HTCAPTION, HTCLIENT, HTTRANSPARENT, IDC_ARROW,
    PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_DESTROY, WM_DISPLAYCHANGE, WM_ENTERSIZEMOVE,
    WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOVING,
    WM_NCLBUTTONDOWN, WM_NCHITTEST, WM_PAINT, WM_QUIT, WM_SETTINGCHANGE, WNDCLASSW,
    WS_EX_NOREDIRECTIONBITMAP,
    WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

/// The work day this build shows, in seconds since midnight.
const START: i64 = 9 * 3600;
const END: i64 = 18 * 3600;

fn main() -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let class = w!("OffWorkCountdown");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&window_class);

        let scale = GetDpiForSystem() as f32 / 96.0;
        let pad = (gfx::SHADOW_PAD_DIP * scale).round() as i32;
        let card_w = (gfx::CARD_WIDTH_DIP * scale).round() as i32;
        let card_h = (gfx::CARD_HEIGHT_DIP * scale).round() as i32;
        let width = card_w + pad * 2;
        let height = card_h + pad * 2;
        // Sit inside the work area, with a few pixels spare so the shadow is
        // not cut by the screen edge or hidden under the taskbar.
        let mut work = RECT::default();
        let work_ok = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut RECT as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok();
        let (area_right, area_bottom) = if work_ok && work.right > work.left {
            (work.right, work.bottom)
        } else {
            (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
        };
        let edge = (10.0 * scale).round() as i32;

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOREDIRECTIONBITMAP,
            class,
            w!("下班倒计时"),
            WS_POPUP | WS_VISIBLE,
            area_right - width - edge,
            area_bottom - height - edge,
            width,
            height,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )?;

        let (cw, ch) = gfx::client_size(hwnd);
        let dpi = windows::Win32::UI::HiDpi::GetDpiForWindow(hwnd) as f32;
        let mut renderer = match gfx::Gfx::new(hwnd, cw, ch, if dpi > 0.0 { dpi } else { 96.0 }) {
            Ok(renderer) => renderer,
            Err(error) => {
                log(&format!("Gfx::new failed: {error}"));
                return Err(error);
            }
        };
        log(&format!("window {cw}x{ch} dpi {dpi}"));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut renderer as *mut gfx::Gfx as isize);

        let mut last = Instant::now();
        let mut message = MSG::default();
        loop {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    return Ok(());
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f32().min(0.1);
            last = now;
            renderer.prepare(dt);
            if renderer.take_layout_dirty() {
                settle_layout(hwnd, &mut renderer);
            }
            let work = renderer.dock_monitor().and_then(dock::monitor_info).map(|info| info.rcWork);
            let hover = work.is_some_and(|area| cursor_on_card(hwnd, &renderer, area));
            if let Some(origin) = renderer.sync_dock(dt, hover, work) {
                if let Some(area) = work {
                    place_dock(hwnd, &mut renderer, origin, area);
                }
            } else if !renderer.is_docked() {
                renderer.set_dock_clip(None);
            }
            if let Err(error) = renderer.render(&frame(), dt) {
                log(&format!("render failed: {error}"));
                return Ok(());
            }
            // The fold waits on vsync inside Present. Idle stays near 30 fps:
            // the ocean is the only thing moving, and it does not need more.
            if !renderer.is_animating() {
                std::thread::sleep(std::time::Duration::from_millis(24));
            }
        }
    }
}

/// The card's content at this moment.
fn frame() -> gfx::Frame {
    let now = local_seconds();
    let done = now >= END;
    let remaining = if done { 0 } else { END - now };
    let progress = if done {
        1.0
    } else {
        ((now - START) as f32 / (END - START) as f32).clamp(0.0, 1.0)
    };
    gfx::Frame {
        time_text: format!(
            "{:02}:{:02}:{:02}",
            remaining / 3600,
            (remaining % 3600) / 60,
            remaining % 60
        ),
        label: if done { "下班啦".to_string() } else { "距离下班".to_string() },
        footer_left: "下班时间 18:00".to_string(),
        footer_right: format!("{:.0}%", progress * 100.0),
        progress,
        background: 1.0,
        paused: false,
    }
}

/// Appends a line to `%TEMP%\offwork-countdown.log`, so a failure leaves a trace.
pub fn log(message: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("offwork-countdown.log");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

/// Seconds since midnight, local time.
fn local_seconds() -> i64 {
    let time = unsafe { GetLocalTime() };
    time.wHour as i64 * 3600 + time.wMinute as i64 * 60 + time.wSecond as i64
}

fn gfx_from(hwnd: HWND) -> Option<&'static mut gfx::Gfx> {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut gfx::Gfx;
        if ptr.is_null() { None } else { Some(&mut *ptr) }
    }
}

fn client_point(lparam: LPARAM) -> (i32, i32) {
    let packed = lparam.0 as u32;
    (packed as i16 as i32, (packed >> 16) as i16 as i32)
}

fn card_rect(hwnd: HWND, gfx: &gfx::Gfx) -> RECT {
    let mut window = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut window) };
    let (pad, width, height) = gfx.card_px();
    RECT {
        left: window.left + pad,
        top: window.top + pad,
        right: window.left + pad + width,
        bottom: window.top + pad + height,
    }
}

fn cursor_on_card(hwnd: HWND, gfx: &gfx::Gfx, work: RECT) -> bool {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_err() {
        return false;
    }
    if point.x < work.left || point.y < work.top || point.x >= work.right || point.y >= work.bottom {
        return false;
    }
    let card = card_rect(hwnd, gfx);
    point.x >= card.left && point.y >= card.top && point.x < card.right && point.y < card.bottom
}

fn place_dock(hwnd: HWND, gfx: &mut gfx::Gfx, origin: (i32, i32), work: RECT) {
    let (pad, _, _) = gfx.card_px();
    let x = origin.0 - pad;
    let y = origin.1 - pad;
    let mut window = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut window);
        if window.left != x || window.top != y {
            let _ = SetWindowPos(
                hwnd,
                None,
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            let _ = GetWindowRect(hwnd, &mut window);
        }
    }
    let width = window.right - window.left;
    let height = window.bottom - window.top;
    let left = (work.left - window.left).max(0);
    let top = (work.top - window.top).max(0);
    let right = (work.right - window.left).min(width);
    let bottom = (work.bottom - window.top).min(height);
    if right > left && bottom > top {
        gfx.set_dock_clip(Some((left as f32, top as f32, right as f32, bottom as f32)));
    }
}

fn finish_move(hwnd: HWND) {
    let Some(gfx) = gfx_from(hwnd) else { return };
    let mut cursor = POINT::default();
    let _ = unsafe { GetCursorPos(&mut cursor) };
    let card = card_rect(hwnd, gfx);
    let center = POINT {
        x: (card.left + card.right) / 2,
        y: (card.top + card.bottom) / 2,
    };
    let monitor = dock::monitor_from_point(center);
    let Some(info) = dock::monitor_info(monitor) else { return };
    gfx.end_window_drag(cursor, card, info.rcWork, monitor, dock::outer_edges(monitor));
}

fn settle_layout(hwnd: HWND, gfx: &mut gfx::Gfx) {
    let card = card_rect(hwnd, gfx);
    let center = POINT {
        x: (card.left + card.right) / 2,
        y: (card.top + card.bottom) / 2,
    };
    let monitor = dock::monitor_from_point(center);
    let Some(info) = dock::monitor_info(monitor) else { return };
    let outer = dock::outer_edges(monitor);
    if gfx.is_docked() {
        let stored = gfx.dock_monitor();
        let still_there = stored.is_some_and(|monitor| dock::monitor_info(monitor).is_some())
            && gfx.dock_edge_outer(&dock::outer_edges(stored.unwrap_or_default()));
        if still_there {
            return;
        }
        gfx.clear_dock();
        gfx.rescue(card, info.rcWork, monitor, outer);
        return;
    }
    if !dock::intersects(card, info.rcWork) {
        gfx.rescue(card, info.rcWork, monitor, outer);
    }
}

fn client_from_screen(hwnd: HWND, lparam: LPARAM) -> (i32, i32) {
    let (x, y) = client_point(lparam);
    let mut point = POINT { x, y };
    let _ = unsafe { ScreenToClient(hwnd, &mut point) };
    (point.x, point.y)
}

#[link(name = "user32")]
extern "system" {
    fn SetCapture(hwnd: HWND) -> HWND;
    fn ReleaseCapture() -> i32;
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        // The card drags. The shadow is not part of the card, so a click there
        // goes through to whatever is underneath.
        WM_NCHITTEST => {
            let (sx, sy) = client_point(lparam);
            if let Some(work) = gfx_from(hwnd).and_then(|gfx| gfx.dock_work()) {
                if sx < work.left || sy < work.top || sx >= work.right || sy >= work.bottom {
                    return LRESULT(HTTRANSPARENT as isize);
                }
            }
            let (x, y) = client_from_screen(hwnd, lparam);
            LRESULT(match gfx_from(hwnd).map(|gfx| gfx.hit(x, y)) {
                Some(gfx::Hit::Miss) | None => HTTRANSPARENT as isize,
                Some(gfx::Hit::Drag) => HTCAPTION as isize,
                Some(gfx::Hit::Chrome(_) | gfx::Hit::Pill | gfx::Hit::Peek) => HTCLIENT as isize,
            })
        }
        WM_ENTERSIZEMOVE => {
            if let Some(gfx) = gfx_from(hwnd) {
                let mut cursor = POINT::default();
                let _ = GetCursorPos(&mut cursor);
                gfx.begin_window_drag(cursor);
            }
            LRESULT(0)
        }
        // A click that does not move never arrives here, so the tucked clip
        // stays put. An actual drag reveals the whole card under the pointer.
        WM_MOVING => {
            if let Some(gfx) = gfx_from(hwnd) {
                gfx.set_dock_clip(None);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_EXITSIZEMOVE => {
            finish_move(hwnd);
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            if let Some(gfx) = gfx_from(hwnd) {
                gfx.mark_layout_dirty();
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_SETTINGCHANGE => {
            if wparam.0 == SPI_SETWORKAREA.0 as usize {
                if let Some(gfx) = gfx_from(hwnd) {
                    gfx.mark_layout_dirty();
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = client_point(lparam);
            if let Some(gfx) = gfx_from(hwnd) {
                if gfx.mouse_down(x, y) {
                    SetCapture(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let (x, y) = client_point(lparam);
            if let Some(gfx) = gfx_from(hwnd) {
                if gfx.mouse_dragged(x, y) {
                    ReleaseCapture();
                    let mut screen = POINT::default();
                    let _ = GetCursorPos(&mut screen);
                    let packed = LPARAM(((screen.x as u16 as u32) | ((screen.y as u16 as u32) << 16)) as isize);
                    let _ = SendMessageW(hwnd, WM_NCLBUTTONDOWN, Some(WPARAM(HTCAPTION as usize)), Some(packed));
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = client_point(lparam);
            if let Some(gfx) = gfx_from(hwnd) {
                gfx.mouse_up(x, y);
            }
            ReleaseCapture();
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut paint = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut paint);
            let _ = EndPaint(hwnd, &paint);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
