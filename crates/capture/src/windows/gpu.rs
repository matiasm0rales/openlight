//! Direct3D 11: el dispositivo para Graphics Capture y la reducción del cuadro en la GPU.
//!
//! La reducción copia el cuadro al nivel 0 de una textura con mipmaps, deja que la GPU genere
//! los niveles (`GenerateMips`, promedio 2×2 por nivel) y lee a la CPU solo el nivel que queda
//! apenas por encima del tamaño pedido (por ejemplo, 120×67 para una pantalla 4K y 96×54). De
//! una pantalla 4K se leen ~32 KB por cuadro en lugar de 33 MB.

use ::windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use ::windows::Win32::Foundation::HMODULE;
use ::windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use ::windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D,
};
use ::windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_SAMPLE_DESC,
};
use ::windows::Win32::Graphics::Dxgi::IDXGIDevice;
use ::windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
use ::windows::core::{Interface, Result};

use super::convert::{box_resample_bgra8, mip_level_for, mip_size, rgba16f_to_bgra8};

/// Dispositivo D3D11 y su versión WinRT para el frame pool.
pub(crate) struct Gpu {
    pub device: ID3D11Device,
    pub context: ID3D11DeviceContext,
    pub winrt_device: IDirect3DDevice,
}

impl Gpu {
    /// Crea el dispositivo en la GPU por defecto; si no hay (máquinas virtuales, runners de CI),
    /// usa WARP, el rasterizador por software de Windows.
    pub fn new() -> Result<Self> {
        let (device, context) =
            create_device(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| create_device(D3D_DRIVER_TYPE_WARP))?;
        let dxgi: IDXGIDevice = device.cast()?;
        // SAFETY: `dxgi` es un IDXGIDevice válido recién obtenido del dispositivo.
        let winrt_device: IDirect3DDevice =
            unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi)? }.cast()?;
        Ok(Self {
            device,
            context,
            winrt_device,
        })
    }
}

fn create_device(kind: D3D_DRIVER_TYPE) -> Result<(ID3D11Device, ID3D11DeviceContext)> {
    let mut device = None;
    let mut context = None;
    // SAFETY: punteros a `Option`s locales; los parámetros siguen la documentación de
    // D3D11CreateDevice (sin adaptador explícito, sin módulo de software).
    unsafe {
        D3D11CreateDevice(
            None,
            kind,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
    }
    match (device, context) {
        (Some(d), Some(c)) => Ok((d, c)),
        _ => Err(::windows::core::Error::from_hresult(
            ::windows::Win32::Foundation::E_FAIL,
        )),
    }
}

/// Formato de píxel de la captura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PixelFormat {
    /// SDR: BGRA de 8 bits, sRGB.
    Bgra8,
    /// HDR / color avanzado: RGBA half float, scRGB lineal.
    Rgba16f,
}

impl PixelFormat {
    fn dxgi(self) -> DXGI_FORMAT {
        match self {
            Self::Bgra8 => DXGI_FORMAT_B8G8R8A8_UNORM,
            Self::Rgba16f => DXGI_FORMAT_R16G16B16A16_FLOAT,
        }
    }

    fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Bgra8 => 4,
            Self::Rgba16f => 8,
        }
    }
}

/// Texturas para reducir cuadros de un tamaño y un formato fijos.
pub(crate) struct Downscaler {
    format: PixelFormat,
    src: (u32, u32),
    target: (u32, u32),
    level: u32,
    level_size: (u32, u32),
    mips: ID3D11Texture2D,
    srv: ID3D11ShaderResourceView,
    staging: ID3D11Texture2D,
}

impl Downscaler {
    pub fn new(gpu: &Gpu, format: PixelFormat, src: (u32, u32), target: (u32, u32)) -> Result<Self> {
        let level = mip_level_for(src, target);
        let level_size = mip_size(src, level);
        let mips_desc = D3D11_TEXTURE2D_DESC {
            Width: src.0,
            Height: src.1,
            MipLevels: level + 1,
            ArraySize: 1,
            Format: format.dxgi(),
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        };
        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: level_size.0,
            Height: level_size.1,
            MipLevels: 1,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
            ..mips_desc
        };
        let mut mips = None;
        let mut staging = None;
        let mut srv = None;
        // SAFETY: descripciones válidas armadas arriba; los punteros de salida son `Option`s
        // locales.
        unsafe {
            gpu.device.CreateTexture2D(&mips_desc, None, Some(&mut mips))?;
            gpu.device.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        }
        let mips = mips.ok_or_else(fail)?;
        let staging = staging.ok_or_else(fail)?;
        // SAFETY: `mips` es una textura con SHADER_RESOURCE; sin descripción, la vista cubre
        // todos los niveles.
        unsafe { gpu.device.CreateShaderResourceView(&mips, None, Some(&mut srv))? };
        Ok(Self {
            format,
            src,
            target,
            level,
            level_size,
            mips,
            srv: srv.ok_or_else(fail)?,
            staging,
        })
    }

    /// Si sirve para cuadros de este formato y tamaño.
    pub fn matches(&self, format: PixelFormat, src: (u32, u32), target: (u32, u32)) -> bool {
        self.format == format && self.src == src && self.target == target
    }

    /// Reduce `frame` (una textura de la captura, del formato de este reductor y al menos del
    /// tamaño `src`) y devuelve el BGRA de 8 bits de `target`, sin relleno entre filas.
    pub fn process(&self, gpu: &Gpu, frame: &ID3D11Texture2D, white_scale: f32) -> Result<Vec<u8>> {
        let region = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: self.src.0,
            bottom: self.src.1,
            back: 1,
        };
        let ctx = &gpu.context;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: las texturas son del mismo formato; `region` está dentro de las dos (el frame
        // pool tiene el tamaño `src`); `level` existe en `mips` y su tamaño es el de `staging`.
        unsafe {
            ctx.CopySubresourceRegion(&self.mips, 0, 0, 0, 0, frame, 0, Some(&region));
            ctx.GenerateMips(&self.srv);
            ctx.CopySubresourceRegion(&self.staging, 0, 0, 0, 0, &self.mips, self.level, None);
            ctx.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        }
        let (w, h) = self.level_size;
        let pitch = mapped.RowPitch as usize;
        let row = w as usize * self.format.bytes_per_pixel();
        let len = pitch * (h as usize - 1) + row;
        // SAFETY: mientras el recurso está mapeado, `pData` apunta a `h` filas de `RowPitch`
        // bytes (la última puede no tener relleno, por eso `len` termina en `row`).
        let data = unsafe { std::slice::from_raw_parts(mapped.pData as *const u8, len) };
        let out = match self.format {
            PixelFormat::Bgra8 => box_resample_bgra8(data, (w, h), pitch, self.target),
            PixelFormat::Rgba16f => {
                let sdr = rgba16f_to_bgra8(data, w, h, pitch, white_scale);
                box_resample_bgra8(&sdr, (w, h), w as usize * 4, self.target)
            }
        };
        // SAFETY: se mapeó arriba y `data` no se usa más.
        unsafe { ctx.Unmap(&self.staging, 0) };
        Ok(out)
    }
}

fn fail() -> ::windows::core::Error {
    ::windows::core::Error::from_hresult(::windows::Win32::Foundation::E_FAIL)
}
