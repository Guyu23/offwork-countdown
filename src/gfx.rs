// Direct3D 11 draws the shader into its own texture. Direct2D then composites a
// soft shadow and the rounded card into a premultiplied swap chain, and
// DirectWrite lays the text out. One device, one swap chain, one present.

use std::ops::Deref;

use windows::core::{Interface, Result, HSTRING, PCSTR};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::Graphics::Gdi::{HMONITOR, ScreenToClient};

use crate::dock::{self, Dock, Edges, Placement};
use crate::text::{phrases, LANG_COUNT, LANG_NAMES};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, SetWindowTextW, WindowFromPoint};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER,
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP,
    D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory,     D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE,
    D2D1_ARC_SIZE_SMALL, D2D1_BITMAP_BRUSH_PROPERTIES1, D2D1_BITMAP_PROPERTIES, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_ELLIPSE,
    D2D1_EXTEND_MODE_CLAMP, D2D1_EXTEND_MODE_WRAP, D2D1_GAMMA_2_2,
    D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
    D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1, D2D1_LINE_JOIN_ROUND,
    D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES, D2D1_PROPERTY_TYPE_FLOAT,
    D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES1, D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
    CLSID_D2D1GaussianBlur, ID2D1Bitmap, ID2D1Bitmap1, ID2D1BitmapBrush1, ID2D1Brush, ID2D1Device, ID2D1DeviceContext, ID2D1Effect, ID2D1Factory1,
    ID2D1Factory, ID2D1Geometry, ID2D1GeometrySink, ID2D1Image, ID2D1Layer, ID2D1LinearGradientBrush,
    ID2D1RenderTarget,
    ID2D1PathGeometry, ID2D1SolidColorBrush, ID2D1StrokeStyle,
    D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_INTERPOLATION_MODE_LINEAR,
};
use windows_numerics::{Matrix3x2, Vector2};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionClip, IDCompositionDevice, IDCompositionRectangleClip,
    IDCompositionTarget, IDCompositionVisual,
};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_11_0,
    D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Buffer, ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader,
    ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11Texture2D,
    ID3D11VertexShader, D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_RENDER_TARGET,
    D3D11_BIND_SHADER_RESOURCE, D3D11_COMPARISON_NEVER, D3D11_CPU_ACCESS_WRITE,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_MAPPED_SUBRESOURCE,
    D3D11_MAP_WRITE_DISCARD, D3D11_SAMPLER_DESC, D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_WRAP,
    D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, D3D11_VIEWPORT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFactory3, IDWriteFactory5, IDWriteFontCollection1,
    IDWriteInMemoryFontFileLoader, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_SIMULATIONS_NONE, DWRITE_TEXT_RANGE,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_FAR,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory2, IDXGIDevice, IDXGIFactory2, IDXGIOutput, IDXGISurface, IDXGISwapChain1,
    DXGI_CREATE_FACTORY_FLAGS, DXGI_PRESENT, DXGI_SCALING_STRETCH, DXGI_SWAP_CHAIN_DESC1,
    DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
    DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

/// The card itself, in DIPs. The window is larger by `SHADOW_PAD_DIP` on every
/// side so the blurred shadow has somewhere to fall off.
pub const CARD_WIDTH_DIP: f32 = 300.0;
pub const CARD_HEIGHT_DIP: f32 = 150.0;
/// Taller than the countdown so the background switch fits, with the same
/// bottom inset. The window is this tall; the card eases up to it.
pub const SETTINGS_CARD_DIP: f32 = settings_card_dip();
pub const SHADOW_PAD_DIP: f32 = 40.0;
pub const CORNER_RADIUS_DIP: f32 = 16.0;
const PAD_X_DIP: f32 = 18.0;
const PAD_Y_DIP: f32 = 20.0;
const LABEL_H_DIP: f32 = 18.0;
/// Lucide icons are authored in a 24 box with a stroke of 2.
const ICON_DIP: f32 = 15.0;
const CHROME_BOX_DIP: f32 = 22.0;
const CHROME_GAP_DIP: f32 = 2.0;
const CHROME_RADIUS_DIP: f32 = 6.0;
const COMPACT_W_DIP: f32 = 232.0;
const COMPACT_H_DIP: f32 = 40.0;
const TIME_EXPANDED_DIP: f32 = 40.0;
const TIME_COMPACT_DIP: f32 = 16.0;
/// Width reserved for 「距离下班」 so the compact time can sit beside it.
const LABEL_BLOCK_DIP: f32 = 68.0;
/// Settings steppers. Hours move by one, minutes by five.
const STEP_BTN: f32 = 24.0;
const STEP_ROW: f32 = 36.0;
const FIELD_NUM_W: f32 = 30.0;
const STEP_GAP: f32 = 2.0;
const CLUSTER_GAP: f32 = 8.0;
const COLON_W: f32 = 10.0;
/// Background segment. A rounded track with a sliding pill.
const SEG_H: f32 = 32.0;
/// 默认, 大海, 极光, 山影. Four labels do not fit beside the steppers.
const THEME_COUNT: f32 = 4.0;
const SEG_INSET: f32 = 3.0;
const SEG_SECS: f32 = 0.36;
/// Fold and the settings page share one ease-out. Off-work keeps its own pace.
const FOLD_SECS: f32 = 0.30;
const PAGE_SECS: f32 = 0.34;
const DONE_SECS: f32 = 0.55;
const MUTED: (f32, f32, f32) = (0.84, 0.88, 0.93);
const WHITE: (f32, f32, f32) = (1.0, 1.0, 1.0);
const BLUE: (f32, f32, f32) = (0.23, 0.51, 0.96);
/// Off-work green, #25D366. Brighter than #16A34A so the phrase and the bar
/// stay clear on fog and bright sky, short of the pale mint that felt too much.
const GREEN: (f32, f32, f32) = (37.0 / 255.0, 211.0 / 255.0, 102.0 / 255.0);
/// Sakura, the second reminder color. Saturated enough to read on paper and on the aurora.
const PINK: (f32, f32, f32) = (1.0, 0.42, 0.62);
/// Body of the reference button. The bevel is a separate one-pixel rim, not a wash.
const SKY: (f32, f32, f32) = (37.0 / 255.0, 121.0 / 255.0, 1.0);
/// WPF ink on the paper card: `TextPrimary` #1B1F27, `TextSecondary` #5F6B7A.
const INK: (f32, f32, f32) = (27.0 / 255.0, 31.0 / 255.0, 39.0 / 255.0);
const INK_SOFT: (f32, f32, f32) = (95.0 / 255.0, 107.0 / 255.0, 122.0 / 255.0);
/// WPF `Track` #E8EDF4, the segment track and the paper progress track.
const TRACK: (f32, f32, f32) = (232.0 / 255.0, 237.0 / 255.0, 244.0 / 255.0);
/// WPF `StepperFill` #EAEFF6.
const CHIP: (f32, f32, f32) = (234.0 / 255.0, 239.0 / 255.0, 246.0 / 255.0);

/// What the card shows this frame.
pub struct Frame {
    pub time_text: String,
    pub label: String,
    pub done_label: String,
    pub farewell: String,
    pub bye: String,
    pub footer_left: String,
    /// Shown in place of `footer_left` once the shift is over.
    pub footer_done: String,
    pub footer_right: String,
    pub progress: f32,
    /// The shift is over. The card eases into the green off-work state.
    pub done: bool,
    pub paused: bool,
}

#[repr(C)]
struct Params {
    // Matches the HLSL packing: `float g_time` at 0, `float2 g_resolution` at
    // 4 (a vector only moves to the next register if it would straddle one),
    // `float g_mode` at 12. Padded to 16 bytes.
    time: f32,
    width: f32,
    height: f32,
    mode: f32,
    _pad: [f32; 4],
}

/// The ported ocean shader's `cbuffer Globals`.
#[repr(C)]
struct OceanParams {
    resolution: [f32; 3],
    time: f32,
    mouse: [f32; 4],
    channel_resolution: [[f32; 3]; 4],
}

pub struct Gfx {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    swap_chain: IDXGISwapChain1,
    /// The shader's colour buffer; Direct2D samples it as the card's backdrop.
    scene_bitmap: ID2D1Bitmap1,
    scene_view: ID3D11RenderTargetView,
    d2d: ID2D1DeviceContext,
    text_time: IDWriteTextFormat,
    text_label: IDWriteTextFormat,
    text_footer: IDWriteTextFormat,
    text_footer_right: IDWriteTextFormat,
    text_segment: IDWriteTextFormat,
    /// Bold face for the four reminder-color names. The file is Regular, so this is simulated.
    text_accent: IDWriteTextFormat,
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    constants: ID3D11Buffer,
    /// The ported Shadertoy ocean: its own pixel shader, uniforms and channels.
    ocean_shader: ID3D11PixelShader,
    /// nimitz's Auroras (shadertoy XtGGRt). Shares the ocean uniform buffer.
    aurora_shader: ID3D11PixelShader,
    /// athibaul's Piau-Engaly ridges. Same uniform buffer, time and resolution only.
    mountain_shader: ID3D11PixelShader,
    ocean_constants: ID3D11Buffer,
    channels: Vec<ID3D11ShaderResourceView>,
    channel_sizes: [[f32; 3]; 4],
    channel_sampler: ID3D11SamplerState,
    // Kept alive: dropping them detaches the premultiplied swap chain from the window.
    #[allow(dead_code)]
    composition: IDCompositionDevice,
    #[allow(dead_code)]
    composition_target: IDCompositionTarget,
    #[allow(dead_code)]
    composition_visual: IDCompositionVisual,
    /// Shader target: the card, without the shadow.
    card_w: u32,
    card_h: u32,
    /// Card origin and size in DIPs, matching the rounded clip.
    origin_dip: f32,
    card_dip_w: f32,
    card_dip_h: f32,
    hwnd: HWND,
    shadow: ID2D1Bitmap1,
    shadow_shape: ID2D1Bitmap1,
    dwrite: IDWriteFactory,
    /// Keeps the in-memory faces registered. DirectWrite drops them with the loader.
    #[allow(dead_code)]
    font_loader: IDWriteInMemoryFontFileLoader,
    font_sc: IDWriteFontCollection1,
    font_jp: IDWriteFontCollection1,
    font_kr: IDWriteFontCollection1,
    /// One centered format per language, used only for the language names.
    lang_fonts: [IDWriteTextFormat; 4],
    /// 0 expanded, 1 the single-row pill. `compact_target` is where the
    /// animation is going. The other three fields are the current ease-out leg.
    compact: f32,
    compact_from: f32,
    compact_to: f32,
    compact_u: f32,
    compact_target: bool,
    /// Mouse-down for a click that should not immediately drag.
    press: Option<(i32, i32, Press)>,
    shadow_key: (i32, i32, i32, i32, i32),
    clip: ID2D1Geometry,
    layer: ID2D1Layer,
    /// Blurs the backdrop under the progress track.
    frost: ID2D1Effect,
    frost_layer: ID2D1Layer,
    /// Round caps, matching Lucide's stroke.
    stroke: ID2D1StrokeStyle,
    /// Lucide "settings", in the 24-unit icon space.
    gear: ID2D1Geometry,
    dpi: f32,
    clock: f32,
    dock: Option<Dock>,
    /// The card is mostly past its docked edge, so a click reveals it.
    tucked: bool,
    drag_cursor: POINT,
    drag_tucked: bool,
    /// A system move is in progress, so the padding must keep receiving it.
    moving: bool,
    layout_dirty: bool,
    /// Last work-area clip pushed to the composition visual. `None` is no clip.
    clip_key: Option<(i32, i32, i32, i32)>,
    /// Kept alive while the visual is clipped to the work area.
    dock_clip: Option<IDCompositionRectangleClip>,
    /// Pose the rounded clip was built for. Rebuilt only while the card folds.
    mask_key: (i32, i32, i32, i32, i32),
    /// One Direct2D bitmap per swap-chain buffer. Flip discards the previous
    /// buffer index, so the bitmap has to be remembered rather than rebuilt.
    back_targets: Vec<(isize, ID2D1Bitmap1)>,
    brush_white: ID2D1SolidColorBrush,
    brush_muted: ID2D1SolidColorBrush,
    brush_faded: ID2D1SolidColorBrush,
    brush_accent: ID2D1SolidColorBrush,
    brush_track: ID2D1SolidColorBrush,
    brush_hover: ID2D1SolidColorBrush,
    /// WPF `CardFill`: #FDFEFF → #F6FAFE → #EFF5FC, top-left to bottom-right.
    paper: ID2D1LinearGradientBrush,
    /// Minutes from midnight. The countdown reads these every second.
    start_min: i32,
    end_min: i32,
    settings: bool,
    /// 0 = countdown, 1 = settings. Eased separately from the bool target.
    settings_t: f32,
    settings_from: f32,
    settings_to: f32,
    settings_u: f32,
    /// Selected background. 0 paper, 1 ocean, 2 aurora.
    /// `sea` eases the dark backdrop (paper is 0, either shader is 1).
    /// `pill` is the sliding segment, in slots. `backdrop` is the shader
    /// still on screen while paper fades in.
    theme: u8,
    sea: f32,
    sea_from: f32,
    sea_to: f32,
    sea_u: f32,
    pill: f32,
    pill_from: f32,
    pill_to: f32,
    pill_u: f32,
    /// 0 中文, 1 日本語, 2 English, 3 한국어. `lang_pill` eases the switch.
    lang: u8,
    lang_pill: f32,
    lang_pill_from: f32,
    lang_pill_to: f32,
    lang_pill_u: f32,
    /// 0 竹绿, 1 樱粉, 2 天蓝, 3 鎏金. The pill eases; the fill is applied to the off-work text.
    accent: u8,
    accent_pill: f32,
    accent_pill_from: f32,
    accent_pill_to: f32,
    accent_pill_u: f32,
    /// Edge tuck. On by default; off keeps a dropped card inside the work area.
    tuck: bool,
    tuck_pill: f32,
    tuck_pill_from: f32,
    tuck_pill_to: f32,
    tuck_pill_u: f32,
    /// 0 always show the writing, 1 show it only while the pointer is on the card.
    caption: u8,
    caption_pill: f32,
    caption_pill_from: f32,
    caption_pill_to: f32,
    caption_pill_u: f32,
    /// 1 while the writing and icons are visible.
    copy_t: f32,
    copy_from: f32,
    copy_to: f32,
    copy_u: f32,
    /// Holds the writing up briefly after the pointer leaves, so a graze does not flicker.
    caption_hold: f32,
    card_hover: bool,
    /// Settings page height. Shrinks when the caption row is hidden.
    page_dip: f32,
    /// A free card that should be moved fully inside the work area.
    seat: Option<(i32, i32)>,
    /// Gold-powder tile, rewritten each frame while the gilt phrase is on screen.
    gold_bitmap: ID2D1Bitmap,
    gold_brush: ID2D1BitmapBrush1,
    /// Clock value already uploaded into `gold_bitmap`, so a second phrase skips the refill.
    gold_stamp: f32,
    backdrop: u8,
    /// 0 = counting, 1 = off work. Snaps on the first frame, then eases.
    done_t: f32,
    done_target: bool,
    seen_done: bool,
    schedule_dirty: bool,
    /// Delay before a held stepper repeats, then the gap between repeats.
    hold_delay: f32,
}

const SHADER: &str = include_str!("shader.hlsl");
const OCEAN_SHADER: &str = include_str!("ocean.hlsl");
const AURORA_SHADER: &str = include_str!("aurora.hlsl");
const MOUNTAIN_SHADER: &str = include_str!("mountains.hlsl");
const CHANNEL0_PNG: &[u8] = include_bytes!("../assets/Channel0.png");
const CHANNEL1_JPG: &[u8] = include_bytes!("../assets/Channel1.jpg");
const TEXT_FAMILY: windows::core::PCWSTR = windows::core::w!("OffWork Sans");

fn fail() -> windows::core::Error {
    windows::core::Error::from(windows::Win32::Foundation::E_FAIL)
}

fn d2d_rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F { left, top, right, bottom }
}

/// Seconds since local midnight.
fn local_day_seconds() -> f32 {
    let time = unsafe { GetLocalTime() };
    time.wHour as f32 * 3600.0 + time.wMinute as f32 * 60.0 + time.wSecond as f32
}

/// Sun position for the ocean shader, as a fraction of the card.
///
/// The shader draws the disk at this point and treats `y = 0.25` as the
/// horizon (`Dsfact`, from `Dfact` 0.5). A component of exactly 0 is its
/// locked daytime pose, so the path stays off zero. The disk rises at 06:00
/// on the left, stands highest at noon, and sets at 18:00 on the right.
fn sun_uv(day_seconds: f32) -> (f32, f32) {
    const HORIZON: f32 = 0.25;
    const NOON: f32 = 0.72;
    let day = 24.0 * 3600.0;
    let daylight = 12.0 * 3600.0;
    let t = (day_seconds - 6.0 * 3600.0).rem_euclid(day);
    let alt = (t / daylight * std::f32::consts::PI).sin();
    let across = if t <= daylight { t / daylight } else { 1.0 - (t - daylight) / daylight };
    let x = 0.08 + 0.84 * across;
    let y = if alt >= 0.0 {
        HORIZON + alt * (NOON - HORIZON)
    } else {
        (HORIZON + alt * 1.15).max(-0.08)
    };
    (x, y)
}

/// `iMouse` in the card's pixels. Shared with the aurora pass, which keeps
/// the untouched camera when this is zero.
fn ocean_sun(day_seconds: f32, width: f32, height: f32) -> [f32; 4] {
    let (x, y) = sun_uv(day_seconds);
    [x * width, y * height, 0.0, 0.0]
}

fn rgba(r: f32, g: f32, b: f32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F { r, g, b, a }
}

/// Compiles one entry point, e.g. `("PSMain", "ps_5_0")`. A failure carries the
/// compiler's own message, with the line in the source, which is what a shader
/// port needs to see.
fn compile(source: &str, entry: &str, target: &str) -> Result<Vec<u8>> {
    let mut entry_bytes = entry.as_bytes().to_vec();
    entry_bytes.push(0);
    let mut target_bytes = target.as_bytes().to_vec();
    target_bytes.push(0);
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    unsafe {
        let result = D3DCompile(
            source.as_ptr() as *const core::ffi::c_void,
            source.len(),
            None,
            None,
            None,
            PCSTR(entry_bytes.as_ptr()),
            PCSTR(target_bytes.as_ptr()),
            // D3DCOMPILE_SKIP_OPTIMIZATION (4): fxc's optimiser gives up on the
            // ported ocean pass, and the GPU does not need it.
            4,
            0,
            &mut code,
            Some(&mut errors),
        );
        if let Err(error) = result {
            if let Some(blob) = errors {
                let size = blob.GetBufferSize() as usize;
                let pointer = blob.GetBufferPointer() as *const u8;
                let text = std::slice::from_raw_parts(pointer, size);
                return Err(windows::core::Error::new(
                    error.code(),
                    String::from_utf8_lossy(text).into_owned(),
                ));
            }
            return Err(error);
        }
        let blob = code.ok_or_else(fail)?;
        let size = blob.GetBufferSize() as usize;
        let pointer = blob.GetBufferPointer() as *const u8;
        Ok(std::slice::from_raw_parts(pointer, size).to_vec())
    }
}

impl Gfx {
    pub fn new(hwnd: HWND, width: u32, height: u32, dpi: f32) -> Result<Self> {
        unsafe {
            let place = metrics(width, height, dpi);
            let (card_w, card_h) = (place.card_w, place.card_h);
            let mut device: Option<ID3D11Device> = None;
            let mut feature_level = D3D_FEATURE_LEVEL::default();
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                Default::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                7,
                Some(&mut device),
                Some(&mut feature_level),
                None,
            )?;
            let device = device.ok_or_else(fail)?;
            let context = device.GetImmediateContext()?;

            let dxgi_device: IDXGIDevice = device.cast()?;
            // The factory that owns the adapter rejects a premultiplied HWND
            // swap chain. A factory from CreateDXGIFactory2 accepts it.
            let factory: IDXGIFactory2 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: width,
                Height: height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: false.into(),
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                Flags: 0,
            };
            // A HWND swap chain refuses premultiplied alpha on this machine
            // (DXGI_ERROR_INVALID_CALL). Composition swap chains accept it, and
            // DirectComposition is what puts that bitmap on the window.
            let swap_chain = factory.CreateSwapChainForComposition(&device, &desc, None::<&IDXGIOutput>)?;
            let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let composition_target = composition.CreateTargetForHwnd(hwnd, true)?;
            let composition_visual = composition.CreateVisual()?;
            composition_visual.SetContent(&swap_chain)?;
            composition_target.SetRoot(&composition_visual)?;
            composition.Commit()?;

            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device: ID2D1Device = d2d_factory.CreateDevice(&dxgi_device)?;
            let d2d: ID2D1DeviceContext =
                d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            // The device context's own DPI decides how a DIP becomes a pixel; it
            // defaults to 96, which would draw everything at 1:1 on a scaled
            // display. It has to match the target bitmaps'.
            d2d.SetDpi(dpi, dpi);

            // The shader writes opaque colour. IGNORE tells Direct2D to treat
            // every texel as alpha 1; PREMULTIPLIED here is rejected for a
            // plain texture surface.
            let pixel_format = D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            };
            let scene_desc = D3D11_TEXTURE2D_DESC {
                Width: card_w,
                Height: card_h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut scene_texture: Option<ID3D11Texture2D> = None;
            device.CreateTexture2D(&scene_desc, None, Some(&mut scene_texture))?;
            let scene_texture = scene_texture.ok_or_else(fail)?;
            let mut scene_view: Option<ID3D11RenderTargetView> = None;
            device.CreateRenderTargetView(&scene_texture, None, Some(&mut scene_view))?;
            let scene_view = scene_view.ok_or_else(fail)?;
            let scene_surface: IDXGISurface = scene_texture.cast()?;
            let scene_props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: pixel_format,
                dpiX: dpi,
                dpiY: dpi,
                bitmapOptions: D2D1_BITMAP_OPTIONS_NONE,
                colorContext: Default::default(),
            };
            let scene_bitmap = d2d.CreateBitmapFromDxgiSurface(&scene_surface, Some(&scene_props))?;

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let factory5: IDWriteFactory5 = dwrite.cast()?;
            let font_loader = factory5.CreateInMemoryFontFileLoader()?;
            dwrite.RegisterFontFileLoader(&font_loader)?;
            let factory3: IDWriteFactory3 = dwrite.cast()?;
            let font_sc = embed_face(&dwrite, &factory3, &font_loader, include_bytes!("fonts/sc.ttf"))?;
            let font_jp = embed_face(&dwrite, &factory3, &font_loader, include_bytes!("fonts/jp.ttf"))?;
            let font_kr = embed_face(&dwrite, &factory3, &font_loader, include_bytes!("fonts/kr.ttf"))?;
            let lang_fonts = [
                make_format(&dwrite, &font_sc, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, windows::core::w!("zh-cn"))?,
                make_format(&dwrite, &font_jp, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, windows::core::w!("ja-jp"))?,
                make_format(&dwrite, &font_sc, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, windows::core::w!("en-us"))?,
                make_format(&dwrite, &font_kr, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, windows::core::w!("ko-kr"))?,
            ];

            let (shadow_shape, shadow) = bake_shadow(&d2d, width, height, dpi, place)?;
            let shadow_key = (
                (place.origin_dip * 4.0) as i32,
                (place.origin_dip * 4.0) as i32,
                (place.card_dip_w * 4.0) as i32,
                (place.card_dip_h * 4.0) as i32,
                (CORNER_RADIUS_DIP * 4.0) as i32,
            );
            let rounded = D2D1_ROUNDED_RECT {
                rect: d2d_rect(
                    place.origin_dip,
                    place.origin_dip,
                    place.origin_dip + place.card_dip_w,
                    place.origin_dip + place.card_dip_h,
                ),
                radiusX: CORNER_RADIUS_DIP,
                radiusY: CORNER_RADIUS_DIP,
            };
            let clip: ID2D1Geometry = d2d_factory.CreateRoundedRectangleGeometry(&rounded)?.cast()?;
            let layer = d2d.CreateLayer(None)?;
            let frost = d2d.CreateEffect(&CLSID_D2D1GaussianBlur)?;
            let frost_input: ID2D1Image = scene_bitmap.cast()?;
            frost.SetInput(0, &frost_input, true);
            let deviation = 10.0f32;
            frost.SetValue(
                D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,
                D2D1_PROPERTY_TYPE_FLOAT,
                &deviation.to_ne_bytes(),
            )?;
            let frost_layer = d2d.CreateLayer(None)?;
            let stroke_props = D2D1_STROKE_STYLE_PROPERTIES1 {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
                transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
            };
            let stroke: ID2D1StrokeStyle = d2d_factory.CreateStrokeStyle(&stroke_props, None)?.cast()?;
            let gear = lucide_settings_geometry(d2d_factory.deref())?;

            let vs_bytes = compile(SHADER, "VSMain", "vs_5_0")?;
            let ps_bytes = compile(SHADER, "PSMain", "ps_5_0")?;
            let mut vertex_shader: Option<ID3D11VertexShader> = None;
            device.CreateVertexShader(&vs_bytes, None, Some(&mut vertex_shader))?;
            let mut pixel_shader: Option<ID3D11PixelShader> = None;
            device.CreatePixelShader(&ps_bytes, None, Some(&mut pixel_shader))?;
            let vertex_shader = vertex_shader.ok_or_else(fail)?;
            let pixel_shader = pixel_shader.ok_or_else(fail)?;

            let buffer = windows::Win32::Graphics::Direct3D11::D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<Params>() as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                MiscFlags: 0,
                StructureByteStride: 0,
            };
            let mut constants: Option<ID3D11Buffer> = None;
            device.CreateBuffer(&buffer, None, Some(&mut constants))?;
            let constants = constants.ok_or_else(fail)?;

            // The ported Shadertoy ocean: its own shader, uniforms and channels.
            let started = std::time::Instant::now();
            let ocean_bytes = compile(OCEAN_SHADER, "PSMain", "ps_5_0")?;
            crate::log(&format!(
                "ocean shader compiled in {} ms",
                started.elapsed().as_millis()
            ));
            let mut ocean_shader: Option<ID3D11PixelShader> = None;
            device.CreatePixelShader(&ocean_bytes, None, Some(&mut ocean_shader))?;
            let ocean_shader = ocean_shader.ok_or_else(fail)?;

            let started_aurora = std::time::Instant::now();
            let aurora_bytes = compile(AURORA_SHADER, "PSMain", "ps_5_0")?;
            crate::log(&format!(
                "aurora shader compiled in {} ms",
                started_aurora.elapsed().as_millis()
            ));
            let mut aurora_shader: Option<ID3D11PixelShader> = None;
            device.CreatePixelShader(&aurora_bytes, None, Some(&mut aurora_shader))?;
            let aurora_shader = aurora_shader.ok_or_else(fail)?;

            let started_mountains = std::time::Instant::now();
            let mountain_bytes = compile(MOUNTAIN_SHADER, "PSMain", "ps_5_0")?;
            crate::log(&format!(
                "mountain shader compiled in {} ms",
                started_mountains.elapsed().as_millis()
            ));
            let mut mountain_shader: Option<ID3D11PixelShader> = None;
            device.CreatePixelShader(&mountain_bytes, None, Some(&mut mountain_shader))?;
            let mountain_shader = mountain_shader.ok_or_else(fail)?;

            let ocean_buffer = windows::Win32::Graphics::Direct3D11::D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<OceanParams>() as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                MiscFlags: 0,
                StructureByteStride: 0,
            };
            let mut ocean_constants: Option<ID3D11Buffer> = None;
            device.CreateBuffer(&ocean_buffer, None, Some(&mut ocean_constants))?;
            let ocean_constants = ocean_constants.ok_or_else(fail)?;

            let (channels, channel_sizes) = ocean_channels(&device)?;
            let sampler_desc = D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressV: D3D11_TEXTURE_ADDRESS_WRAP,
                AddressW: D3D11_TEXTURE_ADDRESS_WRAP,
                MipLODBias: 0.0,
                MaxAnisotropy: 1,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                BorderColor: [0.0; 4],
                MinLOD: 0.0,
                MaxLOD: f32::MAX,
            };
            let mut channel_sampler: Option<ID3D11SamplerState> = None;
            device.CreateSamplerState(&sampler_desc, Some(&mut channel_sampler))?;
            let channel_sampler = channel_sampler.ok_or_else(fail)?;

            let brush_white = brush(&d2d, rgba(1.0, 1.0, 1.0, 1.0))?;
            let brush_muted = brush(&d2d, rgba(0.84, 0.88, 0.93, 1.0))?;
            let brush_faded = brush(&d2d, rgba(0.84, 0.88, 0.93, 1.0))?;
            let brush_accent = brush(&d2d, rgba(0.23, 0.51, 0.96, 1.0))?;
            let brush_track = brush(&d2d, rgba(0.15, 0.20, 0.27, 1.0))?;
            let brush_hover = brush(&d2d, rgba(0.0, 0.0, 0.0, 0.22))?;
            let paper_stops = [
                D2D1_GRADIENT_STOP { position: 0.0, color: rgba(0xFD as f32 / 255.0, 0xFE as f32 / 255.0, 0xFF as f32 / 255.0, 1.0) },
                D2D1_GRADIENT_STOP { position: 0.55, color: rgba(0xF6 as f32 / 255.0, 0xFA as f32 / 255.0, 0xFE as f32 / 255.0, 1.0) },
                D2D1_GRADIENT_STOP { position: 1.0, color: rgba(0xEF as f32 / 255.0, 0xF5 as f32 / 255.0, 0xFC as f32 / 255.0, 1.0) },
            ];
            let paper_target: ID2D1RenderTarget = d2d.cast()?;
            let paper_stops = paper_target.CreateGradientStopCollection(&paper_stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)?;
            let paper_props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                startPoint: Vector2 { X: 0.0, Y: 0.0 },
                endPoint: Vector2 { X: 1.0, Y: 1.0 },
            };
            let paper = d2d.CreateLinearGradientBrush(&paper_props, None, &paper_stops)?;
            let (gold_bitmap, gold_brush) = gold_tile(&d2d, dpi)?;

            Ok(Self {
                text_time: make_format(&dwrite, &font_sc, 40.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, windows::core::w!("zh-cn"))?,
                text_label: make_format(&dwrite, &font_sc, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, windows::core::w!("zh-cn"))?,
                text_footer: make_format(&dwrite, &font_sc, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_FAR, windows::core::w!("zh-cn"))?,
                text_footer_right: make_format(&dwrite, &font_sc, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_PARAGRAPH_ALIGNMENT_FAR, windows::core::w!("zh-cn"))?,
                text_segment: make_format(&dwrite, &font_sc, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, windows::core::w!("zh-cn"))?,
                text_accent: make_format(&dwrite, &font_sc, 13.0, DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, windows::core::w!("zh-cn"))?,
                device,
                context,
                swap_chain,
                scene_bitmap,
                scene_view,
                d2d,
                vertex_shader,
                pixel_shader,
                constants,
                ocean_shader,
                aurora_shader,
                mountain_shader,
                ocean_constants,
                channels,
                channel_sizes,
                channel_sampler,
                hwnd,
                composition,
                composition_target,
                composition_visual,
                card_w,
                card_h,
                origin_dip: place.origin_dip,
                card_dip_w: place.card_dip_w,
                card_dip_h: place.card_dip_h,
                shadow,
                shadow_shape,
                dwrite,
                font_loader,
                font_sc,
                font_jp,
                font_kr,
                lang_fonts,
                compact: 0.0,
                compact_from: 0.0,
                compact_to: 0.0,
                compact_u: 1.0,
                compact_target: false,
                press: None,
                shadow_key,
                clip,
                layer,
                frost,
                frost_layer,
                stroke,
                gear,
                dpi,
                clock: 0.0,
                dock: None,
                tucked: false,
                drag_cursor: POINT::default(),
                drag_tucked: false,
                moving: false,
                layout_dirty: false,
                clip_key: None,
                dock_clip: None,
                mask_key: shadow_key,
                back_targets: Vec::new(),
                brush_white,
                brush_muted,
                brush_faded,
                brush_accent,
                brush_track,
                brush_hover,
                paper,
                start_min: 9 * 60,
                end_min: 18 * 60,
                settings: false,
                settings_t: 0.0,
                settings_from: 0.0,
                settings_to: 0.0,
                settings_u: 1.0,
                theme: 1,
                sea: 1.0,
                sea_from: 1.0,
                sea_to: 1.0,
                sea_u: 1.0,
                pill: 1.0,
                pill_from: 1.0,
                pill_to: 1.0,
                pill_u: 1.0,
                lang: 0,
                lang_pill: 0.0,
                lang_pill_from: 0.0,
                lang_pill_to: 0.0,
                lang_pill_u: 1.0,
                accent: 0,
                accent_pill: 0.0,
                accent_pill_from: 0.0,
                accent_pill_to: 0.0,
                accent_pill_u: 1.0,
                tuck: true,
                tuck_pill: 0.0,
                tuck_pill_from: 0.0,
                tuck_pill_to: 0.0,
                tuck_pill_u: 1.0,
                caption: 0,
                caption_pill: 0.0,
                caption_pill_from: 0.0,
                caption_pill_to: 0.0,
                caption_pill_u: 1.0,
                copy_t: 1.0,
                copy_from: 1.0,
                copy_to: 1.0,
                copy_u: 1.0,
                caption_hold: 0.16,
                card_hover: false,
                page_dip: settings_page_dip(true),
                seat: None,
                gold_bitmap,
                gold_brush,
                gold_stamp: -1.0,
                backdrop: 1,
                done_t: 0.0,
                done_target: false,
                seen_done: false,
                schedule_dirty: false,
                hold_delay: 0.0,
            })
        }
    }

    /// Advance the fold before the window position is settled for this frame.
    pub fn prepare(&mut self, dt: f32) {
        self.advance_compact(dt);
        let target = if self.settings { 1.0 } else { 0.0 };
        glide(
            &mut self.settings_t, &mut self.settings_from, &mut self.settings_to, &mut self.settings_u,
            target, dt, PAGE_SECS,
        );
        let dark = if self.theme == 0 { 0.0 } else { 1.0 };
        glide(&mut self.sea, &mut self.sea_from, &mut self.sea_to, &mut self.sea_u, dark, dt, SEG_SECS);
        glide(
            &mut self.pill, &mut self.pill_from, &mut self.pill_to, &mut self.pill_u,
            self.theme as f32, dt, SEG_SECS,
        );
        glide(
            &mut self.lang_pill, &mut self.lang_pill_from, &mut self.lang_pill_to, &mut self.lang_pill_u,
            self.lang as f32, dt, SEG_SECS,
        );
        glide(
            &mut self.accent_pill, &mut self.accent_pill_from, &mut self.accent_pill_to, &mut self.accent_pill_u,
            self.accent as f32, dt, SEG_SECS,
        );
        glide(
            &mut self.tuck_pill, &mut self.tuck_pill_from, &mut self.tuck_pill_to, &mut self.tuck_pill_u,
            if self.tuck { 0.0 } else { 1.0 }, dt, SEG_SECS,
        );
        glide(
            &mut self.caption_pill, &mut self.caption_pill_from, &mut self.caption_pill_to, &mut self.caption_pill_u,
            self.caption as f32, dt, SEG_SECS,
        );
        let scenic = self.theme != 0;
        let hide_copy = scenic && self.caption == 1 && !self.settings;
        if !hide_copy || self.card_hover {
            self.caption_hold = 0.16;
        } else {
            self.caption_hold = (self.caption_hold - dt).max(0.0);
        }
        let show_copy = !hide_copy || self.card_hover || self.caption_hold > 0.0;
        glide(
            &mut self.copy_t, &mut self.copy_from, &mut self.copy_to, &mut self.copy_u,
            if show_copy { 1.0 } else { 0.0 }, dt, 0.22,
        );
        let page = settings_page_dip(scenic);
        self.page_dip += (page - self.page_dip) * (dt / 0.18).min(1.0);
        if self.theme != 0 {
            self.backdrop = self.theme;
        }
        self.repeat_step(dt);
    }

    pub fn work_seconds(&self) -> (i64, i64) {
        (self.start_min as i64 * 60, self.end_min as i64 * 60)
    }

    pub fn set_work_minutes(&mut self, start: i32, end: i32) {
        self.start_min = start.clamp(0, 24 * 60 - 1);
        self.end_min = end.clamp(0, 24 * 60 - 1);
    }

    /// Restores a saved background without playing the pill slide.
    pub fn set_theme(&mut self, theme: u8) {
        let theme = if theme < 4 { theme } else { 1 };
        self.theme = theme;
        if theme != 0 {
            self.backdrop = theme;
        }
        let dark = if theme == 0 { 0.0 } else { 1.0 };
        self.sea = dark;
        self.sea_from = dark;
        self.sea_to = dark;
        self.sea_u = 1.0;
        let slot = theme as f32;
        self.pill = slot;
        self.pill_from = slot;
        self.pill_to = slot;
        self.pill_u = 1.0;
    }

    pub fn lang(&self) -> u8 {
        self.lang
    }

    /// Restores a saved language without sliding the pill, and retitles the window.
    pub fn set_lang(&mut self, lang: u8) {
        let lang = if lang < LANG_COUNT { lang } else { 0 };
        self.lang = lang;
        let slot = lang as f32;
        self.lang_pill = slot;
        self.lang_pill_from = slot;
        self.lang_pill_to = slot;
        self.lang_pill_u = 1.0;
        let _ = self.rebuild_formats();
        self.apply_title();
    }

    pub fn accent(&self) -> u8 {
        self.accent
    }

    pub fn tuck_enabled(&self) -> bool {
        self.tuck
    }

    /// Restores the edge-tuck switch without sliding its pill.
    pub fn set_tuck(&mut self, enabled: bool) {
        self.tuck = enabled;
        let slot = if enabled { 0.0 } else { 1.0 };
        self.tuck_pill = slot;
        self.tuck_pill_from = slot;
        self.tuck_pill_to = slot;
        self.tuck_pill_u = 1.0;
    }

    pub fn caption_mode(&self) -> u8 {
        self.caption
    }

    /// Restores the writing mode. 0 stays on the card, 1 waits for the pointer.
    pub fn set_caption(&mut self, mode: u8) {
        self.caption = if mode == 1 { 1 } else { 0 };
        let slot = self.caption as f32;
        self.caption_pill = slot;
        self.caption_pill_from = slot;
        self.caption_pill_to = slot;
        self.caption_pill_u = 1.0;
    }

    pub fn set_card_hover(&mut self, on: bool) {
        self.card_hover = on;
    }

    pub fn take_seat(&mut self) -> Option<(i32, i32)> {
        self.seat.take()
    }

    /// Restores a saved reminder color without sliding the pill.
    pub fn set_accent(&mut self, accent: u8) {
        let accent = if accent < 4 { accent } else { 0 };
        self.accent = accent;
        let slot = accent as f32;
        self.accent_pill = slot;
        self.accent_pill_from = slot;
        self.accent_pill_to = slot;
        self.accent_pill_u = 1.0;
    }

    fn apply_title(&self) {
        let title = HSTRING::from(phrases(self.lang).title);
        unsafe {
            let _ = SetWindowTextW(self.hwnd, &title);
        }
    }

    fn rebuild_formats(&mut self) -> Result<()> {
        let lang = self.lang;
        let locale = match lang {
            1 => windows::core::w!("ja-jp"),
            2 => windows::core::w!("en-us"),
            3 => windows::core::w!("ko-kr"),
            _ => windows::core::w!("zh-cn"),
        };
        let (time, label, footer, footer_right, segment, accent) = {
            let face = match lang {
                1 => &self.font_jp,
                3 => &self.font_kr,
                _ => &self.font_sc,
            };
            (
                make_format(&self.dwrite, face, 40.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, locale)?,
                make_format(&self.dwrite, face, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR, locale)?,
                make_format(&self.dwrite, face, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_FAR, locale)?,
                make_format(&self.dwrite, face, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_PARAGRAPH_ALIGNMENT_FAR, locale)?,
                make_format(&self.dwrite, face, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, locale)?,
                make_format(&self.dwrite, face, 13.0, DWRITE_FONT_WEIGHT_BOLD, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, locale)?,
            )
        };
        self.text_time = time;
        self.text_label = label;
        self.text_footer = footer;
        self.text_footer_right = footer_right;
        self.text_segment = segment;
        self.text_accent = accent;
        Ok(())
    }

    pub fn theme(&self) -> u8 {
        self.theme
    }

    pub fn take_schedule_dirty(&mut self) -> bool {
        let dirty = self.schedule_dirty;
        self.schedule_dirty = false;
        dirty
    }

    pub fn render(&mut self, frame: &Frame, dt: f32) -> Result<()> {
        if !frame.paused {
            self.clock += dt;
        }
        self.advance_done(frame.done, dt);
        let pose = self.pose();
        self.ensure_shadow(pose)?;
        let animating = self.is_animating();
        unsafe {
            // 1. The shader draws into its own colour buffer.
            let scene_view = self.scene_view.clone();
            if self.sea > 0.02 {
                let scale = if self.dpi > 0.0 { self.dpi / 96.0 } else { 1.0 };
                let render_px = (self.open_height() * scale).round().clamp(1.0, self.card_h as f32);
                self.draw_shader(&scene_view, render_px, self.backdrop)?;
            }
            self.context.OMSetRenderTargets(None, None);
            self.context.Flush();

            // 2. Direct2D composes the card on the swap chain's back buffer.
            //    The bitmap is cached per buffer; creating one every frame
            //    stalls behind the driver.
            let target = self.swap_target()?;
            self.d2d.SetTarget(&target);
            self.draw_card(frame)?;
            // Drop the target before Present, or the swap chain stays bound and
            // the premultiplied frame can come out unchanged.
            self.d2d.SetTarget(None::<&ID2D1Image>);

            // While the card is folding, wait for the display so the steps are
            // even. Idle keeps Present(0): the ocean does not need 60 fps, and
            // the loop sleeps the rest of the frame.
            let interval = if animating { 1 } else { 0 };
            self.swap_chain.Present(interval, DXGI_PRESENT(0)).ok()?;
        }
        Ok(())
    }

    pub fn is_animating(&self) -> bool {
        let target = if self.compact_target { 1.0 } else { 0.0 };
        let folding = (self.compact - target).abs() > 0.001;
        let docking = self.dock.as_ref().is_some_and(|dock| dock.t < 1.0 && !dock.suspended);
        let page = (self.settings_t - if self.settings { 1.0 } else { 0.0 }).abs() > 0.001;
        let celebrate = (self.done_t - if self.done_target { 1.0 } else { 0.0 }).abs() > 0.001;
        let dark = if self.theme == 0 { 0.0 } else { 1.0 };
        let fading = (self.sea - dark).abs() > 0.001;
        let sliding = (self.pill - self.theme as f32).abs() > 0.001;
        let lang = (self.lang_pill - self.lang as f32).abs() > 0.001;
        let accent = (self.accent_pill - self.accent as f32).abs() > 0.001;
        let gilt = self.accent == 3 && self.done_t > 0.02 && self.copy_t > 0.02 && self.settings_t < 0.98 && !self.tucked;
        let tuck = (self.tuck_pill - if self.tuck { 0.0 } else { 1.0 }).abs() > 0.001;
        let caption = (self.caption_pill - self.caption as f32).abs() > 0.001;
        let copy = (self.copy_t - self.copy_to).abs() > 0.001;
        let page_h = (self.page_dip - settings_page_dip(self.theme != 0)).abs() > 0.4;
        folding || docking || page || celebrate || fading || sliding || lang || accent || gilt || tuck || caption || copy || page_h
    }

    fn advance_done(&mut self, done: bool, dt: f32) {
        // A shift still in progress opens on the countdown. One that is already
        // over plays the green transition once, the same as crossing the end time.
        if !self.seen_done && !done {
            self.done_t = 0.0;
            self.done_target = false;
            self.seen_done = true;
            return;
        }
        self.seen_done = true;
        self.done_target = done;
        let target = if done { 1.0 } else { 0.0 };
        self.done_t = approach(self.done_t, target, dt, DONE_SECS);
    }

    /// The card is parked past an edge, so only the peek is on screen.
    pub fn is_tucked(&self) -> bool {
        self.tucked && !self.is_animating()
    }

    /// Direct2D target for the buffer `Present` will show. Flip swap chains
    /// hand back the same two textures; the pointer identifies which one.
    fn swap_target(&mut self) -> Result<ID2D1Bitmap1> {
        unsafe {
            let back: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
            let key = back.as_raw() as isize;
            if let Some((_, bitmap)) = self.back_targets.iter().find(|(cached, _)| *cached == key) {
                return Ok(bitmap.clone());
            }
            let surface: IDXGISurface = back.cast()?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: self.dpi,
                dpiY: self.dpi,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: Default::default(),
            };
            let bitmap = self.d2d.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.back_targets.push((key, bitmap.clone()));
            Ok(bitmap)
        }
    }

    fn draw_shader(&self, view: &ID3D11RenderTargetView, view_h: f32, theme: u8) -> Result<()> {
        unsafe {
            self.context.OMSetRenderTargets(Some(&[Some(view.clone())]), None);
            let viewport = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.card_w as f32,
                Height: view_h,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.context.RSSetViewports(Some(&[viewport]));
            self.context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vertex_shader, None);

            if theme >= 1 {
                let params = OceanParams {
                    resolution: [self.card_w as f32, view_h, 1.0],
                    time: self.clock,
                    mouse: if theme == 1 {
                        ocean_sun(local_day_seconds(), self.card_w as f32, view_h)
                    } else {
                        [0.0; 4]
                    },
                    channel_resolution: self.channel_sizes,
                };
                let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                self.context.Map(
                    &self.ocean_constants,
                    0,
                    D3D11_MAP_WRITE_DISCARD,
                    0,
                    Some(&mut mapped),
                )?;
                std::ptr::copy_nonoverlapping(
                    &params as *const OceanParams as *const u8,
                    mapped.pData as *mut u8,
                    std::mem::size_of::<OceanParams>(),
                );
                self.context.Unmap(&self.ocean_constants, 0);
                self.context
                    .PSSetConstantBuffers(0, Some(&[Some(self.ocean_constants.clone())]));

                if theme == 3 {
                    self.context.PSSetShader(&self.mountain_shader, None);
                    self.context.Draw(3, 0);
                    return Ok(());
                }
                if theme == 2 {
                    self.context.PSSetShader(&self.aurora_shader, None);
                    self.context.Draw(3, 0);
                    return Ok(());
                }

                self.context.PSSetShader(&self.ocean_shader, None);
                let views = [
                    self.channels.first().cloned(),
                    self.channels.get(1).cloned(),
                    self.channels.get(2).cloned(),
                ];
                self.context.PSSetShaderResources(0, Some(&views));
                self.context
                    .PSSetSamplers(0, Some(&[Some(self.channel_sampler.clone())]));
                self.context.Draw(3, 0);

                // Leave no channel bound while Direct2D draws.
                let none: [Option<ID3D11ShaderResourceView>; 3] = [None, None, None];
                self.context.PSSetShaderResources(0, Some(&none));
                return Ok(());
            }

            let params = Params {
                time: self.clock,
                width: self.card_w as f32,
                height: view_h,
                mode: 0.0,
                _pad: [0.0; 4],
            };
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&self.constants, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut mapped))?;
            std::ptr::copy_nonoverlapping(
                &params as *const Params as *const u8,
                mapped.pData as *mut u8,
                std::mem::size_of::<Params>(),
            );
            self.context.Unmap(&self.constants, 0);

            self.context.PSSetShader(&self.pixel_shader, None);
            self.context.PSSetConstantBuffers(0, Some(&[Some(self.constants.clone())]));
            self.context.Draw(3, 0);
        }
        Ok(())
    }

    fn advance_compact(&mut self, dt: f32) {
        let target = if self.compact_target { 1.0 } else { 0.0 };
        glide(
            &mut self.compact, &mut self.compact_from, &mut self.compact_to, &mut self.compact_u,
            target, dt, FOLD_SECS,
        );
    }

    /// Countdown height, grown to fit the settings page. Compact folds from here.
    fn open_height(&self) -> f32 {
        lerp(CARD_HEIGHT_DIP, self.page_dip, self.settings_t)
    }

    fn pose(&self) -> Pose {
        let ease = self.compact;
        Pose {
            x: self.origin_dip,
            y: self.origin_dip,
            w: lerp(self.card_dip_w, COMPACT_W_DIP.min(self.card_dip_w), ease),
            h: lerp(self.open_height(), COMPACT_H_DIP, ease),
            radius: lerp(CORNER_RADIUS_DIP, COMPACT_H_DIP * 0.5, ease),
            ease,
        }
    }

    fn primary(&self, alpha: f32) -> D2D1_COLOR_F {
        mix_color(INK, WHITE, self.sea, alpha)
    }

    fn secondary(&self, alpha: f32) -> D2D1_COLOR_F {
        mix_color(INK_SOFT, MUTED, self.sea, alpha)
    }

    fn ensure_shadow(&mut self, pose: Pose) -> Result<()> {
        let key = pose_key(pose);
        if key == self.shadow_key {
            return Ok(());
        }
        stamp(
            &self.d2d, &self.shadow_shape, &self.shadow,
            pose.x, pose.y, pose.w, pose.h, pose.radius,
            rgba(0.05, 0.07, 0.11, 0.32), 13.0, 7.0, true,
        )?;
        stamp(
            &self.d2d, &self.shadow_shape, &self.shadow,
            pose.x, pose.y, pose.w, pose.h, pose.radius,
            rgba(0.03, 0.04, 0.07, 0.20), 3.0, 1.5, false,
        )?;
        self.shadow_key = key;
        Ok(())
    }

    fn dip(&self, x: i32, y: i32) -> (f32, f32) {
        let scale = if self.dpi > 0.0 { self.dpi / 96.0 } else { 1.0 };
        (x as f32 / scale, y as f32 / scale)
    }

    fn hover_index(&self) -> i32 {
        if self.compact >= 0.4 {
            return -1;
        }
        let Some((x, y)) = self.cursor_client() else { return -1 };
        let (px, py) = self.dip(x, y);
        for (index, (bx, by)) in chrome_slots(self.card_dip_w).into_iter().enumerate() {
            let bx = self.origin_dip + bx;
            let by = self.origin_dip + by;
            if px >= bx && px < bx + CHROME_BOX_DIP && py >= by && py < by + CHROME_BOX_DIP {
                return index as i32;
            }
        }
        -1
    }

    fn cursor_client(&self) -> Option<(i32, i32)> {
        unsafe {
            let mut point = POINT::default();
            GetCursorPos(&mut point).ok()?;
            if WindowFromPoint(point) != self.hwnd {
                return None;
            }
            let _ = ScreenToClient(self.hwnd, &mut point);
            Some((point.x, point.y))
        }
    }

    /// Where a client pixel lands while the card is folding.
    pub fn hit(&self, x: i32, y: i32) -> Hit {
        let (px, py) = self.dip(x, y);
        let pose = self.pose();
        if !in_round(px, py, pose.x, pose.y, pose.x + pose.w, pose.y + pose.h, pose.radius) {
            return Hit::Miss;
        }
        if self.tucked {
            return Hit::Peek;
        }
        if self.compact < 0.45 {
            let index = self.hover_index();
            if index >= 0 {
                return Hit::Chrome(index);
            }
            if self.settings {
                if let Some(step) = self.step_at(px, py) {
                    return Hit::Step(step);
                }
                if let Some(index) = self.segment_at(px, py) {
                    return Hit::Segment(index);
                }
                if let Some(index) = self.lang_at(px, py) {
                    return Hit::Lang(index);
                }
                if let Some(index) = self.accent_at(px, py) {
                    return Hit::Accent(index);
                }
                if let Some(index) = self.tuck_at(px, py) {
                    return Hit::Tuck(index);
                }
                if self.theme != 0 {
                    if let Some(index) = self.caption_at(px, py) {
                        return Hit::Caption(index);
                    }
                }
            }
            return Hit::Drag;
        }
        Hit::Pill
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) -> bool {
        match self.hit(x, y) {
            Hit::Chrome(0) => {
                self.press = Some((x, y, Press::Settings));
                false
            }
            Hit::Chrome(1) => {
                self.press = Some((x, y, Press::Minimize));
                false
            }
            Hit::Step(step) => {
                self.nudge(step);
                self.hold_delay = 0.35;
                self.press = Some((x, y, Press::Step(step)));
                true
            }
            Hit::Segment(index) => {
                if self.theme != index {
                    self.theme = index;
                    self.schedule_dirty = true;
                }
                false
            }
            Hit::Lang(index) => {
                if self.lang != index {
                    self.lang = index;
                    self.schedule_dirty = true;
                    let _ = self.rebuild_formats();
                    self.apply_title();
                }
                false
            }
            Hit::Accent(index) => {
                if self.accent != index {
                    self.accent = index;
                    self.schedule_dirty = true;
                }
                false
            }
            Hit::Tuck(index) => {
                let enabled = index == 0;
                if self.tuck != enabled {
                    self.tuck = enabled;
                    self.schedule_dirty = true;
                }
                false
            }
            Hit::Caption(index) => {
                let mode = if index == 0 { 0 } else { 1 };
                if self.caption != mode {
                    self.caption = mode;
                    self.schedule_dirty = true;
                }
                false
            }
            Hit::Pill => {
                self.press = Some((x, y, Press::Pill));
                true
            }
            Hit::Peek => {
                self.press = Some((x, y, Press::Peek));
                true
            }
            _ => false,
        }
    }

    /// True when a press on the pill or the peek has moved far enough to drag.
    pub fn mouse_dragged(&mut self, x: i32, y: i32) -> bool {
        let Some((px, py, kind)) = self.press else { return false };
        if !matches!(kind, Press::Pill | Press::Peek) {
            return false;
        }
        if (x - px).abs() + (y - py).abs() > 8 {
            self.press = None;
            true
        } else {
            false
        }
    }

    pub fn mouse_up(&mut self, x: i32, y: i32) {
        let Some((px, py, kind)) = self.press.take() else { return };
        let moved = (x - px).abs() + (y - py).abs() > 8;
        match kind {
            Press::Pill => {
                if !moved {
                    self.compact_target = false;
                }
            }
            Press::Minimize => {
                if self.hit(x, y) == Hit::Chrome(1) {
                    self.compact_target = true;
                }
            }
            Press::Settings => {
                if self.hit(x, y) == Hit::Chrome(0) {
                    self.settings = !self.settings;
                    if self.settings {
                        self.compact_target = false;
                    }
                }
            }
            Press::Step(_) => {}
            Press::Peek => {
                if !moved {
                    self.reveal_dock();
                }
            }
        }
    }

    /// The progress track on a shader: a blurred strip of the backdrop, clipped
    /// to the capsule. `scene` is that strip in the scene bitmap's DIP space
    /// and must be the same size as `screen`.
    fn paint_frost(&self, screen: D2D_RECT_F, scene: D2D_RECT_F, opacity: f32) -> Result<()> {
        if opacity <= 0.02 {
            return Ok(());
        }
        let factory = unsafe { self.d2d.GetFactory() }?;
        let capsule = D2D1_ROUNDED_RECT { rect: screen, radiusX: 2.0, radiusY: 2.0 };
        let mask: ID2D1Geometry = unsafe { factory.CreateRoundedRectangleGeometry(&capsule)?.cast() }?;
        let layer = D2D1_LAYER_PARAMETERS1 {
            contentBounds: d2d_rect(screen.left - 1.0, screen.top - 1.0, screen.right + 1.0, screen.bottom + 1.0),
            geometricMask: core::mem::ManuallyDrop::new(Some(mask)),
            maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
            maskTransform: Matrix3x2::identity(),
            opacity,
            opacityBrush: core::mem::ManuallyDrop::new(None),
            layerOptions: D2D1_LAYER_OPTIONS1_NONE,
        };
        let layer_ref = self.frost_layer.clone();
        unsafe {
            self.d2d.PushLayer(&layer, &layer_ref);
            let output: ID2D1Image = self.frost.cast()?;
            self.d2d.DrawImage(
                &output,
                Some(&Vector2 { X: screen.left, Y: screen.top }),
                Some(&scene),
                D2D1_INTERPOLATION_MODE_LINEAR,
                D2D1_COMPOSITE_MODE_SOURCE_OVER,
            );
            self.d2d.PopLayer();
            let _ = core::mem::ManuallyDrop::into_inner(layer.geometricMask);
            let _ = core::mem::ManuallyDrop::into_inner(layer.opacityBrush);
        }
        Ok(())
    }

    fn draw_card(&mut self, frame: &Frame) -> Result<()> {
        let pose = self.pose();
        let key = pose_key(pose);
        if key != self.mask_key {
            let factory = unsafe { self.d2d.GetFactory() }?;
            let mask = D2D1_ROUNDED_RECT {
                rect: d2d_rect(pose.x, pose.y, pose.x + pose.w, pose.y + pose.h),
                radiusX: pose.radius,
                radiusY: pose.radius,
            };
            self.clip = unsafe { factory.CreateRoundedRectangleGeometry(&mask)?.cast() }?;
            self.mask_key = key;
        }
        let (ox, oy) = (pose.x, pose.y);
        let card_w = self.card_dip_w;
        // Countdown copy stays on the short card. The extra height is the
        // settings row, so the footer does not drop when the card grows.
        let card_h = CARD_HEIGHT_DIP;
        let ease = pose.ease;
        // The footer and the corner buttons leave in the first part of the fold,
        // before the card edge clips them.
        let body = (1.0 - ease * 1.45).clamp(0.0, 1.0);
        unsafe {
            self.brush_faded.SetColor(&self.secondary(body));
            self.brush_accent.SetColor(&rgba(0.23, 0.51, 0.96, body));
            self.brush_track.SetColor(&mix_color(TRACK, (0.15, 0.20, 0.27), self.sea, body));
            self.brush_hover.SetColor(&rgba(0.0, 0.0, 0.0, lerp(0.06, 0.22, self.sea) * body));

            self.d2d.BeginDraw();
            let clear = rgba(0.0, 0.0, 0.0, 0.0);
            self.d2d.Clear(Some(&clear));

            let window = d2d_rect(0.0, 0.0, self.origin_dip * 2.0 + card_w, self.origin_dip * 2.0 + self.card_dip_h);
            self.d2d.DrawBitmap(
                &self.shadow,
                Some(&window),
                1.0,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None,
                None,
            );

            let mut layer = D2D1_LAYER_PARAMETERS1 {
                contentBounds: d2d_rect(pose.x - 1.0, pose.y - 1.0, pose.x + pose.w + 1.0, pose.y + pose.h + 1.0),
                geometricMask: core::mem::ManuallyDrop::new(Some(self.clip.clone())),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: Matrix3x2::identity(),
                opacity: 1.0,
                opacityBrush: core::mem::ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            let layer_ref = self.layer.clone();
            self.d2d.PushLayer(&layer, &layer_ref);
            // Show a 1:1 window of the scene. Expanded, that window is the whole
            // card; collapsed, it sits on the middle of the ocean.
            let open = self.open_height();
            let backdrop = d2d_rect(pose.x, pose.y, pose.x + pose.w, pose.y + pose.h);
            if self.sea > 0.02 {
                let src_w = pose.w.min(card_w);
                let src_h = pose.h.min(open);
                let src_x = (card_w - src_w) * 0.5;
                let src_y = (open - src_h) * 0.5;
                let source = d2d_rect(src_x, src_y, src_x + src_w, src_y + src_h);
                self.d2d.DrawBitmap(
                    &self.scene_bitmap,
                    Some(&backdrop),
                    self.sea,
                    D2D1_INTERPOLATION_MODE_LINEAR,
                    Some(&source),
                    None,
                );
            }
            if self.sea < 0.98 {
                self.paper.SetStartPoint(Vector2 { X: pose.x, Y: pose.y });
                self.paper.SetEndPoint(Vector2 { X: pose.x + pose.w, Y: pose.y + pose.h });
                ID2D1Brush::SetOpacity(&self.paper, 1.0 - self.sea);
                self.d2d.FillRectangle(&backdrop, &self.paper);
            }

            // Settings and the countdown trade places: each fades, and the one
            // leaving slides up while the one arriving rises into the same spot.
            let e = ease;
            let s = self.settings_t;
            let d = smooth(self.done_t);
            let travel = 12.0 * (1.0 - e);
            let page = s * (1.0 - e);
            let copy = self.copy_t;
            let chrome = body * lerp(copy, 1.0, s);
            let clock_fade = (1.0 - page) * copy;
            let clock_dy = -travel * s;
            let settings_dy = travel * (1.0 - s);
            if page > 0.02 {
                self.draw_settings(ox, oy + settings_dy, card_w, page)?;
            }
            let label_x = ox + PAD_X_DIP + ICON_DIP + 6.0;
            let farewell = 8.0 * (1.0 - e);
            if clock_fade > 0.02 {
            let icon_x = ox + PAD_X_DIP;
            let icon_y = lerp(
                oy + PAD_Y_DIP + (LABEL_H_DIP - ICON_DIP) * 0.5,
                oy + (COMPACT_H_DIP - ICON_DIP) * 0.5,
                e,
            ) + clock_dy;
            let clock_icon = clock_fade * (1.0 - d);
            if clock_icon > 0.02 {
                self.brush_muted.SetColor(&self.secondary(clock_icon));
                draw_lucide_clock(&self.d2d, &self.brush_muted, &self.stroke, icon_x, icon_y, ICON_DIP);
            }
            let check_icon = clock_fade * d;
            if check_icon > 0.02 {
                let c = reminder_rgb(self.accent);
                self.brush_muted.SetColor(&rgba(c.0, c.1, c.2, check_icon));
                draw_check_icon(&self.d2d, &self.brush_muted, &self.stroke, icon_x, icon_y, ICON_DIP);
            }
            let label_y = lerp(oy + PAD_Y_DIP, oy + (COMPACT_H_DIP - LABEL_H_DIP) * 0.5, e);
            let work_label = clock_fade * (1.0 - d);
            if work_label > 0.02 {
                self.brush_muted.SetColor(&self.secondary(work_label));
                draw_text(
                    &self.d2d, &self.text_label, &self.brush_muted, &frame.label,
                    label_x, label_y + clock_dy - farewell * d, ox + card_w - PAD_X_DIP, label_y + LABEL_H_DIP + clock_dy - farewell * d,
                )?;
            }
            let off_label = clock_fade * d;
            if off_label > 0.02 {
                let label_top = label_y + clock_dy + farewell * (1.0 - d);
                let format = self.text_label.clone();
                self.paint_reminder(
                    &format, &frame.done_label,
                    label_x, label_top, card_w - (label_x - ox), LABEL_H_DIP, 13.0, off_label,
                )?;
            }
            }

            let hover_index = self.hover_index();
            if chrome > 0.02 {
                for (index, (bx, by)) in chrome_slots(card_w).into_iter().enumerate() {
                    let (bx, by) = (ox + bx, oy + by);
                    if hover_index == index as i32 {
                        let chip = D2D1_ROUNDED_RECT {
                            rect: d2d_rect(bx, by, bx + CHROME_BOX_DIP, by + CHROME_BOX_DIP),
                            radiusX: CHROME_RADIUS_DIP,
                            radiusY: CHROME_RADIUS_DIP,
                        };
                        self.d2d.FillRoundedRectangle(&chip, &self.brush_hover);
                    }
                    let inset = (CHROME_BOX_DIP - ICON_DIP) * 0.5;
                    if index == 0 {
                        if s < 0.98 {
                            self.brush_faded.SetColor(&self.secondary(chrome * (1.0 - s)));
                            draw_chrome_icon(
                                &self.d2d, &self.brush_faded, &self.stroke, &self.gear,
                                index, false, bx + inset, by + inset, ICON_DIP,
                            );
                        }
                        if s > 0.02 {
                            self.brush_faded.SetColor(&self.secondary(chrome * s));
                            draw_chrome_icon(
                                &self.d2d, &self.brush_faded, &self.stroke, &self.gear,
                                index, true, bx + inset, by + inset, ICON_DIP,
                            );
                        }
                    } else {
                        self.brush_faded.SetColor(&self.secondary(chrome));
                        draw_chrome_icon(
                            &self.d2d, &self.brush_faded, &self.stroke, &self.gear,
                            index, false, bx + inset, by + inset, ICON_DIP,
                        );
                    }
                }
            }

            if clock_fade > 0.02 {
            let footer_h = 16.0;
            let bar_h = 4.0;
            let bar_gap = 8.0;
            let footer_bottom = oy + card_h - PAD_Y_DIP + clock_dy;
            let footer_top = footer_bottom - footer_h;
            let bar_y = footer_top - bar_gap - bar_h;
            let time_top_exp = oy + PAD_Y_DIP + LABEL_H_DIP + 2.0;
            let time_h_exp = (bar_y - clock_dy - 6.0) - time_top_exp;
            let time_x = lerp(ox + PAD_X_DIP, label_x + LABEL_BLOCK_DIP, e);
            let time_y = lerp(time_top_exp, oy, e);
            let time_h = lerp(time_h_exp, COMPACT_H_DIP, e);
            let time_size = lerp(TIME_EXPANDED_DIP, TIME_COMPACT_DIP, e);
            let count_fade = clock_fade * (1.0 - d);
            if count_fade > 0.02 {
                self.brush_white.SetColor(&self.primary(count_fade));
                draw_text_sized(
                    &self.d2d, &self.dwrite, &self.text_time, &self.brush_white, &frame.time_text,
                    time_x, time_y + clock_dy - farewell * d, card_w, time_h, time_size,
                )?;
            }
            let greet_fade = clock_fade * d;
            let phrase = greet_fade * (1.0 - e);
            if phrase > 0.02 {
                let format = self.text_time.clone();
                self.paint_reminder(
                    &format, &frame.farewell,
                    time_x, time_y + clock_dy + farewell * (1.0 - d), card_w, time_h, 26.0, phrase,
                )?;
            }
            let see_you = greet_fade * e;
            if see_you > 0.02 {
                let format = self.text_time.clone();
                self.paint_reminder(
                    &format, &frame.bye,
                    time_x, time_y + clock_dy + farewell * (1.0 - d), card_w, time_h, TIME_COMPACT_DIP, see_you,
                )?;
            }

            if body > 0.02 {
                let bar_w = card_w - PAD_X_DIP * 2.0;
                let track_rect = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(ox + PAD_X_DIP, bar_y, ox + PAD_X_DIP + bar_w, bar_y + bar_h),
                    radiusX: 2.0,
                    radiusY: 2.0,
                };
                let fade = body * clock_fade;
                if self.sea > 0.02 && fade > 0.02 {
                    let open = self.open_height();
                    let src_w = pose.w.min(card_w);
                    let src_h = pose.h.min(open);
                    let src_x = (card_w - src_w) * 0.5;
                    let src_y = (open - src_h) * 0.5;
                    let left = ox + PAD_X_DIP;
                    let u0 = (left - pose.x) / pose.w.max(1.0);
                    let u1 = (left + bar_w - pose.x) / pose.w.max(1.0);
                    let v0 = (bar_y - pose.y) / pose.h.max(1.0);
                    let v1 = (bar_y + bar_h - pose.y) / pose.h.max(1.0);
                    let scene = d2d_rect(
                        src_x + u0 * src_w, src_y + v0 * src_h,
                        src_x + u1 * src_w, src_y + v1 * src_h,
                    );
                    if (scene.right - scene.left - bar_w).abs() < 1.5
                        && (scene.bottom - scene.top - bar_h).abs() < 1.5
                    {
                        self.paint_frost(track_rect.rect, scene, self.sea * fade)?;
                    }
                }
                // Paper keeps the solid track. On a shader the same dark colour
                // is only a veil, so the blurred backdrop shows through.
                self.brush_track.SetColor(&mix_color(
                    TRACK, (0.15, 0.20, 0.27), self.sea, lerp(1.0, 0.34, self.sea) * fade,
                ));
                self.d2d.FillRoundedRectangle(&track_rect, &self.brush_track);
                let filled = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(
                        ox + PAD_X_DIP, bar_y,
                        ox + PAD_X_DIP + bar_w * frame.progress.clamp(0.0, 1.0),
                        bar_y + bar_h,
                    ),
                    radiusX: 2.0,
                    radiusY: 2.0,
                };
                self.brush_accent.SetColor(&mix_color(BLUE, reminder_rgb(self.accent), d, body * clock_fade));
                self.d2d.FillRoundedRectangle(&filled, &self.brush_accent);
                let foot = body * clock_fade;
                let today = foot * (1.0 - d);
                if today > 0.02 {
                    self.brush_faded.SetColor(&self.secondary(today));
                    draw_text(&self.d2d, &self.text_footer, &self.brush_faded, &frame.footer_left, ox + PAD_X_DIP, footer_top, ox + card_w - PAD_X_DIP, footer_bottom)?;
                }
                let tomorrow = foot * d;
                if tomorrow > 0.02 {
                    self.brush_faded.SetColor(&self.secondary(tomorrow));
                    draw_text(&self.d2d, &self.text_footer, &self.brush_faded, &frame.footer_done, ox + PAD_X_DIP, footer_top, ox + card_w - PAD_X_DIP, footer_bottom)?;
                }
                self.brush_faded.SetColor(&self.secondary(foot));
                draw_text(&self.d2d, &self.text_footer_right, &self.brush_faded, &frame.footer_right, ox + PAD_X_DIP, footer_top, ox + card_w - PAD_X_DIP, footer_bottom)?;
            }
            }

            self.d2d.PopLayer();
            core::mem::ManuallyDrop::drop(&mut layer.geometricMask);
            core::mem::ManuallyDrop::drop(&mut layer.opacityBrush);
            self.d2d.EndDraw(None, None)?;
        }
        Ok(())
    }

    fn draw_settings(&mut self, ox: f32, oy: f32, card_w: f32, fade: f32) -> Result<()> {
        let drawn = self.paint_settings(ox, oy, card_w, fade);
        unsafe {
            self.text_time.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            self.brush_white.SetColor(&rgba(1.0, 1.0, 1.0, 1.0));
            self.brush_faded.SetColor(&rgba(0.84, 0.88, 0.93, fade));
            self.brush_hover.SetColor(&rgba(0.0, 0.0, 0.0, 0.22 * fade));
        }
        drawn
    }

    /// Off-work phrase. Bamboo, sakura and sky are flat. Gilt is a slow silk.
    fn paint_reminder(
        &mut self,
        format: &IDWriteTextFormat,
        text: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        size: f32,
        alpha: f32,
    ) -> Result<()> {
        let wide: Vec<u16> = text.encode_utf16().collect();
        if wide.is_empty() || w <= 1.0 || h <= 1.0 || alpha < 0.02 {
            return Ok(());
        }
        let accent = self.accent;
        let layout = unsafe {
            let layout = self.dwrite.CreateTextLayout(&wide, format, w, h)?;
            layout.SetFontSize(size, DWRITE_TEXT_RANGE { startPosition: 0, length: wide.len() as u32 })?;
            layout
        };
        unsafe {
            match accent {
                3 => {
                    self.refresh_gold()?;
                    ID2D1Brush::SetOpacity(&self.gold_brush, alpha);
                    self.d2d.DrawTextLayout(Vector2 { X: x, Y: y }, &layout, &self.gold_brush, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
                _ => {
                    let c = reminder_rgb(accent);
                    self.brush_white.SetColor(&rgba(c.0, c.1, c.2, alpha));
                    self.d2d.DrawTextLayout(Vector2 { X: x, Y: y }, &layout, &self.brush_white, D2D1_DRAW_TEXT_OPTIONS_NONE);
                }
            }
        }
        Ok(())
    }

    /// Gray chip beside the off-work label when that time is the next day.
    fn paint_next_day(&mut self, x: f32, y: f32, fade: f32, label: &str) -> Result<()> {
        let w = label.chars().count() as f32 * 11.0 + 12.0;
        let h = 16.0;
        unsafe {
            self.brush_track.SetColor(&rgba(
                lerp(0.86, 0.05, self.sea),
                lerp(0.89, 0.07, self.sea),
                lerp(0.93, 0.10, self.sea),
                lerp(1.0, 0.42, self.sea) * fade,
            ));
            let chip = D2D1_ROUNDED_RECT {
                rect: d2d_rect(x, y, x + w, y + h),
                radiusX: 4.0,
                radiusY: 4.0,
            };
            self.d2d.FillRoundedRectangle(&chip, &self.brush_track);
            self.brush_faded.SetColor(&self.secondary(0.85 * fade));
        }
        draw_text_sized(
            &self.d2d, &self.dwrite, &self.text_segment, &self.brush_faded, label,
            x, y, w, h, 11.0,
        )
    }

    fn paint_settings(&mut self, ox: f32, oy: f32, card_w: f32, fade: f32) -> Result<()> {
        let copy = phrases(self.lang);
        unsafe {
            self.brush_white.SetColor(&self.primary(fade));
            self.brush_faded.SetColor(&self.secondary(fade));
            self.text_time.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
        }
        draw_text(
            &self.d2d, &self.text_label, &self.brush_faded, copy.settings,
            ox + PAD_X_DIP, oy + PAD_Y_DIP, ox + card_w * 0.55, oy + PAD_Y_DIP + LABEL_H_DIP,
        )?;
        let hover = self.current_step();
        for row in 0..2u8 {
            let mins = if row == 0 { self.start_min } else { self.end_min };
            let row_y = oy + settings_row_y(row);
            let label = if row == 0 { copy.start } else { copy.end };
            draw_text(
                &self.d2d, &self.text_label, &self.brush_faded, label,
                ox + PAD_X_DIP, row_y + 9.0, ox + settings_block_x(card_w) - 4.0, row_y + STEP_ROW - 6.0,
            )?;
            if row == 1 && self.end_min <= self.start_min {
                let text_w: f32 = label.chars().map(|c| if c.is_ascii() { 7.0 } else { 13.0 }).sum();
                self.paint_next_day(ox + PAD_X_DIP + text_w + 6.0, row_y + 11.0, fade, copy.next_day)?;
            }
            let hour = format!("{:02}", mins.div_euclid(60));
            let minute = format!("{:02}", mins.rem_euclid(60));
            draw_text_sized(
                &self.d2d, &self.dwrite, &self.text_time, &self.brush_white, &hour,
                ox + field_num_x(card_w, 0), row_y, FIELD_NUM_W, STEP_ROW, 20.0,
            )?;
            draw_text_sized(
                &self.d2d, &self.dwrite, &self.text_time, &self.brush_faded, ":",
                ox + colon_x(card_w), row_y, COLON_W, STEP_ROW, 18.0,
            )?;
            draw_text_sized(
                &self.d2d, &self.dwrite, &self.text_time, &self.brush_white, &minute,
                ox + field_num_x(card_w, 1), row_y, FIELD_NUM_W, STEP_ROW, 20.0,
            )?;
        }
        for slot in step_slots(card_w) {
            let x = ox + slot.x;
            let y = oy + slot.y;
            let hot = hover == Some(slot.step);
            unsafe {
                let ocean_a = if hot { 0.34 } else { 0.16 } * fade;
                let paper = if hot { (0.90, 0.93, 0.96) } else { CHIP };
                self.brush_hover.SetColor(&rgba(
                    lerp(paper.0, 0.0, self.sea),
                    lerp(paper.1, 0.0, self.sea),
                    lerp(paper.2, 0.0, self.sea),
                    lerp(fade, ocean_a, self.sea),
                ));
                let chip = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(x, y, x + STEP_BTN, y + STEP_BTN),
                    radiusX: 7.0,
                    radiusY: 7.0,
                };
                self.d2d.FillRoundedRectangle(&chip, &self.brush_hover);
            }
            draw_step_glyph(&self.d2d, &self.brush_white, &self.stroke, x, y, STEP_BTN, slot.step.delta);
        }
        let row_y = oy + settings_row_y(2);
        if background_inline() {
            draw_text(
                &self.d2d, &self.text_label, &self.brush_faded, copy.backdrop,
                ox + PAD_X_DIP, row_y + 9.0, ox + settings_block_x(card_w) - 4.0, row_y + STEP_ROW - 6.0,
            )?;
        } else {
            draw_text(
                &self.d2d, &self.text_label, &self.brush_faded, copy.backdrop,
                ox + PAD_X_DIP, row_y, ox + card_w - PAD_X_DIP, row_y + LABEL_H_DIP,
            )?;
        }
        self.paint_segment(ox, oy, card_w, fade, &copy.themes)?;
        let lang_y = oy + lang_label_y();
        unsafe { self.brush_faded.SetColor(&self.secondary(fade)); }
        draw_text(
            &self.d2d, &self.text_label, &self.brush_faded, copy.language,
            ox + PAD_X_DIP, lang_y, ox + card_w - PAD_X_DIP, lang_y + LABEL_H_DIP,
        )?;
        self.paint_lang(ox, oy, card_w, fade)?;
        let accent_y = oy + accent_label_y();
        unsafe { self.brush_faded.SetColor(&self.secondary(fade)); }
        draw_text(
            &self.d2d, &self.text_label, &self.brush_faded, copy.accent,
            ox + PAD_X_DIP, accent_y, ox + card_w - PAD_X_DIP, accent_y + LABEL_H_DIP,
        )?;
        self.paint_accent(ox, oy, card_w, fade, &copy.accents)?;
        let tuck_y = oy + tuck_label_y();
        unsafe { self.brush_faded.SetColor(&self.secondary(fade)); }
        draw_text(
            &self.d2d, &self.text_label, &self.brush_faded, copy.tuck,
            ox + PAD_X_DIP, tuck_y, ox + card_w - PAD_X_DIP, tuck_y + LABEL_H_DIP,
        )?;
        self.paint_pair(ox, oy, tuck_box(card_w), self.tuck_pill, fade, &copy.tucks)?;
        if self.theme != 0 {
            let caption_y = oy + caption_label_y();
            unsafe { self.brush_faded.SetColor(&self.secondary(fade)); }
            draw_text(
                &self.d2d, &self.text_label, &self.brush_faded, copy.caption,
                ox + PAD_X_DIP, caption_y, ox + card_w - PAD_X_DIP, caption_y + LABEL_H_DIP,
            )?;
            self.paint_pair(ox, oy, caption_box(card_w), self.caption_pill, fade, &copy.captions)?;
        }
        Ok(())
    }

    fn paint_pair(
        &mut self,
        ox: f32,
        oy: f32,
        slot: (f32, f32, f32, f32),
        pill_at: f32,
        fade: f32,
        labels: &[&str; 2],
    ) -> Result<()> {
        let (sx, sy, sw, sh) = slot;
        let x = ox + sx;
        let y = oy + sy;
        let sea = self.sea;
        let count = 2.0;
        let pill_w = (sw - SEG_INSET * 2.0) / count;
        unsafe {
            self.brush_track.SetColor(&rgba(
                lerp(TRACK.0, 1.0, sea),
                lerp(TRACK.1, 1.0, sea),
                lerp(TRACK.2, 1.0, sea),
                lerp(fade, 0.16 * fade, sea),
            ));
            let track = D2D1_ROUNDED_RECT {
                rect: d2d_rect(x, y, x + sw, y + sh),
                radiusX: 10.0,
                radiusY: 10.0,
            };
            self.d2d.FillRoundedRectangle(&track, &self.brush_track);
            self.brush_faded.SetColor(&rgba(225.0 / 255.0, 231.0 / 255.0, 239.0 / 255.0, (1.0 - sea) * fade));
            self.d2d.DrawRoundedRectangle(&track, &self.brush_faded, 1.0, &self.stroke);
            let pill_h = sh - SEG_INSET * 2.0;
            let pill_x = x + SEG_INSET + pill_w * pill_at;
            let pill_y = y + SEG_INSET;
            let pill = D2D1_ROUNDED_RECT {
                rect: d2d_rect(pill_x, pill_y, pill_x + pill_w, pill_y + pill_h),
                radiusX: 8.0,
                radiusY: 8.0,
            };
            self.brush_white.SetColor(&rgba(1.0, 1.0, 1.0, fade));
            self.d2d.FillRoundedRectangle(&pill, &self.brush_white);
            self.brush_faded.SetColor(&rgba(INK.0, INK.1, INK.2, lerp(0.08, 0.0, sea) * fade));
            self.d2d.DrawRoundedRectangle(&pill, &self.brush_faded, 1.0, &self.stroke);
        }
        let cell = sw / count;
        for (index, label) in labels.iter().enumerate() {
            let sel = (1.0 - (pill_at - index as f32).abs()).clamp(0.0, 1.0);
            let idle = mix_color(INK_SOFT, WHITE, sea, fade);
            unsafe {
                self.brush_white.SetColor(&rgba(
                    lerp(idle.r, INK.0, sel),
                    lerp(idle.g, INK.1, sel),
                    lerp(idle.b, INK.2, sel),
                    fade,
                ));
            }
            draw_text(
                &self.d2d, &self.text_segment, &self.brush_white, label,
                x + cell * index as f32, y, x + cell * (index as f32 + 1.0), y + sh,
            )?;
        }
        Ok(())
    }

    fn paint_segment(&mut self, ox: f32, oy: f32, card_w: f32, fade: f32, labels: &[&str; 4]) -> Result<()> {
        let (sx, sy, sw, sh) = segment_box(card_w);
        let x = ox + sx;
        let y = oy + sy;
        let sea = self.sea;
        let count = THEME_COUNT;
        let pill_w = (sw - SEG_INSET * 2.0) / count;
        unsafe {
            self.brush_track.SetColor(&rgba(
                lerp(TRACK.0, 1.0, sea),
                lerp(TRACK.1, 1.0, sea),
                lerp(TRACK.2, 1.0, sea),
                lerp(fade, 0.16 * fade, sea),
            ));
            let track = D2D1_ROUNDED_RECT {
                rect: d2d_rect(x, y, x + sw, y + sh),
                radiusX: 10.0,
                radiusY: 10.0,
            };
            self.d2d.FillRoundedRectangle(&track, &self.brush_track);
            self.brush_faded.SetColor(&rgba(225.0 / 255.0, 231.0 / 255.0, 239.0 / 255.0, (1.0 - sea) * fade));
            self.d2d.DrawRoundedRectangle(&track, &self.brush_faded, 1.0, &self.stroke);
            let pill_h = sh - SEG_INSET * 2.0;
            let pill_x = x + SEG_INSET + pill_w * self.pill;
            let pill_y = y + SEG_INSET;
            let pill = D2D1_ROUNDED_RECT {
                rect: d2d_rect(pill_x, pill_y, pill_x + pill_w, pill_y + pill_h),
                radiusX: 8.0,
                radiusY: 8.0,
            };
            self.brush_white.SetColor(&rgba(1.0, 1.0, 1.0, fade));
            self.d2d.FillRoundedRectangle(&pill, &self.brush_white);
            self.brush_faded.SetColor(&rgba(INK.0, INK.1, INK.2, lerp(0.08, 0.0, sea) * fade));
            self.d2d.DrawRoundedRectangle(&pill, &self.brush_faded, 1.0, &self.stroke);
        }
        let cell = sw / count;
        for (index, label) in labels.iter().enumerate() {
            let sel = (1.0 - (self.pill - index as f32).abs()).clamp(0.0, 1.0);
            let idle = mix_color(INK_SOFT, WHITE, sea, fade);
            unsafe {
                self.brush_white.SetColor(&rgba(
                    lerp(idle.r, INK.0, sel),
                    lerp(idle.g, INK.1, sel),
                    lerp(idle.b, INK.2, sel),
                    fade,
                ));
            }
            draw_text(
                &self.d2d, &self.text_segment, &self.brush_white, label,
                x + cell * index as f32, y, x + cell * (index as f32 + 1.0), y + sh,
            )?;
        }
        Ok(())
    }

    fn paint_lang(&mut self, ox: f32, oy: f32, card_w: f32, fade: f32) -> Result<()> {
        let (sx, sy, sw, sh) = lang_box(card_w);
        let x = ox + sx;
        let y = oy + sy;
        let sea = self.sea;
        let count = 4.0;
        let pill_w = (sw - SEG_INSET * 2.0) / count;
        unsafe {
            self.brush_track.SetColor(&rgba(
                lerp(TRACK.0, 1.0, sea),
                lerp(TRACK.1, 1.0, sea),
                lerp(TRACK.2, 1.0, sea),
                lerp(fade, 0.16 * fade, sea),
            ));
            let track = D2D1_ROUNDED_RECT {
                rect: d2d_rect(x, y, x + sw, y + sh),
                radiusX: 10.0,
                radiusY: 10.0,
            };
            self.d2d.FillRoundedRectangle(&track, &self.brush_track);
            self.brush_faded.SetColor(&rgba(225.0 / 255.0, 231.0 / 255.0, 239.0 / 255.0, (1.0 - sea) * fade));
            self.d2d.DrawRoundedRectangle(&track, &self.brush_faded, 1.0, &self.stroke);
            let pill_h = sh - SEG_INSET * 2.0;
            let pill_x = x + SEG_INSET + pill_w * self.lang_pill;
            let pill_y = y + SEG_INSET;
            let pill = D2D1_ROUNDED_RECT {
                rect: d2d_rect(pill_x, pill_y, pill_x + pill_w, pill_y + pill_h),
                radiusX: 8.0,
                radiusY: 8.0,
            };
            self.brush_white.SetColor(&rgba(1.0, 1.0, 1.0, fade));
            self.d2d.FillRoundedRectangle(&pill, &self.brush_white);
            self.brush_faded.SetColor(&rgba(INK.0, INK.1, INK.2, lerp(0.08, 0.0, sea) * fade));
            self.d2d.DrawRoundedRectangle(&pill, &self.brush_faded, 1.0, &self.stroke);
        }
        let cell = sw / count;
        for (index, label) in LANG_NAMES.iter().enumerate() {
            let sel = (1.0 - (self.lang_pill - index as f32).abs()).clamp(0.0, 1.0);
            let idle = mix_color(INK_SOFT, WHITE, sea, fade);
            unsafe {
                self.brush_white.SetColor(&rgba(
                    lerp(idle.r, INK.0, sel),
                    lerp(idle.g, INK.1, sel),
                    lerp(idle.b, INK.2, sel),
                    fade,
                ));
            }
            draw_text(
                &self.d2d, &self.lang_fonts[index], &self.brush_white, label,
                x + cell * index as f32, y, x + cell * (index as f32 + 1.0), y + sh,
            )?;
        }
        Ok(())
    }

    fn paint_accent(&mut self, ox: f32, oy: f32, card_w: f32, fade: f32, labels: &[&str; 4]) -> Result<()> {
        let (sx, sy, sw, sh) = accent_box(card_w);
        let x = ox + sx;
        let y = oy + sy;
        let sea = self.sea;
        let count = 4.0;
        let pill_w = (sw - SEG_INSET * 2.0) / count;
        unsafe {
            self.brush_track.SetColor(&rgba(
                lerp(TRACK.0, 1.0, sea),
                lerp(TRACK.1, 1.0, sea),
                lerp(TRACK.2, 1.0, sea),
                lerp(fade, 0.16 * fade, sea),
            ));
            let track = D2D1_ROUNDED_RECT {
                rect: d2d_rect(x, y, x + sw, y + sh),
                radiusX: 10.0,
                radiusY: 10.0,
            };
            self.d2d.FillRoundedRectangle(&track, &self.brush_track);
            self.brush_faded.SetColor(&rgba(225.0 / 255.0, 231.0 / 255.0, 239.0 / 255.0, (1.0 - sea) * fade));
            self.d2d.DrawRoundedRectangle(&track, &self.brush_faded, 1.0, &self.stroke);
            let pill_h = sh - SEG_INSET * 2.0;
            let pill_x = x + SEG_INSET + pill_w * self.accent_pill;
            let pill_y = y + SEG_INSET;
            let pill = D2D1_ROUNDED_RECT {
                rect: d2d_rect(pill_x, pill_y, pill_x + pill_w, pill_y + pill_h),
                radiusX: 8.0,
                radiusY: 8.0,
            };
            self.brush_white.SetColor(&rgba(1.0, 1.0, 1.0, fade));
            self.d2d.FillRoundedRectangle(&pill, &self.brush_white);
            self.brush_faded.SetColor(&rgba(INK.0, INK.1, INK.2, lerp(0.08, 0.0, sea) * fade));
            self.d2d.DrawRoundedRectangle(&pill, &self.brush_faded, 1.0, &self.stroke);
        }
        let cell = sw / count;
        for (index, label) in labels.iter().enumerate() {
            let sel = (1.0 - (self.accent_pill - index as f32).abs()).clamp(0.0, 1.0);
            let ink = accent_label_rgb(index as u8, sel > 0.5);
            unsafe {
                self.brush_white.SetColor(&rgba(ink.0, ink.1, ink.2, fade));
            }
            draw_text(
                &self.d2d, &self.text_accent, &self.brush_white, label,
                x + cell * index as f32, y, x + cell * (index as f32 + 1.0), y + sh,
            )?;
        }
        Ok(())
    }

    fn repeat_step(&mut self, dt: f32) {
        let Some(step) = self.held_step() else { return };
        self.hold_delay -= dt;
        if self.hold_delay > 0.0 {
            return;
        }
        self.hold_delay = 0.08;
        if self.current_step() == Some(step) {
            self.nudge(step);
        }
    }

    fn held_step(&self) -> Option<Step> {
        match self.press {
            Some((_, _, Press::Step(step))) => Some(step),
            _ => None,
        }
    }

    fn nudge(&mut self, step: Step) {
        let slot = if step.row == 0 { &mut self.start_min } else { &mut self.end_min };
        *slot = nudge_minutes(*slot, step.field, step.delta as i32);
        self.schedule_dirty = true;
    }

    fn current_step(&self) -> Option<Step> {
        if !self.settings || self.tucked || self.compact >= 0.45 {
            return None;
        }
        let (x, y) = self.cursor_client()?;
        let (px, py) = self.dip(x, y);
        self.step_at(px, py)
    }

    fn segment_at(&self, px: f32, py: f32) -> Option<u8> {
        let x = px - self.origin_dip;
        let y = py - self.origin_dip;
        let (sx, sy, sw, sh) = segment_box(self.card_dip_w);
        if x >= sx && x < sx + sw && y >= sy && y < sy + sh {
            let index = ((x - sx) / sw * THEME_COUNT).floor() as u8;
            Some(index.min((THEME_COUNT as u8) - 1))
        } else {
            None
        }
    }

    fn lang_at(&self, px: f32, py: f32) -> Option<u8> {
        let x = px - self.origin_dip;
        let y = py - self.origin_dip;
        let (sx, sy, sw, sh) = lang_box(self.card_dip_w);
        if x >= sx && x < sx + sw && y >= sy && y < sy + sh {
            let index = ((x - sx) / sw * 4.0).floor() as u8;
            Some(index.min(LANG_COUNT - 1))
        } else {
            None
        }
    }

    fn pair_at(&self, px: f32, py: f32, slot: (f32, f32, f32, f32)) -> Option<u8> {
        let x = px - self.origin_dip;
        let y = py - self.origin_dip;
        let (sx, sy, sw, sh) = slot;
        if x >= sx && x < sx + sw && y >= sy && y < sy + sh {
            let index = ((x - sx) / sw * 2.0).floor() as u8;
            Some(index.min(1))
        } else {
            None
        }
    }

    fn tuck_at(&self, px: f32, py: f32) -> Option<u8> {
        self.pair_at(px, py, tuck_box(self.card_dip_w))
    }

    fn caption_at(&self, px: f32, py: f32) -> Option<u8> {
        self.pair_at(px, py, caption_box(self.card_dip_w))
    }

    fn accent_at(&self, px: f32, py: f32) -> Option<u8> {
        let x = px - self.origin_dip;
        let y = py - self.origin_dip;
        let (sx, sy, sw, sh) = accent_box(self.card_dip_w);
        if x >= sx && x < sx + sw && y >= sy && y < sy + sh {
            let index = ((x - sx) / sw * 4.0).floor() as u8;
            Some(index.min(3))
        } else {
            None
        }
    }

    fn step_at(&self, px: f32, py: f32) -> Option<Step> {
        let x = px - self.origin_dip;
        let y = py - self.origin_dip;
        step_slots(self.card_dip_w).into_iter().find(|slot| {
            x >= slot.x && x < slot.x + STEP_BTN && y >= slot.y && y < slot.y + STEP_BTN
        }).map(|slot| slot.step)
    }

    pub fn mark_layout_dirty(&mut self) {
        self.layout_dirty = true;
    }

    pub fn take_layout_dirty(&mut self) -> bool {
        let dirty = self.layout_dirty;
        self.layout_dirty = false;
        dirty
    }

    pub fn is_docked(&self) -> bool {
        self.dock.is_some()
    }

    pub fn dock_monitor(&self) -> Option<HMONITOR> {
        self.dock.as_ref().map(|dock| dock.monitor)
    }

    pub fn dock_edge_outer(&self, edges: &Edges) -> bool {
        self.dock.as_ref().map(|dock| edges.allows(dock.edge)).unwrap_or(false)
    }

    /// Work area the docked card must stay inside of, for hit testing.
    /// Empty while a drag is moving the window.
    pub fn dock_work(&self) -> Option<RECT> {
        self.dock.as_ref().filter(|dock| !dock.suspended).map(|dock| dock.work)
    }

    /// Card inset, width and height in physical pixels.
    pub fn card_px(&self) -> (i32, i32, i32) {
        let scale = if self.dpi > 0.0 { self.dpi / 96.0 } else { 1.0 };
        let pose = self.pose();
        (
            (pose.x * scale).round() as i32,
            (pose.w * scale).round() as i32,
            (pose.h * scale).round() as i32,
        )
    }

    fn dip_px(&self, dip: f32) -> i32 {
        let scale = if self.dpi > 0.0 { self.dpi / 96.0 } else { 1.0 };
        (dip * scale).round() as i32
    }

    pub fn begin_window_drag(&mut self, cursor: POINT) {
        self.moving = true;
        self.drag_cursor = cursor;
        self.drag_tucked = self.tucked;
        if let Some(dock) = self.dock.as_mut() {
            dock.suspended = true;
        }
        self.tucked = false;
    }

    pub fn is_moving(&self) -> bool {
        self.moving
    }

    pub fn end_window_drag(
        &mut self,
        cursor: POINT,
        card: RECT,
        work: RECT,
        monitor: HMONITOR,
        outer: Edges,
    ) {
        self.moving = false;
        let velocity = (cursor.x - self.drag_cursor.x, cursor.y - self.drag_cursor.y);
        let moved = velocity.0.abs() + velocity.1.abs() > self.dip_px(8.0);
        let tucked = self.drag_tucked;
        if let Some(dock) = self.dock.as_mut() {
            dock.suspended = false;
            dock.work = work;
        }
        if !moved {
            if tucked {
                self.reveal_dock();
            }
            return;
        }
        if !self.tuck {
            self.clear_dock();
            let (x, y) = dock::clamp_inside(card, work);
            if x != card.left || y != card.top {
                self.seat = Some((x, y));
            }
            return;
        }
        self.drop_card(card, work, monitor, outer);
    }

    /// Pull a card that no longer intersects any work area back to a peek.
    pub fn rescue(&mut self, card: RECT, work: RECT, monitor: HMONITOR, outer: Edges) {
        self.drag_tucked = false;
        if !self.tuck {
            self.clear_dock();
            self.seat = Some(dock::clamp_inside(card, work));
            return;
        }
        self.drop_card(card, work, monitor, outer);
    }

    fn drop_card(&mut self, card: RECT, work: RECT, monitor: HMONITOR, outer: Edges) {
        let threshold = self.dip_px(dock::THRESHOLD_DIP);
        match dock::plan(card, work, outer, threshold) {
            Some(placement) => {
                crate::log(&format!("dock {:?} corner {:?}", placement.edge, placement.corner));
                self.engage(monitor, placement, (card.left, card.top), work);
            }
            None => {
                if self.dock.is_some() {
                    crate::log("undock");
                }
                self.clear_dock();
            }
        }
    }

    fn engage(&mut self, monitor: HMONITOR, placement: Placement, from: (i32, i32), work: RECT) {
        self.dock = Some(Dock::engage(monitor, placement, from, work));
        self.tucked = false;
    }

    pub fn clear_dock(&mut self) {
        self.dock = None;
        self.tucked = false;
        self.set_dock_clip(None);
    }

    fn reveal_dock(&mut self) {
        let Some(dock) = self.dock.as_mut() else { return };
        if dock.suspended {
            return;
        }
        let from = dock.shown;
        dock.retarget(0.0, from);
    }

    /// Move the dock animation and report the card origin for this frame.
    pub fn sync_dock(&mut self, dt: f32, hover: bool, work: Option<RECT>) -> Option<(i32, i32)> {
        if self.dock.is_none() {
            self.tucked = false;
            return None;
        }
        if self.dock.as_ref().is_some_and(|dock| dock.suspended) {
            return None;
        }
        let Some(work) = work else {
            self.clear_dock();
            return None;
        };
        let (_, card_w, card_h) = self.card_px();
        let peek = self.dip_px(dock::PEEK_DIP);
        if !self.tuck {
            if let Some(dock) = self.dock.as_mut() {
                if !dock.suspended {
                    let shown = dock.shown;
                    dock.retarget(0.0, shown);
                }
            }
        }
        let dock = self.dock.as_mut().unwrap();
        dock.work = work;
        dock.tick(dt, hover, card_w, card_h, peek, self.tuck);
        let origin = dock.origin(card_w, card_h, peek);
        dock.shown = origin;
        let resting = !self.tuck && dock.resting_open() && !dock.suspended;
        let tucked = dock.is_tucked(card_w, card_h);
        self.tucked = tucked && self.tuck;
        if resting {
            self.clear_dock();
        }
        Some(origin)
    }

    /// Clip the composition visual to the part of the window inside the work
    /// area, so a tucked card does not paint over the taskbar. `None` clears it.
    pub fn set_dock_clip(&mut self, rect: Option<(f32, f32, f32, f32)>) {
        if let Err(error) = self.apply_dock_clip(rect) {
            crate::log(&format!("dock clip failed: {error}"));
        }
    }

    fn apply_dock_clip(&mut self, rect: Option<(f32, f32, f32, f32)>) -> Result<()> {
        // Drop a clip that misses the card. An empty or misplaced clip hides
        // the whole visual, and a tucked window with nothing to grab is lost.
        let rect = rect.filter(|(left, top, right, bottom)| {
            let (pad, width, height) = self.card_px();
            let (card_l, card_t) = (pad as f32, pad as f32);
            let (card_r, card_b) = (card_l + width as f32, card_t + height as f32);
            *right > *left && *bottom > *top && *right > card_l && *left < card_r && *bottom > card_t && *top < card_b
        });
        let key = rect.map(|(left, top, right, bottom)| {
            (left.round() as i32, top.round() as i32, right.round() as i32, bottom.round() as i32)
        });
        if key == self.clip_key {
            return Ok(());
        }
        self.clip_key = key;
        unsafe {
            match rect {
                Some((left, top, right, bottom)) => {
                    let clip = self.composition.CreateRectangleClip()?;
                    clip.SetLeft2(left)?;
                    clip.SetTop2(top)?;
                    clip.SetRight2(right)?;
                    clip.SetBottom2(bottom)?;
                    self.composition_visual.SetClip(&clip)?;
                    self.dock_clip = Some(clip);
                }
                None => {
                    self.composition_visual.SetClip(None::<&IDCompositionClip>)?;
                    self.dock_clip = None;
                }
            }
            self.composition.Commit()?;
        }
        Ok(())
    }
}

fn brush(target: &ID2D1DeviceContext, color: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {
    unsafe { target.CreateSolidColorBrush(&color, None) }
}

#[derive(Clone, Copy)]
struct Pose {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    ease: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Press {
    Minimize,
    Pill,
    Peek,
    Settings,
    Step(Step),
}

/// One button on a settings stepper: which row, hour or minute, and which way.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Step {
    row: u8,
    field: u8,
    delta: i8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Miss,
    Drag,
    /// The collapsed pill. A click expands it; a drag still moves the window.
    Pill,
    /// The card is tucked against an edge. A click slides it back out.
    Peek,
    Chrome(i32),
    Step(Step),
    /// A background segment. 0 is the paper card, 1 is the ocean.
    Segment(u8),
    /// A language segment. 0 中文, 1 日本語, 2 English, 3 한국어.
    Lang(u8),
    /// A reminder-color segment. 0 竹绿, 1 樱粉, 2 天蓝, 3 鎏金.
    Accent(u8),
    /// Edge tuck. 0 hides at the edge, 1 stays inside the work area.
    Tuck(u8),
    /// Writing. 0 always, 1 only while the pointer is on the card.
    Caption(u8),
}

fn pose_key(pose: Pose) -> (i32, i32, i32, i32, i32) {
    (
        (pose.x * 4.0) as i32,
        (pose.y * 4.0) as i32,
        (pose.w * 4.0) as i32,
        (pose.h * 4.0) as i32,
        (pose.radius * 4.0) as i32,
    )
}

fn cluster_w() -> f32 {
    STEP_BTN + STEP_GAP + FIELD_NUM_W + STEP_GAP + STEP_BTN
}

fn settings_block_x(card_w: f32) -> f32 {
    let total = cluster_w() + CLUSTER_GAP + COLON_W + CLUSTER_GAP + cluster_w();
    card_w - PAD_X_DIP - total
}

const fn settings_row_y(row: u8) -> f32 {
    PAD_Y_DIP + LABEL_H_DIP + 14.0 + row as f32 * (STEP_ROW + 8.0)
}

/// Width of the time steppers, which is also the inline background control.
const fn stepper_row_w() -> f32 {
    let cluster = STEP_BTN + STEP_GAP + FIELD_NUM_W + STEP_GAP + STEP_BTN;
    cluster + CLUSTER_GAP + COLON_W + CLUSTER_GAP + cluster
}

/// Four two-character labels need about 52px each. The stepper row is narrower
/// than that, so the label moves above a full-width control.
const fn background_inline() -> bool {
    let x = CARD_WIDTH_DIP - PAD_X_DIP - stepper_row_w();
    let w = CARD_WIDTH_DIP - PAD_X_DIP - x;
    w / THEME_COUNT >= 52.0
}

const fn background_bottom() -> f32 {
    let top = settings_row_y(2);
    if background_inline() {
        top + STEP_ROW
    } else {
        top + LABEL_H_DIP + 6.0 + SEG_H
    }
}

const fn lang_label_y() -> f32 {
    background_bottom() + 10.0
}

const fn lang_bottom() -> f32 {
    lang_label_y() + LABEL_H_DIP + 6.0 + SEG_H
}

const fn accent_label_y() -> f32 {
    lang_bottom() + 10.0
}

const fn row_block() -> f32 {
    LABEL_H_DIP + 6.0 + SEG_H
}

const fn tuck_label_y() -> f32 {
    accent_label_y() + row_block() + 10.0
}

const fn tuck_bottom() -> f32 {
    tuck_label_y() + row_block()
}

const fn caption_label_y() -> f32 {
    tuck_bottom() + 10.0
}

const fn settings_page_dip(scenic: bool) -> f32 {
    let end = if scenic { caption_label_y() + row_block() } else { tuck_bottom() };
    end + 16.0
}

const fn settings_card_dip() -> f32 {
    settings_page_dip(true)
}

fn tuck_box(card_w: f32) -> (f32, f32, f32, f32) {
    pair_box(card_w, tuck_label_y())
}

fn caption_box(card_w: f32) -> (f32, f32, f32, f32) {
    pair_box(card_w, caption_label_y())
}

fn pair_box(card_w: f32, label_y: f32) -> (f32, f32, f32, f32) {
    let x = PAD_X_DIP;
    let y = label_y + LABEL_H_DIP + 6.0;
    let w = (card_w - PAD_X_DIP * 2.0).max(SEG_H * 2.0);
    (x, y, w, SEG_H)
}

fn accent_box(card_w: f32) -> (f32, f32, f32, f32) {
    let x = PAD_X_DIP;
    let y = accent_label_y() + LABEL_H_DIP + 6.0;
    let w = (card_w - PAD_X_DIP * 2.0).max(SEG_H * 4.0);
    (x, y, w, SEG_H)
}

fn lang_box(card_w: f32) -> (f32, f32, f32, f32) {
    let x = PAD_X_DIP;
    let y = lang_label_y() + LABEL_H_DIP + 6.0;
    let w = (card_w - PAD_X_DIP * 2.0).max(SEG_H * 4.0);
    (x, y, w, SEG_H)
}

fn segment_box(card_w: f32) -> (f32, f32, f32, f32) {
    if background_inline() {
        let x = settings_block_x(card_w);
        let y = settings_row_y(2) + (STEP_ROW - SEG_H) * 0.5;
        let w = (card_w - PAD_X_DIP - x).max(SEG_H * THEME_COUNT);
        (x, y, w, SEG_H)
    } else {
        let x = PAD_X_DIP;
        let y = settings_row_y(2) + LABEL_H_DIP + 6.0;
        let w = (card_w - PAD_X_DIP * 2.0).max(SEG_H * THEME_COUNT);
        (x, y, w, SEG_H)
    }
}

fn field_origin(card_w: f32, field: u8) -> f32 {
    let x = settings_block_x(card_w);
    if field == 0 {
        x
    } else {
        x + cluster_w() + CLUSTER_GAP + COLON_W + CLUSTER_GAP
    }
}

fn field_num_x(card_w: f32, field: u8) -> f32 {
    field_origin(card_w, field) + STEP_BTN + STEP_GAP
}

fn colon_x(card_w: f32) -> f32 {
    settings_block_x(card_w) + cluster_w() + CLUSTER_GAP
}

#[derive(Clone, Copy)]
struct StepSlot {
    x: f32,
    y: f32,
    step: Step,
}

fn step_slots(card_w: f32) -> [StepSlot; 8] {
    let mut slots = [StepSlot {
        x: 0.0,
        y: 0.0,
        step: Step { row: 0, field: 0, delta: -1 },
    }; 8];
    let mut index = 0;
    for row in 0..2u8 {
        let y = settings_row_y(row) + (STEP_ROW - STEP_BTN) * 0.5;
        for field in 0..2u8 {
            let origin = field_origin(card_w, field);
            let plus_x = origin + STEP_BTN + STEP_GAP + FIELD_NUM_W + STEP_GAP;
            slots[index] = StepSlot { x: origin, y, step: Step { row, field, delta: -1 } };
            index += 1;
            slots[index] = StepSlot { x: plus_x, y, step: Step { row, field, delta: 1 } };
            index += 1;
        }
    }
    slots
}

/// `field` 0 moves the hour by `delta`, field 1 snaps minutes onto a five-minute grid.
pub(crate) fn nudge_minutes(mins: i32, field: u8, delta: i32) -> i32 {
    let hour = mins.div_euclid(60).rem_euclid(24);
    let minute = mins.rem_euclid(60);
    if field == 0 {
        let hour = (hour + delta).rem_euclid(24);
        hour * 60 + minute
    } else {
        let base = minute.div_euclid(5) * 5;
        let minute = (base + delta * 5).rem_euclid(60);
        hour * 60 + minute
    }
}

fn draw_step_glyph(
    target: &ID2D1DeviceContext,
    brush: &ID2D1SolidColorBrush,
    stroke: &ID2D1StrokeStyle,
    x: f32,
    y: f32,
    size: f32,
    delta: i8,
) {
    let cx = x + size * 0.5;
    let cy = y + size * 0.5;
    let arm = size * 0.22;
    unsafe {
        target.DrawLine(
            Vector2 { X: cx - arm, Y: cy },
            Vector2 { X: cx + arm, Y: cy },
            brush,
            1.5,
            stroke,
        );
        if delta > 0 {
            target.DrawLine(
                Vector2 { X: cx, Y: cy - arm },
                Vector2 { X: cx, Y: cy + arm },
                brush,
                1.5,
                stroke,
            );
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn mix_color(from: (f32, f32, f32), to: (f32, f32, f32), t: f32, alpha: f32) -> D2D1_COLOR_F {
    let t = t.clamp(0.0, 1.0);
    rgba(
        from.0 + (to.0 - from.0) * t,
        from.1 + (to.1 - from.1) * t,
        from.2 + (to.2 - from.2) * t,
        alpha,
    )
}

fn approach(current: f32, target: f32, dt: f32, seconds: f32) -> f32 {
    let step = (dt / seconds.max(0.001)).min(1.0);
    if (target - current).abs() <= step {
        target
    } else if current < target {
        current + step
    } else {
        current - step
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cubic ease-out: `1 - (1 - t)³`. Most of the distance is covered up front,
/// then the motion decelerates into the target. Symmetric smoothstep made the
/// fold hesitate at both ends.
fn ease_out(t: f32) -> f32 {
    let t = 1.0 - t.clamp(0.0, 1.0);
    1.0 - t * t * t
}

/// Play `ease_out` from the current value toward `target`. A new target
/// restarts the curve, so both directions leave quickly and settle.
fn glide(value: &mut f32, from: &mut f32, to: &mut f32, u: &mut f32, target: f32, dt: f32, seconds: f32) {
    if (*to - target).abs() > 1.0e-4 {
        *from = *value;
        *to = target;
        *u = 0.0;
    }
    if *u >= 1.0 {
        *value = *to;
        return;
    }
    *u = (*u + dt / seconds.max(0.001)).min(1.0);
    *value = lerp(*from, *to, ease_out(*u));
    if *u >= 1.0 {
        *value = *to;
    }
}

fn in_round(px: f32, py: f32, left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> bool {
    if px < left || py < top || px > right || py > bottom {
        return false;
    }
    let cx = if px < left + radius {
        left + radius
    } else if px > right - radius {
        right - radius
    } else {
        px
    };
    let cy = if py < top + radius {
        top + radius
    } else if py > bottom - radius {
        bottom - radius
    } else {
        py
    };
    let (dx, dy) = (px - cx, py - cy);
    dx * dx + dy * dy <= radius * radius
}

/// Top-right buttons, in card-local DIPs: settings, then minimize.
fn chrome_slots(card_w: f32) -> [(f32, f32); 2] {
    let y = PAD_Y_DIP + (LABEL_H_DIP - CHROME_BOX_DIP) * 0.5;
    let stride = CHROME_BOX_DIP + CHROME_GAP_DIP;
    let x0 = card_w - PAD_X_DIP - CHROME_BOX_DIP * 2.0 - CHROME_GAP_DIP;
    [(x0, y), (x0 + stride, y)]
}

fn draw_chrome_icon(
    target: &ID2D1DeviceContext,
    brush: &ID2D1SolidColorBrush,
    stroke: &ID2D1StrokeStyle,
    gear: &ID2D1Geometry,
    index: usize,
    settings: bool,
    x: f32,
    y: f32,
    size: f32,
) {
    let scale = size / 24.0;
    let placed = Matrix3x2 {
        M11: scale,
        M12: 0.0,
        M21: 0.0,
        M22: scale,
        M31: x,
        M32: y,
    };
    let at = |ux: f32, uy: f32| Vector2 {
        X: ux,
        Y: uy,
    };
    unsafe {
        target.SetTransform(&placed);
        match index {
            0 if settings => {
                // Chevron left: the same corner button leaves the settings page.
                target.DrawLine(at(14.5, 6.5), at(9.0, 12.0), brush, 2.0, stroke);
                target.DrawLine(at(9.0, 12.0), at(14.5, 17.5), brush, 2.0, stroke);
            }
            0 => {
                target.DrawGeometry(gear, brush, 2.0, stroke);
                let hole = D2D1_ELLIPSE {
                    point: at(12.0, 12.0),
                    radiusX: 3.0,
                    radiusY: 3.0,
                };
                target.DrawEllipse(&hole, brush, 2.0, stroke);
            }
            _ => target.DrawLine(at(5.0, 12.0), at(19.0, 12.0), brush, 2.0, stroke),
        }
        let identity = Matrix3x2::identity();
        target.SetTransform(&identity);
    }
}

/// Lucide "settings" outline, viewBox 24. The centre hole is drawn separately.
/// https://lucide.dev/icons/settings
const LUCIDE_SETTINGS: &str = "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z";

fn lucide_settings_geometry(factory: &ID2D1Factory) -> Result<ID2D1Geometry> {
    unsafe {
        let path: ID2D1PathGeometry = factory.CreatePathGeometry()?;
        let sink = path.Open()?;
        append_svg_path(&sink, LUCIDE_SETTINGS);
        sink.Close()?;
        Ok(path.cast()?)
    }
}

fn append_svg_path(sink: &ID2D1GeometrySink, path: &str) {
    let bytes = path.as_bytes();
    let mut i = 0usize;
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut sx = 0.0f32;
    let mut sy = 0.0f32;
    let mut cmd = 0u8;
    let mut open = false;
    let mut pen = false;
    loop {
        skip_svg(bytes, &mut i);
        if i >= bytes.len() {
            break;
        }
        if bytes[i].is_ascii_alphabetic() {
            cmd = bytes[i];
            i += 1;
            if cmd == b'Z' || cmd == b'z' {
                if open {
                    unsafe { sink.EndFigure(D2D1_FIGURE_END_CLOSED) };
                    open = false;
                }
                cx = sx;
                cy = sy;
                pen = true;
                continue;
            }
        } else if cmd == 0 {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        match cmd {
            b'M' | b'm' => {
                let Some((x, y)) = svg_pair(bytes, &mut i, rel, cx, cy) else { break };
                if open {
                    unsafe { sink.EndFigure(D2D1_FIGURE_END_OPEN) };
                }
                unsafe { sink.BeginFigure(Vector2 { X: x, Y: y }, D2D1_FIGURE_BEGIN_FILLED) };
                open = true;
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                pen = true;
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' | b'l' => {
                let Some((x, y)) = svg_pair(bytes, &mut i, rel, cx, cy) else { break };
                unsafe { sink.AddLine(Vector2 { X: x, Y: y }) };
                cx = x;
                cy = y;
                pen = true;
            }
            b'H' | b'h' => {
                let Some(x) = svg_number(bytes, &mut i) else { break };
                let x = if rel { cx + x } else { x };
                unsafe { sink.AddLine(Vector2 { X: x, Y: cy }) };
                cx = x;
                pen = true;
            }
            b'V' | b'v' => {
                let Some(y) = svg_number(bytes, &mut i) else { break };
                let y = if rel { cy + y } else { y };
                unsafe { sink.AddLine(Vector2 { X: cx, Y: y }) };
                cy = y;
                pen = true;
            }
            b'A' | b'a' => {
                let Some(rx) = svg_number(bytes, &mut i) else { break };
                let Some(ry) = svg_number(bytes, &mut i) else { break };
                let Some(rot) = svg_number(bytes, &mut i) else { break };
                let Some(large) = svg_number(bytes, &mut i) else { break };
                let Some(sweep) = svg_number(bytes, &mut i) else { break };
                let Some((x, y)) = svg_pair(bytes, &mut i, rel, cx, cy) else { break };
                if pen && ((x - cx).abs() > 1.0e-4 || (y - cy).abs() > 1.0e-4) {
                    let (rx, ry) = fit_arc_radii(cx, cy, rx, ry, rot, x, y);
                    if rx == 0.0 || ry == 0.0 {
                        unsafe { sink.AddLine(Vector2 { X: x, Y: y }) };
                    } else {
                        let arc = D2D1_ARC_SEGMENT {
                            point: Vector2 { X: x, Y: y },
                            size: D2D_SIZE_F { width: rx, height: ry },
                            rotationAngle: rot,
                            sweepDirection: if sweep != 0.0 {
                                D2D1_SWEEP_DIRECTION_CLOCKWISE
                            } else {
                                D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE
                            },
                            arcSize: if large != 0.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
                        };
                        unsafe { sink.AddArc(&arc) };
                    }
                }
                cx = x;
                cy = y;
                pen = true;
            }
            _ => break,
        }
    }
    if open {
        unsafe { sink.EndFigure(D2D1_FIGURE_END_OPEN) };
    }
}

fn fit_arc_radii(x0: f32, y0: f32, rx: f32, ry: f32, phi_deg: f32, x1: f32, y1: f32) -> (f32, f32) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        return (rx, ry);
    }
    let phi = phi_deg.to_radians();
    let (cos, sin) = (phi.cos(), phi.sin());
    let dx = (x0 - x1) * 0.5;
    let dy = (y0 - y1) * 0.5;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lam > 1.0 {
        let grown = lam.sqrt();
        rx *= grown;
        ry *= grown;
    }
    (rx, ry)
}

fn skip_svg(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && matches!(bytes[*i], b',' | b' ' | b'\n' | b'\t' | b'\r') {
        *i += 1;
    }
}

fn svg_pair(bytes: &[u8], i: &mut usize, rel: bool, cx: f32, cy: f32) -> Option<(f32, f32)> {
    let x = svg_number(bytes, i)?;
    let y = svg_number(bytes, i)?;
    if rel { Some((cx + x, cy + y)) } else { Some((x, y)) }
}

fn svg_number(bytes: &[u8], i: &mut usize) -> Option<f32> {
    skip_svg(bytes, i);
    let start = *i;
    if *i < bytes.len() && (bytes[*i] == b'+' || bytes[*i] == b'-') {
        *i += 1;
    }
    let mut saw = false;
    while *i < bytes.len() && bytes[*i].is_ascii_digit() {
        *i += 1;
        saw = true;
    }
    if *i < bytes.len() && bytes[*i] == b'.' {
        *i += 1;
        while *i < bytes.len() && bytes[*i].is_ascii_digit() {
            *i += 1;
            saw = true;
        }
    }
    if !saw {
        *i = start;
        return None;
    }
    std::str::from_utf8(&bytes[start..*i]).ok()?.parse().ok()
}

/// Lucide's clock, viewBox 24 with a stroke of 2 and round caps.
/// https://lucide.dev/icons/clock
fn draw_lucide_clock(
    target: &ID2D1DeviceContext,
    brush: &ID2D1SolidColorBrush,
    stroke: &ID2D1StrokeStyle,
    x: f32,
    y: f32,
    size: f32,
) {
    let scale = size / 24.0;
    let at = |ux: f32, uy: f32| Vector2 {
        X: x + ux * scale,
        Y: y + uy * scale,
    };
    let width = 2.0 * scale;
    let face = D2D1_ELLIPSE {
        point: at(12.0, 12.0),
        radiusX: 10.0 * scale,
        radiusY: 10.0 * scale,
    };
    unsafe {
        target.DrawEllipse(&face, brush, width, stroke);
        // Hands: 12 o'clock, then out to about four.
        target.DrawLine(at(12.0, 6.0), at(12.0, 12.0), brush, width, stroke);
        target.DrawLine(at(12.0, 12.0), at(16.0, 14.0), brush, width, stroke);
    }
}

/// Off-work mark: a green stroked circle and tick, authored on a 16 box.
fn draw_check_icon(
    target: &ID2D1DeviceContext,
    brush: &ID2D1SolidColorBrush,
    stroke: &ID2D1StrokeStyle,
    x: f32,
    y: f32,
    size: f32,
) {
    let scale = size / 16.0;
    let at = |ux: f32, uy: f32| Vector2 {
        X: x + ux * scale,
        Y: y + uy * scale,
    };
    let face = D2D1_ELLIPSE {
        point: at(8.0, 8.0),
        radiusX: 6.5 * scale,
        radiusY: 6.5 * scale,
    };
    let width = 1.5 * scale;
    unsafe {
        target.DrawEllipse(&face, brush, width, stroke);
        target.DrawLine(at(4.3, 8.2), at(6.7, 10.6), brush, width, stroke);
        target.DrawLine(at(6.7, 10.6), at(11.8, 5.2), brush, width, stroke);
    }
}

fn draw_text_sized(
    target: &ID2D1DeviceContext,
    factory: &IDWriteFactory,
    format: &IDWriteTextFormat,
    brush: &ID2D1SolidColorBrush,
    text: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    size: f32,
) -> Result<()> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() || w <= 1.0 || h <= 1.0 {
        return Ok(());
    }
    unsafe {
        let layout = factory.CreateTextLayout(&wide, format, w, h)?;
        layout.SetFontSize(
            size,
            DWRITE_TEXT_RANGE { startPosition: 0, length: wide.len() as u32 },
        )?;
        target.DrawTextLayout(
            Vector2 { X: x, Y: y },
            &layout,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
        );
    }
    Ok(())
}

fn draw_text(
    target: &ID2D1DeviceContext,
    format: &IDWriteTextFormat,
    brush: &ID2D1SolidColorBrush,
    text: &str,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) -> Result<()> {
    let text = HSTRING::from(text);
    unsafe {
        target.DrawText(
            &text,
            format,
            &d2d_rect(left, top, right, bottom),
            brush as &ID2D1Brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
    Ok(())
}

fn reminder_rgb(accent: u8) -> (f32, f32, f32) {
    match accent {
        1 => PINK,
        2 => SKY,
        3 => GOLD_SILK,
        _ => GREEN,
    }
}

/// Segment names. Gold is darker on the white pill so it stays readable.
fn accent_label_rgb(index: u8, selected: bool) -> (f32, f32, f32) {
    match index {
        1 => (0.92, 0.28, 0.50),
        2 => SKY,
        3 => {
            if selected {
                (107.0 / 255.0, 74.0 / 255.0, 18.0 / 255.0)
            } else {
                (196.0 / 255.0, 148.0 / 255.0, 46.0 / 255.0)
            }
        }
        _ => (0.08, 0.62, 0.32),
    }
}

fn gold_tile(d2d: &ID2D1DeviceContext, dpi: f32) -> Result<(ID2D1Bitmap, ID2D1BitmapBrush1)> {
    const N: u32 = GOLD_N;
    let pixels = gold_frame(N, 0.0);
    let target: ID2D1RenderTarget = d2d.cast()?;
    let props = D2D1_BITMAP_PROPERTIES {
        pixelFormat: premul_format(),
        dpiX: dpi,
        dpiY: dpi,
    };
    let bitmap = unsafe {
        target.CreateBitmap(
            D2D_SIZE_U { width: N, height: N },
            Some(pixels.as_ptr() as *const _),
            N * 4,
            &props,
        )?
    };
    let brush_props = D2D1_BITMAP_BRUSH_PROPERTIES1 {
        extendModeX: D2D1_EXTEND_MODE_WRAP,
        extendModeY: D2D1_EXTEND_MODE_WRAP,
        interpolationMode: D2D1_INTERPOLATION_MODE_LINEAR,
    };
    let brush = unsafe { d2d.CreateBitmapBrush(&bitmap, Some(&brush_props), None)? };
    Ok((bitmap, brush))
}

const GOLD_N: u32 = 256;

impl Gfx {
    /// Rebuilds the gilt tile. The field is periodic, so the brush can wrap it.
    fn refresh_gold(&mut self) -> Result<()> {
        if (self.gold_stamp - self.clock).abs() < 1.0e-4 {
            return Ok(());
        }
        self.gold_stamp = self.clock;
        let pixels = gold_frame(GOLD_N, self.clock);
        unsafe { self.gold_bitmap.CopyFromMemory(None, pixels.as_ptr().cast(), GOLD_N * 4) }
    }
}

/// Rich gold, never the brown that read as dirt. The bar and the check use the same.
const GOLD_SILK: (f32, f32, f32) = (224.0 / 255.0, 168.0 / 255.0, 52.0 / 255.0);
const GOLD_BRIGHT: (f32, f32, f32) = (255.0 / 255.0, 214.0 / 255.0, 96.0 / 255.0);
const GOLD_HOT: (f32, f32, f32) = (255.0 / 255.0, 248.0 / 255.0, 220.0 / 255.0);

/// One seamless frame. Broad folds plus a slow breath, no grit and no dark valleys.
fn gold_frame(n: u32, time: f32) -> Vec<u8> {
    let mut pixels = vec![0u8; (n * n * 4) as usize];
    let inv = 4.0 / n as f32;
    let breath = (time * 0.5).sin() * 0.5 + 0.5;
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 * inv;
            let v = y as f32 * inv;
            let silk = fbm(u, v, time);
            // One full wave across the tile, so the wrap stays a continuous fold.
            let fold = ((u + v) * std::f32::consts::FRAC_PI_2 + time * 0.04).sin() * 0.5 + 0.5;
            let t = ((silk * 0.55 + fold * 0.45 - 0.5) * 1.45 + 0.5).clamp(0.0, 1.0);
            let (mut r, mut g, mut b) = gold_rgb(t);
            let glow = (breath * 0.2 + silk_sheen(x, n, time)).min(0.5);
            r = lerp(r, GOLD_HOT.0, glow);
            g = lerp(g, GOLD_HOT.1, glow);
            b = lerp(b, GOLD_HOT.2, glow);
            let i = ((y * n + x) * 4) as usize;
            pixels[i] = (b.clamp(0.0, 1.0) * 255.0) as u8;
            pixels[i + 1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
            pixels[i + 2] = (r.clamp(0.0, 1.0) * 255.0) as u8;
            pixels[i + 3] = 255;
        }
    }
    pixels
}

fn gold_rgb(t: f32) -> (f32, f32, f32) {
    if t < 0.58 {
        let u = smoothstep(0.0, 0.58, t);
        (
            lerp(GOLD_SILK.0, GOLD_BRIGHT.0, u),
            lerp(GOLD_SILK.1, GOLD_BRIGHT.1, u),
            lerp(GOLD_SILK.2, GOLD_BRIGHT.2, u),
        )
    } else {
        let u = smoothstep(0.58, 1.0, t);
        (
            lerp(GOLD_BRIGHT.0, GOLD_HOT.0, u),
            lerp(GOLD_BRIGHT.1, GOLD_HOT.1, u),
            lerp(GOLD_BRIGHT.2, GOLD_HOT.2, u),
        )
    }
}

/// A wide, dim band. It takes most of half a minute to cross the phrase.
fn silk_sheen(x: u32, n: u32, time: f32) -> f32 {
    let phase = (time * 0.03).fract();
    let mut d = (x as f32 / n as f32 - phase).abs();
    if d > 0.5 {
        d = 1.0 - d;
    }
    (-d * d * 5.0).exp() * 0.34
}

fn fbm(x: f32, y: f32, time: f32) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.55;
    let mut freq = 1.0;
    let mut period = 4i32;
    let mut norm = 0.0;
    for _ in 0..3 {
        let drift = time * 0.016 * freq;
        v += amp * value_noise(x * freq + drift, y * freq + drift * 0.35, period);
        norm += amp;
        amp *= 0.36;
        freq *= 2.0;
        period = period.saturating_mul(2);
    }
    v / norm
}

fn value_noise(x: f32, y: f32, period: i32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let fx = fade(x - x0 as f32);
    let fy = fade(y - y0 as f32);
    let v00 = hash_unit(x0, y0, period);
    let v10 = hash_unit(x0 + 1, y0, period);
    let v01 = hash_unit(x0, y0 + 1, period);
    let v11 = hash_unit(x0 + 1, y0 + 1, period);
    lerp(lerp(v00, v10, fx), lerp(v01, v11, fx), fy)
}

fn fade(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    fade(((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0))
}

fn hash_unit(x: i32, y: i32, period: i32) -> f32 {
    let period = period.max(1);
    (hash_cell(x.rem_euclid(period), y.rem_euclid(period)) & 0x00FF_FFFF) as f32 / 16_777_215.0
}

fn hash_cell(x: i32, y: i32) -> u32 {
    let mut n = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177);
    n ^ (n >> 16)
}

fn make_format(
    factory: &IDWriteFactory,
    collection: &IDWriteFontCollection1,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    alignment: DWRITE_TEXT_ALIGNMENT,
    paragraph: DWRITE_PARAGRAPH_ALIGNMENT,
    locale: windows::core::PCWSTR,
) -> Result<IDWriteTextFormat> {
    unsafe {
        let format = factory.CreateTextFormat(
            TEXT_FAMILY,
            Some(collection.deref()),
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            locale,
        )?;
        format.SetTextAlignment(alignment)?;
        format.SetParagraphAlignment(paragraph)?;
        Ok(format)
    }
}

fn embed_face(
    factory: &IDWriteFactory,
    factory3: &IDWriteFactory3,
    loader: &IDWriteInMemoryFontFileLoader,
    bytes: &'static [u8],
) -> Result<IDWriteFontCollection1> {
    unsafe {
        let file = loader.CreateInMemoryFontFileReference(
            factory,
            bytes.as_ptr() as *const _,
            bytes.len() as u32,
            None::<&windows::core::IUnknown>,
        )?;
        let face = factory3.CreateFontFaceReference(&file, 0, DWRITE_FONT_SIMULATIONS_NONE)?;
        let builder = factory3.CreateFontSetBuilder()?;
        // The windows crate lists the two AddFontFaceReference overloads backwards.
        // The first slot is the one-argument method, which reads names from the file.
        builder.AddFontFaceReference(&face, &[])?;
        let set = builder.CreateFontSet()?;
        factory3.CreateFontCollectionFromFontSet(&set)
    }
}

#[derive(Clone, Copy)]
struct Metrics {
    pad_px: f32,
    card_w: u32,
    card_h: u32,
    origin_dip: f32,
    card_dip_w: f32,
    card_dip_h: f32,
}

fn metrics(window_w: u32, window_h: u32, dpi: f32) -> Metrics {
    let scale = if dpi > 0.0 { dpi / 96.0 } else { 1.0 };
    let pad = (SHADOW_PAD_DIP * scale).round();
    let card_w = (window_w as f32 - pad * 2.0).max(1.0).round();
    let card_h = (window_h as f32 - pad * 2.0).max(1.0).round();
    Metrics {
        pad_px: pad,
        card_w: card_w as u32,
        card_h: card_h as u32,
        origin_dip: pad / scale,
        card_dip_w: card_w / scale,
        card_dip_h: card_h / scale,
    }
}

fn premul_format() -> D2D1_PIXEL_FORMAT {
    D2D1_PIXEL_FORMAT {
        format: DXGI_FORMAT_B8G8R8A8_UNORM,
        alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
    }
}

fn target_bitmap(d2d: &ID2D1DeviceContext, width: u32, height: u32, dpi: f32) -> Result<ID2D1Bitmap1> {
    let props = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: premul_format(),
        dpiX: dpi,
        dpiY: dpi,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET,
        colorContext: Default::default(),
    };
    unsafe { d2d.CreateBitmap(D2D_SIZE_U { width, height }, None, 0, &props) }
}

/// Two blurs of the card's silhouette: a wide faint one, then a short contact
/// shadow, both shifted down a little so the card reads as sitting on the desk.
fn bake_shadow(
    d2d: &ID2D1DeviceContext,
    width: u32,
    height: u32,
    dpi: f32,
    place: Metrics,
) -> Result<(ID2D1Bitmap1, ID2D1Bitmap1)> {
    let shape = target_bitmap(d2d, width, height, dpi)?;
    let shadow = target_bitmap(d2d, width, height, dpi)?;
    stamp(
        d2d, &shape, &shadow,
        place.origin_dip, place.origin_dip, place.card_dip_w, place.card_dip_h, CORNER_RADIUS_DIP,
        rgba(0.05, 0.07, 0.11, 0.32), 13.0, 7.0, true,
    )?;
    stamp(
        d2d, &shape, &shadow,
        place.origin_dip, place.origin_dip, place.card_dip_w, place.card_dip_h, CORNER_RADIUS_DIP,
        rgba(0.03, 0.04, 0.07, 0.20), 3.0, 1.5, false,
    )?;
    Ok((shape, shadow))
}

fn stamp(
    d2d: &ID2D1DeviceContext,
    shape: &ID2D1Bitmap1,
    dest: &ID2D1Bitmap1,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: D2D1_COLOR_F,
    std_dev: f32,
    dy: f32,
    clear_dest: bool,
) -> Result<()> {
    unsafe {
        d2d.SetTarget(shape);
        d2d.BeginDraw();
        let clear = rgba(0.0, 0.0, 0.0, 0.0);
        d2d.Clear(Some(&clear));
        let ink = brush(d2d, color)?;
        let rect = D2D1_ROUNDED_RECT {
            rect: d2d_rect(x, y + dy, x + w, y + dy + h),
            radiusX: radius,
            radiusY: radius,
        };
        d2d.FillRoundedRectangle(&rect, &ink);
        d2d.EndDraw(None, None)?;

        let effect = d2d.CreateEffect(&CLSID_D2D1GaussianBlur)?;
        let input: ID2D1Image = shape.cast()?;
        effect.SetInput(0, &input, true);
        effect.SetValue(
            D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION.0 as u32,
            D2D1_PROPERTY_TYPE_FLOAT,
            &std_dev.to_ne_bytes(),
        )?;
        let output: ID2D1Image = effect.cast()?;

        d2d.SetTarget(dest);
        d2d.BeginDraw();
        if clear_dest {
            d2d.Clear(Some(&clear));
        }
        d2d.DrawImage(&output, None, None, D2D1_INTERPOLATION_MODE_LINEAR, D2D1_COMPOSITE_MODE_SOURCE_OVER);
        d2d.EndDraw(None, None)?;
        d2d.SetTarget(None::<&ID2D1Image>);
    }
    Ok(())
}

/// The client size of `hwnd`, in pixels.
pub fn client_size(hwnd: HWND) -> (u32, u32) {
    unsafe {
        let mut rect = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
        (
            (rect.right - rect.left).max(1) as u32,
            (rect.bottom - rect.top).max(1) as u32,
        )
    }
}

/// Decodes and uploads the shader's channels: the noise texture, the moon and a
/// single black texel standing in for the third channel the source expects.
fn ocean_channels(
    device: &ID3D11Device,
) -> Result<(Vec<ID3D11ShaderResourceView>, [[f32; 3]; 4])> {
    let mut views = Vec::with_capacity(3);
    let mut sizes = [[0.0f32; 3]; 4];
    for (index, bytes) in [CHANNEL0_PNG, CHANNEL1_JPG].into_iter().enumerate() {
        let image = crate::texture::decode(bytes)?;
        crate::log(&format!(
            "channel{index}: {}x{}",
            image.width, image.height
        ));
        sizes[index] = [image.width as f32, image.height as f32, 1.0];
        let uploaded = crate::texture::upload(device, &image)?;
        let mut view: Option<ID3D11ShaderResourceView> = None;
        unsafe { device.CreateShaderResourceView(&uploaded, None, Some(&mut view))? };
        views.push(view.ok_or_else(fail)?);
    }
    sizes[2] = [1.0, 1.0, 1.0];
    let black = crate::texture::Image {
        width: 1,
        height: 1,
        pixels: vec![0, 0, 0, 255],
    };
    let uploaded = crate::texture::upload(device, &black)?;
    let mut view: Option<ID3D11ShaderResourceView> = None;
    unsafe { device.CreateShaderResourceView(&uploaded, None, Some(&mut view))? };
    views.push(view.ok_or_else(fail)?);
    Ok((views, sizes))
}

#[cfg(test)]
mod tests {
    use super::{
        ease_out, glide, gold_frame, lang_box, accent_box, tuck_box, caption_box, nudge_minutes, segment_box, sun_uv,
        CARD_HEIGHT_DIP, SETTINGS_CARD_DIP,
    };

    #[test]
    fn gilt_silk_stays_bright_and_moves() {
        let a = gold_frame(64, 0.0);
        let b = gold_frame(64, 16.0);
        let mut min_r = 255u8;
        let mut min_g = 255u8;
        let mut max_r = 0u8;
        let mut changed = 0u32;
        for i in (0..a.len()).step_by(4) {
            min_g = min_g.min(a[i + 1]);
            min_r = min_r.min(a[i + 2]);
            max_r = max_r.max(a[i + 2]);
            changed += (a[i + 2] as i32 - b[i + 2] as i32).unsigned_abs();
        }
        assert!(min_r > 210, "valleys should stay gold, red was {min_r}");
        assert!(min_g > 160, "valleys should stay gold, green was {min_g}");
        assert!(max_r > 248, "the silk should still catch light, red was {max_r}");
        assert!(changed > 800, "the field should drift, delta {changed}");
    }

    #[test]
    fn the_sun_crosses_the_sky_between_sunrise_and_sunset() {
        let (x6, y6) = sun_uv(6.0 * 3600.0);
        let (x9, y9) = sun_uv(9.0 * 3600.0);
        let (x12, y12) = sun_uv(12.0 * 3600.0);
        let (x15, y15) = sun_uv(15.0 * 3600.0);
        let (x18, y18) = sun_uv(18.0 * 3600.0);
        let (_x0, y0) = sun_uv(0.0);
        assert!(x6 < 0.15 && x18 > 0.85);
        assert!((x12 - 0.5).abs() < 0.02);
        assert!(x6 < x9 && x9 < x12 && x12 < x15 && x15 < x18);
        assert!((y6 - 0.25).abs() < 0.02 && (y18 - 0.25).abs() < 0.02);
        assert!(y9 < y12 && y15 < y12 && y12 > 0.65);
        assert!(y0 < 0.0);
        assert!(x6 > 0.0 && y6 > 0.0);
    }

    #[test]
    fn settings_card_fits_the_background_switch() {
        let (_x, y, w, h) = segment_box(300.0);
        assert!(w / 3.0 > 36.0);
        assert!(y + h + 12.0 < SETTINGS_CARD_DIP);
        let (_lx, ly, lw, lh) = lang_box(300.0);
        assert!(lw / 4.0 > 52.0);
        assert!(ly + lh + 12.0 < SETTINGS_CARD_DIP);
        assert!(ly > y + h);
        let (_ax, ay, aw, ah) = accent_box(300.0);
        assert!(aw / 4.0 > 52.0);
        assert!(ay > ly + lh);
        assert!(ay + ah + 12.0 < SETTINGS_CARD_DIP);
        let (_tx, ty, tw, th) = tuck_box(300.0);
        assert!(tw / 2.0 > 52.0);
        assert!(ty > ay + ah);
        assert!(ty + th + 12.0 < SETTINGS_CARD_DIP);
        let (_cx, cy, _cw, ch) = caption_box(300.0);
        assert!(cy > ty + th);
        assert!(cy + ch + 12.0 < SETTINGS_CARD_DIP);
        assert!(SETTINGS_CARD_DIP > CARD_HEIGHT_DIP);
    }

    #[test]
    fn ease_out_covers_distance_early() {
        assert_eq!(ease_out(0.0), 0.0);
        assert_eq!(ease_out(1.0), 1.0);
        let early = ease_out(0.2);
        assert!(early > 0.45 && early < 0.55, "{early}");
        assert!(ease_out(0.5) > 0.85);
        let mut prev = 0.0;
        let mut prev_step = f32::MAX;
        for i in 1..=8 {
            let v = ease_out(i as f32 / 8.0);
            let step = v - prev;
            assert!(step > 0.0);
            assert!(step <= prev_step + 1.0e-5);
            prev = v;
            prev_step = step;
        }
    }

    #[test]
    fn glide_is_fast_then_slow_both_ways() {
        let mut value = 0.0;
        let mut from = 0.0;
        let mut to = 0.0;
        let mut u = 1.0;
        let mut steps = Vec::new();
        for _ in 0..6 {
            let before = value;
            glide(&mut value, &mut from, &mut to, &mut u, 1.0, 0.05, 0.30);
            steps.push(value - before);
        }
        assert!(steps[0] > steps[5]);
        assert!((value - 1.0).abs() < 1.0e-4);
        let mut back = Vec::new();
        for _ in 0..6 {
            let before = value;
            glide(&mut value, &mut from, &mut to, &mut u, 0.0, 0.05, 0.30);
            back.push(before - value);
        }
        assert!(back[0] > back[5]);
        assert!((value - 0.0).abs() < 1.0e-4);
    }

    #[test]
    fn hours_wrap_through_midnight() {
        assert_eq!(nudge_minutes(9 * 60, 0, 1), 10 * 60);
        assert_eq!(nudge_minutes(23 * 60, 0, 1), 0);
        assert_eq!(nudge_minutes(0, 0, -1), 23 * 60);
    }

    #[test]
    fn minutes_step_by_five_and_wrap() {
        assert_eq!(nudge_minutes(18 * 60, 1, 1), 18 * 60 + 5);
        assert_eq!(nudge_minutes(18 * 60 + 55, 1, 1), 18 * 60);
        assert_eq!(nudge_minutes(18 * 60 + 7, 1, 1), 18 * 60 + 10);
        assert_eq!(nudge_minutes(18 * 60 + 7, 1, -1), 18 * 60);
    }
}