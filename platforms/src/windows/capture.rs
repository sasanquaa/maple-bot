use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::slice;

use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::BI_BITFIELDS;
use windows::Win32::Graphics::Gdi::BITMAPV4HEADER;
use windows::Win32::Graphics::Gdi::HALFTONE;
use windows::Win32::Graphics::Gdi::SetStretchBltMode;
use windows::Win32::Graphics::Gdi::StretchBlt;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, HBITMAP,
    HDC, ReleaseDC, SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

use super::error::Error;
use super::handle::Handle;

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Capture {
    handle: Handle,
    dc: HDC,
    bm: HBITMAP,
    bm_width: i32,
    bm_height: i32,
    bm_size: usize,
    bm_buf: *const u8,
}

impl Capture {
    pub fn new(
        class: Option<&'static str>,
        title: Option<&'static str>,
        width: i32,
        height: i32,
    ) -> Result<Self, Error> {
        let handle = Handle::new(class, title)?;
        let dc = create_dc()?;
        let (bm, bm_buf) = create_bitmap(dc, width, height)?;
        let bm_size = width as usize * height as usize * 4;
        Ok(Self {
            handle,
            dc,
            bm,
            bm_width: width,
            bm_height: height,
            bm_size,
            bm_buf,
        })
    }

    pub fn grab(&self) -> Result<Frame, Error> {
        let handle = self.handle.to_inner()?;
        let handle_dc = get_dc(&self.handle, handle)?;
        let rect = get_rect(handle);
        if rect.is_err() {
            unsafe {
                let _ = ReleaseDC(handle.into(), handle_dc);
            }
            return Err(rect.unwrap_err());
        }
        let rect = rect.expect("unexpected get_rect error");
        let result = unsafe {
            let obj = SelectObject(self.dc, self.bm.into());
            if obj.is_invalid() {
                let _ = ReleaseDC(handle.into(), handle_dc);
                return Err(Error::from_last_win_error());
            }
            let result = StretchBlt(
                self.dc,
                0,
                0,
                self.bm_width as i32,
                self.bm_height as i32,
                handle_dc.into(),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                SRCCOPY,
            )
            .ok();
            let _ = SelectObject(self.dc, obj);
            let _ = ReleaseDC(handle.into(), handle_dc);
            result
        };
        result.map_err(Error::from).map(|_| {
            let ptr = unsafe { slice::from_raw_parts(self.bm_buf, self.bm_size) };
            let vec = ptr.to_vec();
            Frame {
                width: self.bm_width,
                height: self.bm_height,
                data: vec,
            }
        })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bm.into());
        }
        self.dc = HDC::default();
        self.bm = HBITMAP::default();
    }
}

fn get_rect(handle: HWND) -> Result<RECT, Error> {
    let mut rect = RECT::default();
    let _ = unsafe { GetClientRect(handle, &raw mut rect) }?;
    Ok(rect)
}

fn get_dc(handle: &Handle, inner: HWND) -> Result<HDC, Error> {
    let dc = unsafe { GetDC(inner.into()) };
    if dc.is_invalid() {
        handle.reset_inner();
        return Err(unsafe { Error::from_last_win_error() });
    }
    Ok(dc)
}

fn create_dc() -> Result<HDC, Error> {
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return Err(unsafe { Error::from_last_win_error() });
    }
    if unsafe { SetStretchBltMode(dc, HALFTONE) } == 0 {
        unsafe {
            let _ = DeleteDC(dc);
        }
        return Err(unsafe { Error::from_last_win_error() });
    }
    Ok(dc)
}

fn create_bitmap(dc: HDC, width: i32, height: i32) -> Result<(HBITMAP, *const u8), Error> {
    let mut buffer = ptr::null_mut::<c_void>();
    let mut info = BITMAPV4HEADER::default();
    info.bV4Size = mem::size_of::<BITMAPV4HEADER>() as u32;
    info.bV4Width = width;
    info.bV4Height = -height;
    info.bV4Planes = 1;
    info.bV4BitCount = 32;
    info.bV4V4Compression = BI_BITFIELDS;
    info.bV4RedMask = 0x00FF0000;
    info.bV4GreenMask = 0x0000FF00;
    info.bV4BlueMask = 0x000000FF;

    let bitmap = unsafe {
        CreateDIBSection(
            dc.into(),
            (&raw const info).cast(),
            DIB_RGB_COLORS,
            &raw mut buffer,
            None,
            0,
        )?
    };
    return Ok((bitmap, buffer.cast()));
}
