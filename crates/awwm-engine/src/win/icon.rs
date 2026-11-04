//! Extracts an executable's icon and encodes it as a PNG data URL for the UI.

use base64::Engine as _;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::PCWSTR;

/// Owned icon handle destroyed on drop.
struct Icon(HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: the icon was created by ExtractIconExW for this process and is destroyed
        // exactly once.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

/// Owned GDI bitmap deleted on drop.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: bitmaps from GetIconInfo are owned by the caller and deleted once.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.0.0));
            }
        }
    }
}

/// Returns `data:image/png;base64,…` for the large icon of an executable, or `None` when
/// the file has no icon or cannot be read.
#[must_use]
pub fn exe_icon_data_url(exe_path: &str) -> Option<String> {
    if exe_path.is_empty() {
        return None;
    }
    let path = super::wide(exe_path);
    let mut large = HICON::default();
    // SAFETY: `path` is NUL-terminated and outlives the call; `large` is a valid out ptr.
    let n = unsafe { ExtractIconExW(PCWSTR(path.as_ptr()), 0, Some(&raw mut large), None, 1) };
    if n == 0 || large.is_invalid() {
        return None;
    }
    let icon = Icon(large);
    let (width, height, rgba) = icon_rgba(&icon)?;
    let png = encode_png(width, height, &rgba)?;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    ))
}

fn icon_rgba(icon: &Icon) -> Option<(u32, u32, Vec<u8>)> {
    let mut info = ICONINFO::default();
    // SAFETY: `info` is a valid out struct; the returned bitmaps are owned by us.
    unsafe { GetIconInfo(icon.0, &raw mut info) }.ok()?;
    let color = Bitmap(info.hbmColor);
    let mask = Bitmap(info.hbmMask);
    if color.0.is_invalid() {
        // Monochrome icons are rare for applications; the UI shows a generic glyph.
        return None;
    }
    let mut bm = BITMAP::default();
    // SAFETY: `bm` is a writable BITMAP of the size passed.
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(color.0.0),
            i32::try_from(std::mem::size_of::<BITMAP>()).unwrap_or(32),
            Some((&raw mut bm).cast()),
        )
    };
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 || bm.bmWidth > 256 || bm.bmHeight > 256 {
        return None;
    }
    let (w, h) = (bm.bmWidth, bm.bmHeight);
    let mut bgra = read_bitmap_32(color.0, w, h)?;
    if bgra.as_chunks::<4>().0.iter().all(|px| px[3] == 0) {
        // Icon without alpha channel: derive transparency from the AND mask.
        let mask_bits = read_bitmap_32(mask.0, w, h)?;
        for (px, m) in bgra
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(mask_bits.as_chunks::<4>().0)
        {
            px[3] = if m[0] == 0 { 255 } else { 0 };
        }
    }
    let mut rgba = bgra;
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Some((w.unsigned_abs(), h.unsigned_abs(), rgba))
}

fn read_bitmap_32(bitmap: HBITMAP, width: i32, height: i32) -> Option<Vec<u8>> {
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: u32::try_from(std::mem::size_of::<BITMAPINFOHEADER>()).unwrap_or(40),
            biWidth: width,
            biHeight: -height, // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let len = usize::try_from(width).ok()? * usize::try_from(height).ok()? * 4;
    let mut buf = vec![0u8; len];
    // SAFETY: a memory DC is created and deleted here; `buf` holds width*height*4 bytes,
    // exactly what a 32-bpp top-down DIB of this size needs.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height.unsigned_abs(),
            Some(buf.as_mut_ptr().cast()),
            &raw mut bmi,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        if lines == 0 {
            return None;
        }
    }
    Some(buf)
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(rgba.len() / 2);
    {
        let mut encoder = png::Encoder::new(&mut out, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notepad_icon_is_a_png_data_url() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_owned());
        let url = exe_icon_data_url(&format!(r"{windir}\System32\notepad.exe"))
            .or_else(|| exe_icon_data_url(&format!(r"{windir}\explorer.exe")))
            .expect("icon");
        assert!(url.starts_with("data:image/png;base64,"));
        assert!(url.len() > 100);
    }

    #[test]
    fn missing_file_has_no_icon() {
        assert!(exe_icon_data_url(r"C:\definitely\missing\file.exe").is_none());
        assert!(exe_icon_data_url("").is_none());
    }
}
