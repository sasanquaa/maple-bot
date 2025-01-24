use std::cell::Cell;

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_LEFT,
            VK_RIGHT, VK_SPACE,
        },
        WindowsAndMessaging::{GetForegroundWindow, PostMessageW, WM_KEYDOWN, WM_KEYUP},
    },
};

use super::{error::Error, handle::Handle};

#[derive(Clone, Debug)]
pub struct Keys {
    handle: Handle,
    input_key_down: Cell<u128>,
}

#[derive(Clone, Copy, Debug)]
pub enum KeyKind {
    LEFT,
    RIGHT,
    SPACE,
    F,
}

impl Keys {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle,
            input_key_down: Cell::new(0),
        }
    }

    pub fn send(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_key_up(kind)?;
        self.send_key_down(kind)?;
        Ok(())
    }

    pub fn send_key_up(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_post_or_input(kind, true)
    }

    pub fn send_key_down(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_post_or_input(kind, false)
    }

    pub fn send_post_or_input(&self, kind: KeyKind, is_up: bool) -> Result<(), Error> {
        let key = to_vkey(kind);
        let (scan_code, is_extended) = to_scan_code(key);
        match kind {
            KeyKind::LEFT | KeyKind::RIGHT => self.send_input(key, scan_code, is_extended, is_up),
            KeyKind::SPACE | KeyKind::F => self.send_post(key, scan_code, is_extended, is_up),
        }
    }

    fn send_post(
        &self,
        key: VIRTUAL_KEY,
        scan_code: u16,
        is_extended: bool,
        is_up: bool,
    ) -> Result<(), Error> {
        let handle = self.handle.to_inner()?;
        let (wparam, lparam) = to_params(key, scan_code, is_extended, is_up);
        let message = if is_up { WM_KEYUP } else { WM_KEYDOWN };
        unsafe { PostMessageW(Some(handle), message, wparam, lparam) }.map_err(Error::from)
    }

    fn send_input(
        &self,
        key: VIRTUAL_KEY,
        scan_code: u16,
        is_extended: bool,
        is_up: bool,
    ) -> Result<(), Error> {
        let foreground = unsafe { GetForegroundWindow() };
        let handle = self.handle.to_inner()?;
        if foreground.is_invalid() || foreground != handle {
            return Err(Error::KeyNotSent);
        }
        let input = to_input(key, scan_code, is_extended, is_up);
        let key_down = self.input_key_down.get();
        let key_down_mask = 1u128 << key.0;
        let is_down = !is_up;
        let was_down = (key_down & key_down_mask) != 0;
        if is_down && was_down {
            return Err(Error::KeyNotSent);
        } else {
            self.input_key_down
                .set((key_down & !key_down_mask) | ((is_down as u128) << key.0));
        }
        let result = unsafe { SendInput(&input, size_of::<INPUT>() as i32) };
        // could be UIPI
        if result == 0 {
            return Err(unsafe { Error::from_last_win_error() });
        } else {
            Ok(())
        }
    }
}

fn to_vkey(kind: KeyKind) -> VIRTUAL_KEY {
    match kind {
        KeyKind::LEFT => VK_LEFT,
        KeyKind::RIGHT => VK_RIGHT,
        KeyKind::SPACE => VK_SPACE,
        KeyKind::F => VIRTUAL_KEY(0x46),
    }
}

fn to_scan_code(key: VIRTUAL_KEY) -> (u16, bool) {
    let scan_code = unsafe { MapVirtualKeyW(key.0 as u32, MAPVK_VK_TO_VSC_EX) } as u16;
    let code = scan_code & 0xFF;
    let is_extended = (scan_code & 0xFF00) != 0;
    (code, is_extended)
}

fn to_params(key: VIRTUAL_KEY, scan_code: u16, is_extended: bool, is_up: bool) -> (WPARAM, LPARAM) {
    let is_extended = is_extended as u32;
    let flags = 1 | ((scan_code as u32) << 16);
    let flags = if is_up {
        flags | (3 << 30)
    } else {
        flags | is_extended << 30
    };
    (WPARAM(key.0 as usize), LPARAM(flags as isize))
}

fn to_input(key: VIRTUAL_KEY, scan_code: u16, is_extended: bool, is_up: bool) -> [INPUT; 1] {
    let is_extended = if is_extended {
        KEYEVENTF_EXTENDEDKEY
    } else {
        KEYBD_EVENT_FLAGS::default()
    };
    let is_up = if is_up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS::default()
    };
    [INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan_code,
                dwFlags: is_extended | is_up,
                ..KEYBDINPUT::default()
            },
        },
    }]
}
