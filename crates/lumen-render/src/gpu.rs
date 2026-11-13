//! The shared graphics device.

use std::rc::Rc;

use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct2D::{D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1CreateFactory, ID2D1Device, ID2D1Factory1},
            Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::{D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice, ID3D11Device},
            DirectComposition::{DCompositionCreateDevice, IDCompositionDevice},
            DirectWrite::{
                DWRITE_CONTAINER_TYPE_UNKNOWN, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC, IDWriteFactory6, DWriteCreateFactory, IDWriteFactory5, IDWriteFontCollection, IDWriteFontCollection1,
                IDWriteInMemoryFontFileLoader,
            },
            Dxgi::{IDXGIDevice, IDXGIFactory2},
        },
    },
    core::{HSTRING, Interface},
};

use crate::{Context, Result};

/// One Direct3D/Direct2D/DirectWrite/DirectComposition stack, shared by every surface.
pub struct Gpu {
    pub(crate) d3d: ID3D11Device,
    pub(crate) dxgi_factory: IDXGIFactory2,
    pub(crate) d2d: ID2D1Device,
    pub(crate) dwrite: IDWriteFactory5,
    pub(crate) composition: IDCompositionDevice,
    /// Fonts shipped with Lumen, usable without being installed in Windows.
    bundled_fonts: Option<IDWriteFontCollection1>,
    // Keeps the bundled font data registered for as long as the factory uses it.
    font_loader: Option<IDWriteInMemoryFontFileLoader>,
}

impl Gpu {
    /// Creates the device. `fonts` are font files (TTF, OTF, WOFF or WOFF2) to
    /// make available by family name; a font that cannot be loaded is skipped.
    pub fn new(fonts: &[&[u8]]) -> Result<Rc<Self>> {
        unsafe {
            // A software device still works where there is no usable GPU (remote sessions, VMs).
            let d3d = create_device(D3D_DRIVER_TYPE_HARDWARE).or_else(|_| create_device(D3D_DRIVER_TYPE_WARP))?;
            let dxgi_device: IDXGIDevice = d3d.cast().context("getting the DXGI device")?;
            let dxgi_factory: IDXGIFactory2 = dxgi_device.GetAdapter().context("getting the adapter")?.GetParent().context("getting the DXGI factory")?;

            let d2d_factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).context("starting Direct2D")?;
            let d2d = d2d_factory.CreateDevice(&dxgi_device).context("creating the Direct2D device")?;
            let dwrite: IDWriteFactory5 = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).context("starting DirectWrite")?;
            let composition: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device).context("starting DirectComposition")?;

            let (bundled_fonts, font_loader) = match load_fonts(&dwrite, fonts) {
                Ok((collection, loader)) => (Some(collection), Some(loader)),
                Err(_) => (None, None),
            };
            Ok(Rc::new(Self { d3d, dxgi_factory, d2d, dwrite, composition, bundled_fonts, font_loader }))
        }
    }

    /// Where to find `family`: the bundled collection and the name the
    /// family has there, if it is bundled; otherwise `None` and the name as
    /// given, meaning the fonts installed in Windows.
    pub(crate) fn font_for(&self, family: &str) -> (Option<IDWriteFontCollection>, String) {
        let found = self.bundled_fonts.as_ref().and_then(|bundled| {
            let collection: IDWriteFontCollection = bundled.cast().ok()?;
            let has = |name: &str| {
                let (mut index, mut exists) = (0u32, false.into());
                unsafe { collection.FindFamilyName(&HSTRING::from(name), &mut index, &mut exists) }.is_ok() && exists.as_bool()
            };
            // Some font files name their family after one weight ("Space Grotesk
            // Light"); asking for the plain name must still find them.
            let name = if has(family) { family.to_owned() } else { crate::fonts::file_names_for(&collection, family).into_iter().next()? };
            Some((collection, name))
        });
        match found {
            Some((collection, name)) => (Some(collection), name),
            None => (None, family.to_owned()),
        }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        if let Some(loader) = &self.font_loader {
            unsafe {
                let _ = self.dwrite.UnregisterFontFileLoader(loader);
            }
        }
    }
}

unsafe fn create_device(driver: D3D_DRIVER_TYPE) -> Result<ID3D11Device> {
    let mut device = None;
    unsafe { D3D11CreateDevice(None, driver, HMODULE::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, None) }
        .context("creating the Direct3D device")?;
    Ok(device.expect("D3D11CreateDevice succeeded without a device"))
}

/// Builds a private font collection from in-memory font files.
pub(crate) unsafe fn load_fonts(dwrite: &IDWriteFactory5, fonts: &[&[u8]]) -> windows::core::Result<(IDWriteFontCollection1, IDWriteInMemoryFontFileLoader)> {
    unsafe {
        let loader = dwrite.CreateInMemoryFontFileLoader()?;
        dwrite.RegisterFontFileLoader(&loader)?;
        let builder = dwrite.CreateFontSetBuilder()?;

        for data in fonts {
            let Ok(bytes) = unpack(dwrite, data) else { continue };
            // With no owner object the loader keeps its own copy of the bytes.
            if let Ok(file) = loader.CreateInMemoryFontFileReference(dwrite, bytes.as_ptr().cast(), bytes.len() as u32, None) {
                let _ = builder.AddFontFile(&file);
            }
        }
        let set = builder.CreateFontSet()?;
        // Group the files by their *typographic* family, so the weights of
        // "Space Grotesk" are one family of that name. (The older grouping
        // names a family after its lightest file: "Space Grotesk Light".)
        let typographic = dwrite
            .cast::<IDWriteFactory6>()
            .and_then(|factory| factory.CreateFontCollectionFromFontSet(&set, DWRITE_FONT_FAMILY_MODEL_TYPOGRAPHIC))
            .and_then(|collection| collection.cast::<IDWriteFontCollection1>());
        let collection = match typographic {
            Ok(collection) => collection,
            Err(_) => dwrite.CreateFontCollectionFromFontSet(&set)?,
        };
        Ok((collection, loader))
    }
}

/// WOFF and WOFF2 are compressed wrappers; DirectWrite needs the font inside.
unsafe fn unpack(dwrite: &IDWriteFactory5, data: &[u8]) -> windows::core::Result<Vec<u8>> {
    unsafe {
        let container = dwrite.AnalyzeContainerType(data.as_ptr().cast(), data.len() as u32);
        if container == DWRITE_CONTAINER_TYPE_UNKNOWN {
            return Ok(data.to_vec());
        }
        let stream = dwrite.UnpackFontFile(container, data.as_ptr().cast(), data.len() as u32)?;
        let size = stream.GetFileSize()?;
        let (mut start, mut context) = (std::ptr::null_mut(), std::ptr::null_mut());
        stream.ReadFileFragment(&mut start, 0, size, &mut context)?;
        let bytes = std::slice::from_raw_parts(start.cast::<u8>(), size as usize).to_vec();
        stream.ReleaseFileFragment(context);
        Ok(bytes)
    }
}
