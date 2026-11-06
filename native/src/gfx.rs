// Direct3D 11 draws the shader background straight into the swap chain's back
// buffer; a Direct2D 1.1 device context then draws the card on top of it, and
// DirectWrite lays the text out. One device, one swap chain, one present.

use windows::core::{Interface, Result, HSTRING, PCSTR};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, D2D1_ROUNDED_RECT, ID2D1Bitmap1, ID2D1Brush, ID2D1Device, ID2D1DeviceContext,
    ID2D1Factory1, ID2D1SolidColorBrush, D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
    D2D1_BITMAP_OPTIONS_NONE, D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
    D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_INTERPOLATION_MODE_LINEAR,
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
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_IGNORE, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain1, DXGI_PRESENT, DXGI_SCALING_STRETCH,
    DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

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
    width: u32,
    height: u32,
    dpi: f32,
    clock: f32,
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
            let adapter = dxgi_device.GetAdapter()?;
            let factory: IDXGIFactory2 = adapter.GetParent()?;
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
                AlphaMode: DXGI_ALPHA_MODE_IGNORE,
                Flags: 0,
            };
            let swap_chain = factory.CreateSwapChainForHwnd(&device, hwnd, &desc, None, None)?;

            let d2d_factory: ID2D1Factory1 =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let d2d_device: ID2D1Device = d2d_factory.CreateDevice(&dxgi_device)?;
            let d2d: ID2D1DeviceContext =
                d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            // The device context's own DPI decides how a DIP becomes a pixel; it
            // defaults to 96, which would draw everything at 1:1 on a scaled
            // display. It has to match the target bitmaps'.
            d2d.SetDpi(dpi, dpi);

            let pixel_format = D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            };
            let scene_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
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

            // Does this machine have Direct2D's built-in effect graph (the
            // Gaussian blur a frosted card would use)?
            let gaussian = windows::core::GUID::from_u128(0x1feb_6d69_2fe6_4ac9_8c58_1d7f_93e7_a6a5);
            match d2d.CreateEffect(&gaussian) {
                Ok(_) => crate::log("d2d effect: GaussianBlur available"),
                Err(error) => crate::log(&format!("d2d effect: GaussianBlur unavailable ({error})")),
            }

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
                text_time: make_format(&dwrite, 34.0, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_TEXT_ALIGNMENT_LEADING)?,
                text_label: make_format(&dwrite, 13.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING)?,
                text_footer: make_format(&dwrite, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_LEADING)?,
                text_footer_right: make_format(&dwrite, 12.0, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_TEXT_ALIGNMENT_TRAILING)?,
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
                width,
                height,
                dpi,
                clock: 0.0,
            })
        }
    }

    pub fn render(&mut self, frame: &Frame, dt: f32) -> Result<()> {
        if !frame.paused {
            self.clock += dt;
        }
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
                    alphaMode: D2D1_ALPHA_MODE_IGNORE,
                },
                dpiX: self.dpi,
                dpiY: self.dpi,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: Default::default(),
            };
            let target: ID2D1Bitmap1 = self.d2d.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            self.d2d.SetTarget(&target);
            self.draw_card(frame)?;

            // No vsync wait: the loop paces itself, the background only needs
            // about 30 frames a second.
            self.swap_chain.Present(0, DXGI_PRESENT(0)).ok()?;
        }
        Ok(())
    }

    fn draw_shader(&self, view: &ID3D11RenderTargetView, mode: f32) -> Result<()> {
        unsafe {
            self.context.OMSetRenderTargets(Some(&[Some(view.clone())]), None);
            let viewport = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };
            self.context.RSSetViewports(Some(&[viewport]));
            self.context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.context.VSSetShader(&self.vertex_shader, None);

            if mode >= 0.5 {
                // The ported Shadertoy pass: its own uniforms and channels.
                let params = OceanParams {
                    resolution: [self.width as f32, self.height as f32, 1.0],
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
                width: self.width as f32,
                height: self.height as f32,
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

    fn draw_card(&mut self, frame: &Frame) -> Result<()> {
        let scale = self.dpi / 96.0;
        let dip_w = self.width as f32 / scale;
        let dip_h = self.height as f32 / scale;
        unsafe {
            self.d2d.BeginDraw();

            let backdrop = d2d_rect(0.0, 0.0, dip_w, dip_h);
            self.d2d.DrawBitmap(
                &self.scene_bitmap,
                Some(&backdrop),
                1.0,
                D2D1_INTERPOLATION_MODE_LINEAR,
                None,
                None,
            );

            let white = brush(&self.d2d, rgba(1.0, 1.0, 1.0, 1.0))?;
            let muted = brush(&self.d2d, rgba(0.84, 0.88, 0.93, 1.0))?;
            let accent = brush(&self.d2d, rgba(0.23, 0.51, 0.96, 1.0))?;
            let track = brush(&self.d2d, rgba(0.15, 0.20, 0.27, 1.0))?;

            let pad = 18.0;
            draw_text(&self.d2d, &self.text_label, &muted, &frame.label, pad, 8.0, dip_w - pad, 30.0)?;
            draw_text(&self.d2d, &self.text_time, &white, &frame.time_text, pad - 2.0, 26.0, dip_w - pad, 84.0)?;

            let bar_y = dip_h - 34.0;
            let bar_w = dip_w - pad * 2.0;
            let track_rect = D2D1_ROUNDED_RECT {
                rect: d2d_rect(pad, bar_y, pad + bar_w, bar_y + 4.0),
                radiusX: 2.0,
                radiusY: 2.0,
            };
            self.d2d.FillRoundedRectangle(&track_rect, &track);
            let filled = D2D1_ROUNDED_RECT {
                rect: d2d_rect(pad, bar_y, pad + bar_w * frame.progress.clamp(0.0, 1.0), bar_y + 4.0),
                radiusX: 2.0,
                radiusY: 2.0,
            };
            self.d2d.FillRoundedRectangle(&filled, &accent);

            draw_text(&self.d2d, &self.text_footer, &muted, &frame.footer_left, pad, dip_h - 24.0, dip_w - pad, dip_h - 6.0)?;
            draw_text(&self.d2d, &self.text_footer_right, &muted, &frame.footer_right, pad, dip_h - 24.0, dip_w - pad, dip_h - 6.0)?;

            self.d2d.EndDraw(None, None)?;
        }
        Ok(())
    }
}

fn brush(target: &ID2D1DeviceContext, color: D2D1_COLOR_F) -> Result<ID2D1SolidColorBrush> {
    unsafe { target.CreateSolidColorBrush(&color, None) }
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
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
        Ok(format)
    }
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