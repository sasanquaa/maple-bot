use std::ffi::c_void;
use std::fmt;
use std::ptr;
use std::slice;

use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::BI_BITFIELDS;
use windows::Win32::Graphics::Gdi::BITMAPV4HEADER;
use windows::Win32::Graphics::Gdi::BitBlt;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, GetDC, HBITMAP, HDC, ReleaseDC,
    SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::Owned;

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
    capture: Capture,
}

impl DynamicCapture {
    pub fn new(handle: Handle) -> Result<Self, Error> {
        Ok(Self {
            capture: Capture::new(handle, 1024, 768)?,
        })
    }

    pub fn grab(&mut self) -> Result<Frame, Error> {
        let result = self.capture.grab();
        match result {
            Ok(_) => result,
            Err(ref error) => {
                if let Error::InvalidWindowSize(width, height) = error {
                    self.capture = Capture::new_from(&self.capture, *width, *height)?;
                    self.capture.grab()
                } else {
                    result
                }
            }
        }
    }
}

struct Borrowed<T> {
    value: T,
    dropper: Box<dyn Fn()>,
}

impl<T: fmt::Debug> fmt::Debug for Borrowed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T> Drop for Borrowed<T> {
    fn drop(&mut self) {
        (self.dropper)()
    }
}

#[derive(Debug)]
struct Bitmap {
    inner: Owned<HBITMAP>,
    width: i32,
    height: i32,
    size: usize,
    buffer: *const u8,
}

#[derive(Debug)]
pub struct Capture {
    handle: Handle,
    dc: Borrowed<HDC>,
    bm: Bitmap,
}

impl Capture {
    pub fn new(handle: Handle, width: i32, height: i32) -> Result<Self, Error> {
        let dc = create_dc()?;
        let bm = create_bitmap(dc.value, width, height)?;
        Ok(Self { handle, dc, bm })
    }

    fn new_from(from: &Capture, width: i32, height: i32) -> Result<Self, Error> {
        let handle = from.handle.clone();
        let dc = create_dc()?;
        let bm = create_bitmap(dc.value, width, height)?;
        Ok(Self { handle, dc, bm })
    }

    pub fn grab(&self) -> Result<Frame, Error> {
        let handle = self.handle.to_inner()?;
        let rect = get_rect(handle)?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width == 0 || height == 0 {
            return Err(Error::WindowNotFound);
        }
        if width != self.bm.width || height != self.bm.height {
            return Err(Error::InvalidWindowSize(width, height));
        }
        let handle_dc = get_dc(handle)?;
        let dc = self.dc.value;
        let obj = unsafe { SelectObject(dc, (*self.bm.inner).into()) };
        if obj.is_invalid() {
            return Err(Error::from_last_win_error());
        }
        #[allow(unused)]
        let obj = Borrowed {
            value: obj,
            dropper: Box::new(move || {
                let _ = unsafe { SelectObject(dc, obj) };
            }),
        };
        unsafe {
            BitBlt(
                dc,
                0,
                0,
                self.bm.width,
                self.bm.height,
                handle_dc.value.into(),
                rect.left,
                rect.top,
                SRCCOPY,
            )
        }
        .map_err(Error::from)
        .map(|_| {
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
    unsafe { GetClientRect(handle, &raw mut rect) }?;
    Ok(rect)
}

#[inline(always)]
fn get_dc(handle: HWND) -> Result<Borrowed<HDC>, Error> {
    let dc = unsafe { GetDC(handle.into()) };
    if dc.is_invalid() {
        return Err(Error::from_last_win_error());
    }
    Ok(Borrowed {
        value: dc,
        dropper: Box::new(move || {
            let _ = unsafe { ReleaseDC(handle.into(), dc) };
        }),
    })
}

#[inline(always)]
fn create_dc() -> Result<Borrowed<HDC>, Error> {
    let dc = unsafe { CreateCompatibleDC(None) };
    if dc.is_invalid() {
        return Err(Error::from_last_win_error());
    }
    Ok(Borrowed {
        value: dc,
        dropper: Box::new(move || {
            let _ = unsafe { DeleteDC(dc) };
        }),
    })
}

#[inline(always)]
fn create_bitmap(dc: HDC, width: i32, height: i32) -> Result<Bitmap, Error> {
    let size = width as usize * height as usize * 4;
    let buffer = ptr::null_mut::<c_void>();
    let info = BITMAPV4HEADER {
        bV4Size: size_of::<BITMAPV4HEADER>() as u32,
        bV4Width: width,
        bV4Height: -height,
        bV4Planes: 1,
        bV4BitCount: 32,
        bV4V4Compression: BI_BITFIELDS,
        bV4RedMask: 0x00FF0000,
        bV4GreenMask: 0x0000FF00,
        bV4BlueMask: 0x000000FF,
        ..BITMAPV4HEADER::default()
    };
    let dib = unsafe {
        CreateDIBSection(
            dc.into(),
            (&raw const info).cast(),
            DIB_RGB_COLORS,
            (&raw const buffer).cast_mut(),
            None,
            0,
        )?
    };
    Ok(Bitmap {
        inner: unsafe { Owned::new(dib) },
        width,
        height,
        size,
        buffer: buffer.cast(),
    })
}
