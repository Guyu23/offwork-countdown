// The channel textures the ocean shader samples, decoded with Windows Imaging
// Component (no bundled decoders) and uploaded as plain UNORM textures — a
// Shadertoy channel is data, not sRGB colour.

use windows::core::Result;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    ID3D11Device, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
    WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnLoad,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::SHCreateMemStream;

/// A decoded image: BGRA8 pixels, top row first.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Decodes a PNG or JPEG held in memory.
pub fn decode(bytes: &[u8]) -> Result<Image> {
    unsafe {
        // `SHCreateMemStream` copies the bytes into a stream of its own.
        let stream = SHCreateMemStream(Some(bytes))
            .ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))?;

        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)?;
        let frame = decoder.GetFrame(0)?;
        let converter = factory.CreateFormatConverter()?;
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppBGRA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )?;
        let (mut width, mut height) = (0u32, 0u32);
        converter.GetSize(&mut width, &mut height)?;
        let mut pixels = vec![0u8; (width * 4 * height) as usize];
        converter.CopyPixels(std::ptr::null(), width * 4, &mut pixels)?;
        Ok(Image {
            width,
            height,
            pixels,
        })
    }
}

/// Uploads an image with a full mip chain, because the shader samples very
/// negative levels on purpose (Shadertoy's `-100.` trick).
pub fn upload(device: &ID3D11Device, image: &Image) -> Result<ID3D11Texture2D> {
    let mut levels: Vec<Vec<u8>> = vec![image.pixels.clone()];
    let (mut width, mut height) = (image.width, image.height);
    while width > 1 || height > 1 {
        let next = downsample(levels.last().unwrap(), width, height);
        width = (width / 2).max(1);
        height = (height / 2).max(1);
        levels.push(next);
    }

    let desc = D3D11_TEXTURE2D_DESC {
        Width: image.width,
        Height: image.height,
        MipLevels: levels.len() as u32,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };

    let mut data = Vec::with_capacity(levels.len());
    let (mut level_width, mut level_height) = (image.width, image.height);
    for level in &levels {
        data.push(D3D11_SUBRESOURCE_DATA {
            pSysMem: level.as_ptr() as *const core::ffi::c_void,
            SysMemPitch: level_width * 4,
            SysMemSlicePitch: level_width * 4 * level_height,
        });
        level_width = (level_width / 2).max(1);
        level_height = (level_height / 2).max(1);
    }

    let mut texture: Option<ID3D11Texture2D> = None;
    unsafe {
        device.CreateTexture2D(&desc, Some(data.as_ptr()), Some(&mut texture))?;
    }
    texture.ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_FAIL))
}

/// A plain box filter, one level down.
fn downsample(source: &[u8], width: u32, height: u32) -> Vec<u8> {
    let (w, h) = ((width / 2).max(1), (height / 2).max(1));
    let mut out = vec![0u8; (w * h * 4) as usize];
    let row = width as usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            for channel in 0..4usize {
                let at = |px: usize, py: usize| -> u32 {
                    source[((py.min(height as usize - 1)) * row + px.min(width as usize - 1)) * 4
                        + channel] as u32
                };
                let sum = at(x * 2, y * 2)
                    + at(x * 2 + 1, y * 2)
                    + at(x * 2, y * 2 + 1)
                    + at(x * 2 + 1, y * 2 + 1);
                out[(y * w as usize + x) * 4 + channel] = (sum / 4) as u8;
            }
        }
    }
    out
}
