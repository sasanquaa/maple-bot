use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{MAPVK_VK_TO_VSC, MapVirtualKeyW, VK_SPACE},
        WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP},
    },
};

use super::{error::Error, handle::Handle};

#[derive(Clone, Debug)]
pub struct Keys {
    handle: Handle,
}

pub enum KeyKind {
    SPACE,
}

impl Keys {
    pub fn new(handle: Handle) -> Self {
        Self { handle }
    }

    pub fn send(&self, key: KeyKind) -> Result<(), Error> {
        let handle = self.handle.to_inner()?;
        let key = match key {
            KeyKind::SPACE => VK_SPACE.0,
        } as u32;
        let code = unsafe { MapVirtualKeyW(key, MAPVK_VK_TO_VSC) };
        if code == 0 {
            panic!("key {} does not have a translation to scan code", key);
        }
        let keydown_flags = 1 | code << 16;
        let keyup_flags = 1 | code << 16 | 3 << 30;
        unsafe {
            PostMessageW(
                Some(handle),
                WM_KEYDOWN,
                WPARAM(key as usize),
                LPARAM(keydown_flags as isize),
            )
        }?;
        unsafe {
            PostMessageW(
                Some(handle),
                WM_KEYUP,
                WPARAM(key as usize),
                LPARAM(keyup_flags as isize),
            )
        }?;
        Ok(())
    }
}
