use std::mem;

use windows::Win32::Graphics::Gdi::{BITMAPFILEHEADER, BITMAPINFOHEADER};

pub fn rgb_to_bitmap(data: Vec<u8>, width: i32, height: i32) -> Vec<u8> {
    let info_header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biSizeImage: (width * height * 4) as u32,
        ..BITMAPINFOHEADER::default()
    };
    let offset = size_of::<BITMAPINFOHEADER>() as u32 + size_of::<BITMAPFILEHEADER>() as u32;
    let file_header = BITMAPFILEHEADER {
        bfType: 0x4D42,
        bfSize: offset + info_header.biSizeImage,
        bfOffBits: offset,
        ..BITMAPFILEHEADER::default()
    };
    // SAFETY: both Src and Dst type sizes are the same
    let file_header = unsafe {
        mem::transmute::<BITMAPFILEHEADER, [u8; size_of::<BITMAPFILEHEADER>()]>(file_header)
    };
    // SAFETY: both Src and Dst type sizes are the same
    let info_header = unsafe {
        mem::transmute::<BITMAPINFOHEADER, [u8; size_of::<BITMAPINFOHEADER>()]>(info_header)
    };
    file_header
        .into_iter()
        .chain(info_header)
        .chain(data)
        .collect()
}
