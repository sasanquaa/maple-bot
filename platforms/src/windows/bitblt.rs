use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::slice;

use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::BI_BITFIELDS;
use windows::Win32::Graphics::Gdi::BITMAPV4HEADER;
use windows::Win32::Graphics::Gdi::BitBlt;
use windows::Win32::Graphics::Gdi::CreateDCW;
use windows::Win32::Graphics::Gdi::GetMonitorInfoW;
use windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONULL;
use windows::Win32::Graphics::Gdi::MONITORINFO;
use windows::Win32::Graphics::Gdi::MONITORINFOEXW;
use windows::Win32::Graphics::Gdi::MonitorFromWindow;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, GetDC, HBITMAP, HDC, ReleaseDC,
    SRCCOPY, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use windows::core::Owned;
use windows::core::PCWSTR;

use super::Frame;
use super::HandleCell;
use super::error::Error;
use super::handle::Handle;

#[derive(Debug)]
struct DeviceContext {
    inner: HDC,
    handle: Option<HWND>,
    release: bool,
}

impl Drop for DeviceContext {
    fn drop(&mut self) {
        unsafe {
            if self.release {
                let _ = ReleaseDC(self.handle, self.inner);
            } else {
                let _ = DeleteDC(self.inner);
            }
        }
    }
}

#[derive(Debug)]
struct Bitmap {
    inner: Owned<HBITMAP>,
    dc: DeviceContext,
    width: i32,
    height: i32,
    size: usize,
    buffer: *const u8,
}

#[derive(Debug)]
pub struct BitBltCapture {
    handle: HandleCell,
    bitmap: Option<Bitmap>,
    overlay: bool,
}

impl BitBltCapture {
    pub fn new(handle: Handle) -> Self {
        BitBltCapture::new_from_cell(HandleCell::new(handle), false)
    }

    // FIXME: add `overlay` for `new`
    pub(crate) fn new_from_cell(handle: HandleCell, overlay: bool) -> Self {
        Self {
            handle,
            bitmap: None,
            overlay,
        }
    }

    #[inline]
    pub fn grab(&mut self) -> Result<Frame, Error> {
        self.grab_inner(None)
    }

    pub(crate) fn grab_inner_offset(&mut self, offset: Option<(i32, i32)>) -> Result<Frame, Error> {
        self.grab_inner(offset)
    }

    fn grab_inner(&mut self, offset: Option<(i32, i32)>) -> Result<Frame, Error> {
        let handle = self.handle.as_inner().ok_or(Error::WindowNotFound)?;
        let rect = get_rect(handle)?;
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        if width == 0 || height == 0 {
            return Err(Error::InvalidWindowSize);
        }

        let handle_dc = if self.overlay {
            get_device_context_from_monitor(handle)?
        } else {
            get_device_context(handle)?
        };
        if self.bitmap.is_none() {
            self.bitmap = Some(create_bitmap(handle_dc.inner, width, height)?);
        }

        let bitmap = self.bitmap.as_ref().unwrap();
        if width != bitmap.width || height != bitmap.height {
            self.bitmap = None;
            return Err(Error::InvalidWindowSize);
        }

        let bitmap_dc = &bitmap.dc;
        let object = unsafe { SelectObject(bitmap_dc.inner, (*bitmap.inner).into()) };
        if object.is_invalid() {
            return Err(Error::from_last_win_error());
        }
        let (left, top) = offset.unwrap_or((0, 0));
        let result = unsafe {
            BitBlt(
                bitmap_dc.inner,
                0,
                0,
                bitmap.width,
                bitmap.height,
                Some(handle_dc.inner),
                left,
                top,
                SRCCOPY,
            )
        };
        let _ = unsafe { SelectObject(bitmap_dc.inner, object) };
        if let Err(error) = result {
            return Err(Error::from(error));
        }
        // SAFETY: I swear on the love of Axis Order, this call passed the safety vibe check
        let ptr = unsafe { slice::from_raw_parts(bitmap.buffer, bitmap.size) };
        let data = ptr.to_vec();
        Ok(Frame {
            width: bitmap.width,
            height: bitmap.height,
            data,
        })
    }
}

#[inline]
fn get_rect(handle: HWND) -> Result<RECT, Error> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(handle, &raw mut rect) }?;
    Ok(rect)
}

#[inline]
fn get_device_context_from_monitor(handle: HWND) -> Result<DeviceContext, Error> {
    let monitor = unsafe { MonitorFromWindow(handle, MONITOR_DEFAULTTONULL) };
    if monitor.is_invalid() {
        return Err(Error::WindowNotFound);
    }
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: mem::size_of::<MONITORINFOEXW>() as u32,
            ..MONITORINFO::default()
        },
        ..MONITORINFOEXW::default()
    };
    unsafe {
        GetMonitorInfoW(monitor, (&raw mut info).cast()).ok()?;
    }
    let handle_dc =
        unsafe { CreateDCW(None, PCWSTR::from_raw(info.szDevice.as_ptr()), None, None) };
    if handle_dc.is_invalid() {
        return Err(Error::WindowNotFound);
    }
    Ok(DeviceContext {
        inner: handle_dc,
        handle: None,
        release: false,
    })
}

#[inline]
fn get_device_context(handle: HWND) -> Result<DeviceContext, Error> {
    let dc = unsafe { GetDC(handle.into()) };
    if dc.is_invalid() {
        return Err(Error::from_last_win_error());
    }
    Ok(DeviceContext {
        inner: dc,
        handle: Some(handle),
        release: true,
    })
}

#[inline]
fn create_bitmap(dc: HDC, width: i32, height: i32) -> Result<Bitmap, Error> {
    let dc = unsafe { CreateCompatibleDC(Some(dc)) };
    if dc.is_invalid() {
        return Err(Error::from_last_win_error());
    }

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
            Some(dc),
            (&raw const info).cast(),
            DIB_RGB_COLORS,
            (&raw const buffer).cast_mut(),
            None,
            0,
        )?
    };
    Ok(Bitmap {
        inner: unsafe { Owned::new(dib) },
        dc: DeviceContext {
            inner: dc,
            handle: None,
            release: false,
        },
        width,
        height,
        size,
        buffer: buffer.cast(),
    })
}
