use std::cell::RefCell;
use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::slice;

use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::BI_BITFIELDS;
use windows::Win32::Graphics::Gdi::BITMAPV4HEADER;
use windows::Win32::Graphics::Gdi::BitBlt;
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

#[derive(Debug)]
pub struct DynamicCapture {
    capture: RefCell<Capture>,
}

impl DynamicCapture {
    pub fn new(handle: Handle) -> Result<Self, Error> {
        Ok(Self {
            capture: RefCell::new(Capture::new(handle, 1, 1)?),
        })
    }

    pub fn grab(&self) -> Result<Frame, Error> {
        let result = self.capture.borrow().grab();
        match result {
            Ok(_) => result,
            Err(ref err) => match err {
                Error::InvalidWindowSize(width, height) => {
                    let capture = Capture::new_from(&self.capture.borrow(), *width, *height)?;
                    self.capture.replace(capture);
                    self.capture.borrow().grab()
                }
                _ => result,
            },
        }
    }
}

#[derive(Debug)]
struct BorrowedDeviceContext {
    inner: HDC,
    handle: HWND,
}

impl Drop for BorrowedDeviceContext {
    fn drop(&mut self) {
        let _ = unsafe { ReleaseDC(self.handle.into(), self.inner) };
    }
}

#[derive(Debug)]
struct OwnedDeviceContext {
    inner: HDC,
}

impl Drop for OwnedDeviceContext {
    fn drop(&mut self) {
        let _ = unsafe {
            let _ = DeleteDC(self.inner);
        };
    }
}

#[derive(Debug)]
struct Bitmap {
    inner: HBITMAP,
    width: i32,
    height: i32,
    size: usize,
    buffer: *const u8,
}

impl Drop for Bitmap {
    fn drop(&mut self) {
        let _ = unsafe { DeleteObject(self.inner.into()) };
    }
}

#[derive(Debug)]
pub struct Capture {
    handle: Handle,
    dc: OwnedDeviceContext,
    bm: Bitmap,
}

impl Capture {
    pub fn new(handle: Handle, width: i32, height: i32) -> Result<Self, Error> {
        let dc = create_dc()?;
        let bm = create_bitmap(dc.inner, width, height)?;
        Ok(Self { handle, dc, bm })
    }

    fn new_from(from: &Capture, width: i32, height: i32) -> Result<Self, Error> {
        let handle = from.handle.clone();
        let dc = create_dc()?;
        let bm = create_bitmap(dc.inner, width, height)?;
        Ok(Self { handle, dc, bm })
    }

    pub fn grab(&self) -> Result<Frame, Error> {
        let handle = self.handle.to_inner()?;
        let handle_dc = get_dc(handle)?;
        let rect = get_rect(handle)?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width != self.bm.width || height != self.bm.height {
            return Err(Error::InvalidWindowSize(width, height));
        }
        let result = unsafe {
            let obj = SelectObject(self.dc.inner, self.bm.inner.into());
            if obj.is_invalid() {
                return Err(Error::from_last_win_error());
            }
            let result = BitBlt(
                self.dc.inner,
                0,
                0,
                self.bm.width as i32,
                self.bm.height as i32,
                handle_dc.inner.into(),
                rect.left,
                rect.top,
                SRCCOPY,
            );
            let _ = SelectObject(self.dc.inner, obj);
            result
        };
        result.map_err(Error::from).map(|_| {
            let ptr = unsafe { slice::from_raw_parts(self.bm.buffer, self.bm.size) };
            let vec = ptr.to_vec();
            Frame {
                width: self.bm.width,
                height: self.bm.height,
                data: vec,
            }
        })
    }
}

#[inline(always)]
fn get_rect(handle: HWND) -> Result<RECT, Error> {
    let mut rect = RECT::default();
    let _ = unsafe { GetClientRect(handle, &raw mut rect) }?;
    Ok(rect)
}

#[inline(always)]
fn get_dc(handle: HWND) -> Result<BorrowedDeviceContext, Error> {
    let inner = unsafe { GetDC(handle.into()) };
    if inner.is_invalid() {
        return Err(unsafe { Error::from_last_win_error() });
    }
    Ok(BorrowedDeviceContext { inner, handle })
}

fn create_dc() -> Result<OwnedDeviceContext, Error> {
    let inner = unsafe { CreateCompatibleDC(None) };
    if inner.is_invalid() {
        return Err(unsafe { Error::from_last_win_error() });
    }
    Ok(OwnedDeviceContext { inner })
}

fn create_bitmap(dc: HDC, width: i32, height: i32) -> Result<Bitmap, Error> {
    let size = width as usize * height as usize * 4;
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

    let inner = unsafe {
        CreateDIBSection(
            dc.into(),
            (&raw const info).cast(),
            DIB_RGB_COLORS,
            &raw mut buffer,
            None,
            0,
        )?
    };
    return Ok(Bitmap {
        inner,
        width,
        height,
        size,
        buffer: buffer.cast(),
    });
}
