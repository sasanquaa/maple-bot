use std::{cell::Cell, ffi::OsString, os::windows::ffi::OsStringExt, ptr, slice, str};

use windows::Win32::{
    Foundation::{BOOL, GetLastError, HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId,
    },
};

use super::error::Error;

#[derive(Clone, Debug)]
pub struct Handle {
    class: Option<&'static str>,
    title: Option<&'static str>,
    inner: Cell<(HWND, u32)>,
}

impl Handle {
    pub fn new(class: Option<&'static str>, title: Option<&'static str>) -> Result<Self, Error> {
        if class.is_none() && title.is_none() {
            return Err(Error::InvalidHandle);
        }
        Ok(Handle {
            class,
            title,
            inner: Cell::new((HWND::default(), 0)),
        })
    }

    pub(crate) fn to_inner(&self) -> Result<HWND, Error> {
        if self.inner.get().0.is_invalid() {
            self.inner.set(self.query_handle()?);
        } else {
            if !self.validate_handle() {
                self.invalidate();
                return Err(Error::WindowNotFound);
            }
        }
        Ok(self.inner.get().0)
    }

    #[inline(always)]
    fn validate_handle(&self) -> bool {
        let pid = 0u32;
        let inner = self.inner.get();
        let _ = unsafe { GetWindowThreadProcessId(inner.0, Some((&raw const pid).cast_mut())) };
        pid == inner.1
    }

    #[inline(always)]
    fn invalidate(&self) {
        self.inner.set((HWND::default(), 0));
    }

    fn query_handle(&self) -> Result<(HWND, u32), Error> {
        #[repr(C)]
        #[derive(Clone, Copy)]
        struct Params {
            title: *const u8,
            title_len: usize,
            class: *const u8,
            class_len: usize,
            buf: *mut u16,
            buf_len: usize,
            handle_out: *mut HWND,
            pid_out: *mut u32,
        }
        let mut handle = HWND::default();
        let mut pid = 0u32;
        let mut buf = [0u16; 256];
        let params = Params {
            title: self.title.map(str::as_ptr).unwrap_or(ptr::null()),
            title_len: self.title.map(str::len).unwrap_or(0),
            class: self.class.map(str::as_ptr).unwrap_or(ptr::null()),
            class_len: self.class.map(str::len).unwrap_or(0),
            buf: buf.as_mut_ptr(),
            buf_len: buf.len(),
            handle_out: &raw mut handle,
            pid_out: &raw mut pid,
        };

        fn class_or_title_matched(
            handle: HWND,
            buf: &mut [u16],
            text: *const u8,
            text_len: usize,
            is_class: bool,
        ) -> bool {
            if text.is_null() || text_len == 0 {
                return true;
            }
            let count = unsafe {
                if is_class {
                    GetClassNameW(handle, buf) as usize
                } else {
                    GetWindowTextW(handle, buf) as usize
                }
            };
            if count == 0 {
                if cfg!(debug_assertions) {
                    let error = unsafe { GetLastError() };
                    if is_class {
                        eprintln!("GetClassNameW() {:?}", error);
                    } else {
                        eprintln!("GetWindowTextW() {:?}", error);
                    }
                }
                return false;
            }
            let text = unsafe { std::str::from_raw_parts(text, text_len) };
            OsString::from_wide(&buf[..count])
                .to_str()
                .map(|s| s == text)
                .unwrap_or(false)
        }

        unsafe extern "system" fn callback(handle: HWND, params: LPARAM) -> BOOL {
            let params = unsafe { ptr::read::<Params>(params.0 as *const _) };
            let buf = unsafe { slice::from_raw_parts_mut(params.buf, params.buf_len) };

            let class_matched =
                class_or_title_matched(handle, buf, params.class, params.class_len, true);
            let title_matched =
                class_or_title_matched(handle, buf, params.title, params.title_len, false);

            if class_matched && title_matched {
                unsafe { ptr::write(params.handle_out, handle) };
                unsafe { GetWindowThreadProcessId(handle, Some(params.pid_out)) };
                return false.into();
            }
            true.into()
        }
        let _ = unsafe { EnumWindows(Some(callback), LPARAM(&raw const params as isize)) };
        if handle.is_invalid() {
            Err(Error::WindowNotFound)
        } else {
            Ok((handle, pid))
        }
    }
}
