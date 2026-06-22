//! Tiny dependency-free image encoding for the preview.
//!
//! The preview frame is handed to the webview as a 24-bit BMP, which every
//! browser renders directly from a `data:image/bmp;base64,…` URL — so the GUI
//! needs no PNG/JPEG dependency to show a composited frame.

/// Encodes tightly-packed RGBA8 (`w * h * 4` bytes) as a 24-bit BMP byte vector.
///
/// Bottom-up, BGR, rows padded to a 4-byte boundary — the canonical BMP layout.
pub fn rgba_to_bmp(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let row = (w * 3).div_ceil(4) * 4;
    let img_size = row * h;
    let file_size = 54 + img_size;
    let mut f = Vec::with_capacity(file_size as usize);

    f.extend_from_slice(b"BM");
    f.extend_from_slice(&file_size.to_le_bytes());
    f.extend_from_slice(&0u32.to_le_bytes()); // reserved
    f.extend_from_slice(&54u32.to_le_bytes()); // pixel data offset
    f.extend_from_slice(&40u32.to_le_bytes()); // DIB header size
    f.extend_from_slice(&(w as i32).to_le_bytes());
    f.extend_from_slice(&(h as i32).to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes()); // planes
    f.extend_from_slice(&24u16.to_le_bytes()); // bpp
    f.extend_from_slice(&0u32.to_le_bytes()); // compression (none)
    f.extend_from_slice(&img_size.to_le_bytes());
    f.extend_from_slice(&2835i32.to_le_bytes()); // x ppm (~72 dpi)
    f.extend_from_slice(&2835i32.to_le_bytes()); // y ppm
    f.extend_from_slice(&0u32.to_le_bytes()); // palette colors
    f.extend_from_slice(&0u32.to_le_bytes()); // important colors

    let pad = (row - w * 3) as usize;
    for y in (0..h).rev() {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if i + 2 < rgba.len() {
                f.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i]]); // BGR
            } else {
                f.extend_from_slice(&[0, 0, 0]);
            }
        }
        f.extend(std::iter::repeat_n(0u8, pad));
    }
    f
}
