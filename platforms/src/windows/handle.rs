use std::{cell::Cell, ffi::OsString, os::windows::ffi::OsStringExt, ptr, str};

use windows::Win32::{
    Foundation::{BOOL, HWND, LPARAM},
    UI::WindowsAndMessaging::{EnumWindows, GetClassNameW},
};

#[derive(Clone, Debug)]
pub(crate) struct HandleCell {
    handle: Handle,
    inner: Cell<Option<HWND>>,
}

impl HandleCell {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle,
            inner: Cell::new(None),
        }
    }

    #[inline]
    pub fn as_inner(&self) -> Option<HWND> {
        match self.handle.kind {
            HandleKind::Fixed(_) => self.handle.query_handle(),
            HandleKind::Dynamic(class) => {
                if self.inner.get().is_none() {
                    self.inner.set(self.handle.query_handle());
                }
                let handle_inner = self.inner.get()?;
                if is_class_matched(handle_inner, class) {
                    return Some(handle_inner);
                }
                self.inner.set(None);
                None
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum HandleKind {
    Fixed(HWND),
    Dynamic(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub struct Handle {
    kind: HandleKind,
}

impl Handle {
    pub fn new(class: &'static str) -> Self {
        Self {
            kind: HandleKind::Dynamic(class),
        }
    }

    pub(crate) fn new_fixed(handle: HWND) -> Self {
        Self {
            kind: HandleKind::Fixed(handle),
        }
    }

    fn query_handle(&self) -> Option<HWND> {
        match self.kind {
            HandleKind::Fixed(handle) => Some(handle),
            HandleKind::Dynamic(class) => {
                #[repr(C)]
                #[derive(Clone, Copy)]
                struct Params {
                    class: &'static str,
                    handle_out: *mut HWND,
                }

                unsafe extern "system" fn callback(handle: HWND, params: LPARAM) -> BOOL {
                    let params = unsafe { ptr::read::<Params>(params.0 as *const _) };
                    if is_class_matched(handle, params.class) {
                        unsafe { ptr::write(params.handle_out, handle) };
                        false.into()
                    } else {
                        true.into()
                    }
                }

                let mut handle = HWND::default();
                let params = Params {
                    class,
                    handle_out: &raw mut handle,
                };
                let _ = unsafe { EnumWindows(Some(callback), LPARAM(&raw const params as isize)) };
                (!handle.is_invalid()).then_some(handle)
            }
        }
    }
}

#[inline]
fn is_class_matched(handle: HWND, class: &'static str) -> bool {
    let mut buf = [0u16; 256];
    let count = unsafe { GetClassNameW(handle, &mut buf) as usize };
    if count == 0 {
        return false;
    }
    OsString::from_wide(&buf[..count])
        .to_str()
        .map(|s| s.starts_with(class))
        .unwrap_or(false)
}
