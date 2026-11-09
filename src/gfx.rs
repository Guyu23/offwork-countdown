// Direct3D 11 draws the shader into its own texture. Direct2D then composites a
// soft shadow and the rounded card into a premultiplied swap chain, and
// DirectWrite lays the text out. One device, one swap chain, one present.

use std::ops::Deref;

use windows::core::{Interface, Result, HSTRING, PCSTR};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{HMONITOR, ScreenToClient};

use crate::dock::{self, Dock, Edges, Placement};
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, WindowFromPoint};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_COMPOSITE_MODE_SOURCE_OVER,
    D2D1_FIGURE_BEGIN_FILLED, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_PIXEL_FORMAT,
    D2D_RECT_F, D2D_SIZE_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE, D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE,
    D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_SOLID, D2D1_ELLIPSE,
    D2D1_GAUSSIANBLUR_PROP_STANDARD_DEVIATION, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE,
    D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1, D2D1_LINE_JOIN_ROUND, D2D1_PROPERTY_TYPE_FLOAT,
    D2D1_ROUNDED_RECT, D2D1_STROKE_STYLE_PROPERTIES1, D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
    CLSID_D2D1GaussianBlur, ID2D1Bitmap1, ID2D1Brush, ID2D1Device, ID2D1DeviceContext, ID2D1Factory1,
    ID2D1Factory, ID2D1Geometry, ID2D1GeometrySink, ID2D1Image, ID2D1Layer, ID2D1PathGeometry,
    ID2D1SolidColorBrush, ID2D1StrokeStyle,
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
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_TEXT_RANGE,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_PARAGRAPH_ALIGNMENT_FAR,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_LEADING,
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

/// What the card shows this frame.
pub struct Frame {
    pub time_text: String,
    pub label: String,
    pub footer_left: String,
    pub footer_right: String,
    pub progress: f32,
    /// 0 = aurora, 1 = ocean.
    pub background: f32,
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
    vertex_shader: ID3D11VertexShader,
    pixel_shader: ID3D11PixelShader,
    constants: ID3D11Buffer,
    /// The ported Shadertoy ocean: its own pixel shader, uniforms and channels.
    ocean_shader: ID3D11PixelShader,
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
    /// 0 expanded, 1 the single-row pill. `compact_target` is where the
    /// animation is going.
    compact: f32,
    compact_target: bool,
    /// Mouse-down for a click that should not immediately drag.
    press: Option<(i32, i32, Press)>,
    shadow_key: (i32, i32, i32, i32, i32),
    clip: ID2D1Geometry,
    layer: ID2D1Layer,
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
    layout_dirty: bool,
    /// Last work-area clip pushed to the composition visual. `None` is no clip.
    clip_key: Option<(i32, i32, i32, i32)>,
    /// Kept alive while the visual is clipped to the work area.
    dock_clip: Option<IDCompositionRectangleClip>,
}

const SHADER: &str = include_str!("shader.hlsl");
const OCEAN_SHADER: &str = include_str!("ocean.hlsl");
const CHANNEL0_PNG: &[u8] = include_bytes!("../assets/Channel0.png");
const CHANNEL1_JPG: &[u8] = include_bytes!("../assets/Channel1.jpg");
const TEXT_FAMILY: windows::core::PCWSTR = windows::core::w!("Consolas");

fn fail() -> windows::core::Error {
    windows::core::Error::from(windows::Win32::Foundation::E_FAIL)
}

fn d2d_rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F { left, top, right, bottom }
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

            Ok(Self {
                text_time: make_format(&dwrite, 40.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?,
                text_label: make_format(&dwrite, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?,
                text_footer: make_format(&dwrite, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_PARAGRAPH_ALIGNMENT_FAR)?,
                text_footer_right: make_format(&dwrite, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_PARAGRAPH_ALIGNMENT_FAR)?,
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
                compact: 0.0,
                compact_target: false,
                press: None,
                shadow_key,
                clip,
                layer,
                stroke,
                gear,
                dpi,
                clock: 0.0,
                dock: None,
                tucked: false,
                drag_cursor: POINT::default(),
                drag_tucked: false,
                layout_dirty: false,
                clip_key: None,
                dock_clip: None,
            })
        }
    }

    /// Advance the fold before the window position is settled for this frame.
    pub fn prepare(&mut self, dt: f32) {
        self.advance_compact(dt);
    }

    pub fn render(&mut self, frame: &Frame, dt: f32) -> Result<()> {
        if !frame.paused {
            self.clock += dt;
        }
        let pose = self.pose();
        self.ensure_shadow(pose)?;
        let animating = self.is_animating();
        unsafe {
            // 1. The shader draws into its own colour buffer.
            let scene_view = self.scene_view.clone();
            self.draw_shader(&scene_view, frame.background)?;
            self.context.OMSetRenderTargets(None, None);
            self.context.Flush();

            // 2. Direct2D composes the card on the swap chain's back buffer,
            //    sampling that buffer: rounded clipping, blur and filters all
            //    have the background texture to work with.
            let back: ID3D11Texture2D = self.swap_chain.GetBuffer(0)?;
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
            let target: ID2D1Bitmap1 = self.d2d.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
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
        folding || docking
    }

    fn draw_shader(&self, view: &ID3D11RenderTargetView, mode: f32) -> Result<()> {
        unsafe {
            self.context.OMSetRenderTargets(Some(&[Some(view.clone())]), None);
            let viewport = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.card_w as f32,
                Height: self.card_h as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.context.RSSetViewports(Some(&[viewport]));
            self.context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vertex_shader, None);

            if mode >= 0.5 {
                // The ported Shadertoy pass: its own uniforms and channels.
                let params = OceanParams {
                    resolution: [self.card_w as f32, self.card_h as f32, 1.0],
                    time: self.clock,
                    mouse: [0.0; 4],
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

                self.context.PSSetShader(&self.ocean_shader, None);
                self.context
                    .PSSetConstantBuffers(0, Some(&[Some(self.ocean_constants.clone())]));
                let views: Vec<Option<ID3D11ShaderResourceView>> =
                    self.channels.iter().cloned().map(Some).collect();
                self.context.PSSetShaderResources(0, Some(&views));
                self.context
                    .PSSetSamplers(0, Some(&[Some(self.channel_sampler.clone())]));
                self.context.Draw(3, 0);

                // Leave no channel bound while Direct2D draws.
                let none: Vec<Option<ID3D11ShaderResourceView>> = vec![None; self.channels.len()];
                self.context.PSSetShaderResources(0, Some(&none));
                return Ok(());
            }

            let params = Params {
                time: self.clock,
                width: self.card_w as f32,
                height: self.card_h as f32,
                mode,
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
        let step = (dt / 0.30).min(1.0);
        if (target - self.compact).abs() <= step {
            self.compact = target;
        } else if self.compact < target {
            self.compact += step;
        } else {
            self.compact -= step;
        }
    }

    fn pose(&self) -> Pose {
        let ease = smooth(self.compact);
        Pose {
            x: self.origin_dip,
            y: self.origin_dip,
            w: lerp(self.card_dip_w, COMPACT_W_DIP.min(self.card_dip_w), ease),
            h: lerp(self.card_dip_h, COMPACT_H_DIP, ease),
            radius: lerp(CORNER_RADIUS_DIP, COMPACT_H_DIP * 0.5, ease),
            ease,
        }
    }

    fn ensure_shadow(&mut self, pose: Pose) -> Result<()> {
        let key = (
            (pose.x * 4.0) as i32,
            (pose.y * 4.0) as i32,
            (pose.w * 4.0) as i32,
            (pose.h * 4.0) as i32,
            (pose.radius * 4.0) as i32,
        );
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
            return Hit::Drag;
        }
        Hit::Pill
    }

    pub fn mouse_down(&mut self, x: i32, y: i32) -> bool {
        match self.hit(x, y) {
            Hit::Chrome(1) => {
                self.press = Some((x, y, Press::Minimize));
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
            Press::Peek => {
                if !moved {
                    self.reveal_dock();
                }
            }
        }
    }

    fn draw_card(&mut self, frame: &Frame) -> Result<()> {
        let pose = self.pose();
        let factory = unsafe { self.d2d.GetFactory() }?;
        let mask = D2D1_ROUNDED_RECT {
            rect: d2d_rect(pose.x, pose.y, pose.x + pose.w, pose.y + pose.h),
            radiusX: pose.radius,
            radiusY: pose.radius,
        };
        self.clip = unsafe { factory.CreateRoundedRectangleGeometry(&mask)?.cast() }?;
        let (ox, oy) = (pose.x, pose.y);
        let (card_w, card_h) = (self.card_dip_w, self.card_dip_h);
        let ease = pose.ease;
        // The footer and the corner buttons leave in the first part of the fold,
        // before the card edge clips them.
        let body = (1.0 - ease * 1.45).clamp(0.0, 1.0);
        unsafe {
            self.d2d.BeginDraw();
            let clear = rgba(0.0, 0.0, 0.0, 0.0);
            self.d2d.Clear(Some(&clear));

            let window = d2d_rect(0.0, 0.0, self.origin_dip * 2.0 + card_w, self.origin_dip * 2.0 + card_h);
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
            let backdrop = d2d_rect(pose.x, pose.y, pose.x + pose.w, pose.y + pose.h);
            let src_w = pose.w.min(card_w);
            let src_h = pose.h.min(card_h);
            let src_x = (card_w - src_w) * 0.5;
            let src_y = (card_h - src_h) * 0.5;
            let source = d2d_rect(src_x, src_y, src_x + src_w, src_y + src_h);
            self.d2d.DrawBitmap(
                &self.scene_bitmap,
                Some(&backdrop),
                1.0,
                D2D1_INTERPOLATION_MODE_LINEAR,
                Some(&source),
                None,
            );

            let white = brush(&self.d2d, rgba(1.0, 1.0, 1.0, 1.0))?;
            let muted = brush(&self.d2d, rgba(0.84, 0.88, 0.93, 1.0))?;
            let faded = brush(&self.d2d, rgba(0.84, 0.88, 0.93, body))?;
            let accent = brush(&self.d2d, rgba(0.23, 0.51, 0.96, body))?;
            let track = brush(&self.d2d, rgba(0.15, 0.20, 0.27, body))?;

            // The header row slides into the vertical centre of the pill; the
            // countdown shrinks and joins it on the same line.
            let e = ease;
            let icon_x = ox + PAD_X_DIP;
            let icon_y = lerp(
                oy + PAD_Y_DIP + (LABEL_H_DIP - ICON_DIP) * 0.5,
                oy + (COMPACT_H_DIP - ICON_DIP) * 0.5,
                e,
            );
            draw_lucide_clock(&self.d2d, &muted, &self.stroke, icon_x, icon_y, ICON_DIP);
            let label_x = icon_x + ICON_DIP + 6.0;
            let label_y = lerp(oy + PAD_Y_DIP, oy + (COMPACT_H_DIP - LABEL_H_DIP) * 0.5, e);
            draw_text(
                &self.d2d, &self.text_label, &muted, &frame.label,
                label_x, label_y, ox + card_w - PAD_X_DIP, label_y + LABEL_H_DIP,
            )?;

            let hover_index = self.hover_index();
            if body > 0.02 {
                let hover = brush(&self.d2d, rgba(0.0, 0.0, 0.0, 0.22 * body))?;
                for (index, (bx, by)) in chrome_slots(card_w).into_iter().enumerate() {
                    let (bx, by) = (ox + bx, oy + by);
                    if hover_index == index as i32 {
                        let chip = D2D1_ROUNDED_RECT {
                            rect: d2d_rect(bx, by, bx + CHROME_BOX_DIP, by + CHROME_BOX_DIP),
                            radiusX: CHROME_RADIUS_DIP,
                            radiusY: CHROME_RADIUS_DIP,
                        };
                        self.d2d.FillRoundedRectangle(&chip, &hover);
                    }
                    let inset = (CHROME_BOX_DIP - ICON_DIP) * 0.5;
                    draw_chrome_icon(&self.d2d, &faded, &self.stroke, &self.gear, index, bx + inset, by + inset, ICON_DIP);
                }
            }

            let footer_h = 16.0;
            let bar_h = 4.0;
            let bar_gap = 8.0;
            let footer_bottom = oy + card_h - PAD_Y_DIP;
            let footer_top = footer_bottom - footer_h;
            let bar_y = footer_top - bar_gap - bar_h;
            let time_top_exp = oy + PAD_Y_DIP + LABEL_H_DIP + 2.0;
            let time_h_exp = (bar_y - 6.0) - time_top_exp;
            let time_x = lerp(ox + PAD_X_DIP, label_x + LABEL_BLOCK_DIP, e);
            let time_y = lerp(time_top_exp, oy, e);
            let time_h = lerp(time_h_exp, COMPACT_H_DIP, e);
            let time_size = lerp(TIME_EXPANDED_DIP, TIME_COMPACT_DIP, e);
            draw_text_sized(
                &self.d2d, &self.dwrite, &self.text_time, &white, &frame.time_text,
                time_x, time_y, card_w, time_h, time_size,
            )?;

            if body > 0.02 {
                let bar_w = card_w - PAD_X_DIP * 2.0;
                let track_rect = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(ox + PAD_X_DIP, bar_y, ox + PAD_X_DIP + bar_w, bar_y + bar_h),
                    radiusX: 2.0,
                    radiusY: 2.0,
                };
                self.d2d.FillRoundedRectangle(&track_rect, &track);
                let filled = D2D1_ROUNDED_RECT {
                    rect: d2d_rect(
                        ox + PAD_X_DIP, bar_y,
                        ox + PAD_X_DIP + bar_w * frame.progress.clamp(0.0, 1.0),
                        bar_y + bar_h,
                    ),
                    radiusX: 2.0,
                    radiusY: 2.0,
                };
                self.d2d.FillRoundedRectangle(&filled, &accent);
                draw_text(&self.d2d, &self.text_footer, &faded, &frame.footer_left, ox + PAD_X_DIP, footer_top, ox + card_w - PAD_X_DIP, footer_bottom)?;
                draw_text(&self.d2d, &self.text_footer_right, &faded, &frame.footer_right, ox + PAD_X_DIP, footer_top, ox + card_w - PAD_X_DIP, footer_bottom)?;
            }

            self.d2d.PopLayer();
            core::mem::ManuallyDrop::drop(&mut layer.geometricMask);
            core::mem::ManuallyDrop::drop(&mut layer.opacityBrush);
            self.d2d.EndDraw(None, None)?;
        }
        Ok(())
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
        self.drag_cursor = cursor;
        self.drag_tucked = self.tucked;
        if let Some(dock) = self.dock.as_mut() {
            dock.suspended = true;
        }
        self.tucked = false;
    }

    pub fn end_window_drag(
        &mut self,
        cursor: POINT,
        card: RECT,
        work: RECT,
        monitor: HMONITOR,
        outer: Edges,
    ) {
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
        self.drop_card(card, work, monitor, outer);
    }

    /// Pull a card that no longer intersects any work area back to a peek.
    pub fn rescue(&mut self, card: RECT, work: RECT, monitor: HMONITOR, outer: Edges) {
        self.drag_tucked = false;
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
        let dock = self.dock.as_mut().unwrap();
        dock.work = work;
        dock.tick(dt, hover, card_w, card_h, peek);
        let origin = dock.origin(card_w, card_h, peek);
        dock.shown = origin;
        let tucked = dock.is_tucked(card_w, card_h);
        self.tucked = tucked;
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
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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

fn make_format(
    factory: &IDWriteFactory,
    size: f32,
    weight: DWRITE_FONT_WEIGHT,
    alignment: DWRITE_TEXT_ALIGNMENT,
    paragraph: DWRITE_PARAGRAPH_ALIGNMENT,
) -> Result<IDWriteTextFormat> {
    unsafe {
        let format = factory.CreateTextFormat(
            TEXT_FAMILY,
            None,
            weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            windows::core::w!("zh-cn"),
        )?;
        format.SetTextAlignment(alignment)?;
        format.SetParagraphAlignment(paragraph)?;
        Ok(format)
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