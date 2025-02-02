use std::cell::Cell;

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
            KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC_EX, MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_C,
            VK_DOWN, VK_F, VK_LEFT, VK_RIGHT, VK_SPACE,
        },
        WindowsAndMessaging::{GetForegroundWindow, PostMessageW, WM_KEYDOWN, WM_KEYUP},
    },
};

use super::{error::Error, handle::Handle};

#[derive(Debug)]
pub struct Keys {
    handle: Handle,
    input_key_down: Cell<u128>,
    post_key_down: Cell<u128>,
}

#[derive(Clone, Copy, Debug)]
pub enum KeyKind {
    LEFT,
    DOWN,
    RIGHT,
    SPACE,
    F,
    C,
}

impl Keys {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle,
            input_key_down: Cell::new(0),
            post_key_down: Cell::new(0),
        }
    }

    pub fn send(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_down(kind)?;
        self.send_up(kind)?;
        Ok(())
    }

    pub fn send_up(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_post_or_input(kind, true)
    }

    pub fn send_down(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_post_or_input(kind, false)
    }

    #[inline(always)]
    pub fn send_post_or_input(&self, kind: KeyKind, is_up: bool) -> Result<(), Error> {
        let handle = self.handle.to_inner()?;
        let key = to_vkey(kind);
        let (scan_code, is_extended) = to_scan_code(key);
        match kind {
            KeyKind::LEFT | KeyKind::RIGHT | KeyKind::DOWN => {
                let handle_fg = unsafe { GetForegroundWindow() };
                if handle_fg.is_invalid() || handle_fg != handle {
                    return Err(Error::KeyNotSent);
                }
                if !is_up && was_key_down(key, self.input_key_down.get()) {
                    return Err(Error::KeyNotSent);
                } else {
                    self.input_key_down
                        .set(set_key_down(key, self.input_key_down.get(), is_up));
                }
                let input = to_input(key, scan_code, is_extended, is_up);
                let result = unsafe { SendInput(&input, size_of::<INPUT>() as i32) };
                // could be UIPI
                if result == 0 {
                    Err(unsafe { Error::from_last_win_error() })
                } else {
                    Ok(())
                }
            }
            KeyKind::SPACE | KeyKind::F | KeyKind::C => {
                let key_down =
                    self.post_key_down
                        .replace(set_key_down(key, self.post_key_down.get(), is_up));
                let was_down = was_key_down(key, key_down);
                let params = to_params(key, scan_code, is_extended, is_up, was_down);
                let message = if is_up { WM_KEYUP } else { WM_KEYDOWN };
                unsafe { PostMessageW(Some(handle), message, params.0, params.1) }
                    .map_err(Error::from)
            }
        }
    }
}

#[inline(always)]
fn was_key_down(key: VIRTUAL_KEY, key_down: u128) -> bool {
    (key_down & (1u128 << key.0)) != 0
}

#[inline(always)]
fn set_key_down(key: VIRTUAL_KEY, key_down: u128, is_up: bool) -> u128 {
    (key_down & !(1u128 << key.0)) | (!is_up as u128) << key.0
}

#[inline(always)]
fn to_vkey(kind: KeyKind) -> VIRTUAL_KEY {
    match kind {
        KeyKind::LEFT => VK_LEFT,
        KeyKind::RIGHT => VK_RIGHT,
        KeyKind::DOWN => VK_DOWN,
        KeyKind::SPACE => VK_SPACE,
        KeyKind::F => VK_F,
        KeyKind::C => VK_C,
    }
}

#[inline(always)]
fn to_scan_code(key: VIRTUAL_KEY) -> (u16, bool) {
    let scan_code = unsafe { MapVirtualKeyW(key.0 as u32, MAPVK_VK_TO_VSC_EX) } as u16;
    let code = scan_code & 0xFF;
    let is_extended = (scan_code & 0xFF00) != 0;
    (code, is_extended)
}

#[inline(always)]
fn to_params(
    key: VIRTUAL_KEY,
    scan_code: u16,
    is_extended: bool,
    is_up: bool,
    was_down: bool,
) -> (WPARAM, LPARAM) {
    let is_extended = is_extended as u32;
    let was_down = was_down as u32;
    let scan_code = scan_code as u32;
    let flags = scan_code << 16 | is_extended << 24;
    let flags = if is_up {
        0xC0000001 | flags
    } else {
        0x00000001 | flags | was_down << 30
    };
    (WPARAM(key.0 as usize), LPARAM(flags as isize))
}

#[inline(always)]
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
