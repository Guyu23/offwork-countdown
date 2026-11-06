// OffWorkCountdown, native shell: a frameless topmost window whose background
// is a Direct3D 11 shader and whose card is drawn with Direct2D.
//
// M0: one 300×150 window, the ocean shader behind it, the countdown on top.

#![windows_subsystem = "windows"]

mod gfx;
mod texture;

use std::time::Instant;

use windows::core::{w, Result};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{GetDpiForSystem, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetSystemMetrics, LoadCursorW,
    MSG, PeekMessageW, PostQuitMessage, RegisterClassW, TranslateMessage, HTCAPTION, IDC_ARROW,
    PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, WM_DESTROY, WM_ERASEBKGND, WM_NCHITTEST, WM_PAINT,
    WM_QUIT, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

/// The work day this build shows, in seconds since midnight.
const START: i64 = 9 * 3600;
const END: i64 = 18 * 3600;

fn main() -> Result<()> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let class = w!("OffWorkCountdownNative");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&window_class);

        let scale = GetDpiForSystem() as f32 / 96.0;
        let width = (300.0 * scale).round() as i32;
        let height = (150.0 * scale).round() as i32;
        let margin = (24.0 * scale).round() as i32;
        let (screen_w, screen_h) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!("下班倒计时"),
            WS_POPUP | WS_VISIBLE,
            screen_w - width - margin,
            screen_h - height - margin,
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
            if let Err(error) = renderer.render(&frame(), dt) {
                log(&format!("render failed: {error}"));
                return Ok(());
            }
            // ~30 frames a second is enough for the background, and it keeps a
            // desktop widget off the GPU and the CPU the rest of the time.
            std::thread::sleep(std::time::Duration::from_millis(24));
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

/// Appends a line to `%TEMP%\offwork-native.log`, so a failure leaves a trace.
pub fn log(message: &str) {
    use std::io::Write;
    let path = std::env::temp_dir().join("offwork-native.log");
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

/// Seconds since midnight, local time.
fn local_seconds() -> i64 {
    let time = unsafe { GetLocalTime() };
    time.wHour as i64 * 3600 + time.wMinute as i64 * 60 + time.wSecond as i64
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
        // The whole card drags the window for now; the header will do that later.
        WM_NCHITTEST => LRESULT(HTCAPTION as isize),
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
