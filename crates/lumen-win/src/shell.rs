//! Small shell helpers for the launcher: a program's icon, and opening it.

use windows::{
    Win32::{
        Foundation::{RPC_E_CHANGED_MODE, SIZE},
        Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits},
        System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
        UI::{
            Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY, ShellExecuteW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{HSTRING, PCWSTR, w},
};

/// A program's icon as square RGBA pixels (not premultiplied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Icon {
    pub size: u32,
    pub rgba: Vec<u8>,
}

/// Reads the icon Windows shows for a file (a program or a shortcut), at
/// `size` pixels per side. `None` if the file is missing or has no icon.
pub fn file_icon(path: &str, size: u32) -> Option<Icon> {
    unsafe {
        // Needed on this thread for the shell's image service; fine if already done.
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() && hr != RPC_E_CHANGED_MODE {
            return None;
        }
        let item: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(path.replace('/', "\\")), None).ok()?;
        let side = size as i32;
        let bitmap = item.GetImage(SIZE { cx: side, cy: side }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;

        // Read the pixels back as top-down 32-bit BGRA.
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: side,
                biHeight: -side,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bgra = vec![0u8; (size * size * 4) as usize];
        let dc = CreateCompatibleDC(None);
        let rows = GetDIBits(dc, bitmap, 0, size, Some(bgra.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(bitmap.into());
        if rows != side {
            return None;
        }

        // Shell icons come premultiplied; undo that, and swap to RGBA.
        let mut rgba = Vec::with_capacity(bgra.len());
        let has_alpha = bgra.chunks_exact(4).any(|p| p[3] != 0);
        for pixel in bgra.chunks_exact(4) {
            let alpha = if has_alpha { pixel[3] } else { 255 };
            let straight = |c: u8| {
                if alpha == 0 { 0 } else { ((u32::from(c) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8 }
            };
            rgba.extend_from_slice(&[straight(pixel[2]), straight(pixel[1]), straight(pixel[0]), alpha]);
        }
        Some(Icon { size, rgba })
    }
}

/// Opens a file the way double-clicking it would. Returns whether Windows accepted it.
pub fn open(path: &str) -> bool {
    let result = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(path), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL) };
    // ShellExecute reports success as a value above 32.
    result.0 as usize > 32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_program_has_an_icon() {
        let icon = file_icon(r"C:\Windows\notepad.exe", 32).expect("notepad has an icon");
        assert_eq!(icon.size, 32);
        assert_eq!(icon.rgba.len(), 32 * 32 * 4);
        assert!(icon.rgba.chunks_exact(4).any(|p| p[3] > 0), "something is visible");
        assert!(icon.rgba.chunks_exact(4).any(|p| p[3] == 0), "and the corners are see-through");
    }

    #[test]
    fn a_missing_file_has_no_icon() {
        assert_eq!(file_icon(r"C:\no\such\program.exe", 32), None);
    }
}
