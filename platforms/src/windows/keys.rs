use std::cell::RefCell;

use bit_vec::BitVec;
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
            KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC_EX, MOUSEEVENTF_ABSOLUTE,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT, MapVirtualKeyW,
            SendInput, VIRTUAL_KEY, VK_1, VK_4, VK_6, VK_A, VK_C, VK_CONTROL, VK_D, VK_DELETE,
            VK_DOWN, VK_ESCAPE, VK_F, VK_F2, VK_F3, VK_F4, VK_F7, VK_INSERT, VK_LEFT, VK_OEM_3,
            VK_RETURN, VK_RIGHT, VK_SPACE, VK_UP, VK_W, VK_Y,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetSystemMetrics, GetWindowRect, SM_CXSCREEN, SM_CYSCREEN,
        },
    },
};

use super::{error::Error, handle::Handle};

#[derive(Debug)]
pub struct Keys {
    handle: Handle,
    key_down: RefCell<BitVec>,
}

#[derive(Clone, Copy, Debug)]
pub enum KeyKind {
    Esc,
    Enter,
    Tilde,
    One,
    Four,
    Six,
    Insert,
    Up,
    Ctrl,
    Down,
    Left,
    Right,
    Space,
    Delete,
    Y,
    F,
    C,
    A,
    D,
    W,
    R,
    F2,
    F3,
    F4,
    F7,
}

impl Keys {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle,
            key_down: RefCell::new(BitVec::from_elem(256, false)),
        }
    }

    pub fn send(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_down(kind)?;
        self.send_up(kind)?;
        Ok(())
    }

    // FIXME: hack for now
    pub fn send_click_to_focus(&self) -> Result<(), Error> {
        self.ensure_foreground()?;
        let handle = self.handle.to_inner()?;
        let x_metric = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let y_metric = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let mut rect = RECT::default();
        unsafe { GetWindowRect(handle, &raw mut rect)? };
        let dx = rect.left + (rect.right - rect.left) / 2;
        let dx = (dx * 65536) / x_metric;
        let dy = rect.top + (rect.bottom - rect.top) / 2;
        let dy = (dy * 65536) / y_metric;
        let input = [INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    dwFlags: MOUSEEVENTF_ABSOLUTE
                        | MOUSEEVENTF_MOVE
                        | MOUSEEVENTF_LEFTDOWN
                        | MOUSEEVENTF_LEFTUP,
                    ..MOUSEINPUT::default()
                },
            },
        }];
        send_input(input)
    }

    pub fn send_up(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_input(kind, true)
    }

    pub fn send_down(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_input(kind, false)
    }

    fn ensure_foreground(&self) -> Result<(), Error> {
        if !is_foreground(self.handle.to_inner()?) {
            return Err(Error::NotSent);
        }
        Ok(())
    }

    #[inline(always)]
    fn send_input(&self, kind: KeyKind, is_up: bool) -> Result<(), Error> {
        self.ensure_foreground()?;
        let key = to_vkey(kind);
        let (scan_code, is_extended) = to_scan_code(key);
        let mut key_down = self.key_down.borrow_mut();
        // SAFETY: VIRTUAL_KEY is from range 0..254 (inclusive) and BitVec
        // was initialized with 256 elements
        let was_key_down = unsafe { key_down.get_unchecked(key.0 as usize) };
        match (is_up, was_key_down) {
            (is_up, was_down) if !is_up && was_down => return Err(Error::NotSent),
            (is_up, was_down) if is_up && !was_down => return Err(Error::NotSent),
            _ => {
                key_down.set(key.0 as usize, !is_up);
            }
        }
        send_input(to_input(key, scan_code, is_extended, is_up))
    }
}

fn is_foreground(handle: HWND) -> bool {
    let handle_fg = unsafe { GetForegroundWindow() };
    !handle_fg.is_invalid() && handle_fg == handle
}

#[inline(always)]
fn send_input(input: [INPUT; 1]) -> Result<(), Error> {
    let result = unsafe { SendInput(&input, size_of::<INPUT>() as i32) };
    // could be UIPI
    if result == 0 {
        Err(Error::from_last_win_error())
    } else {
        Ok(())
    }
}

#[inline(always)]
fn to_vkey(kind: KeyKind) -> VIRTUAL_KEY {
    match kind {
        KeyKind::Esc => VK_ESCAPE,
        KeyKind::Enter => VK_RETURN,
        KeyKind::Tilde => VK_OEM_3,
        KeyKind::Left => VK_LEFT,
        KeyKind::Right => VK_RIGHT,
        KeyKind::Up => VK_UP,
        KeyKind::Down => VK_DOWN,
        KeyKind::Space => VK_SPACE,
        KeyKind::Delete => VK_DELETE,
        KeyKind::One => VK_1,
        KeyKind::Four => VK_4,
        KeyKind::Six => VK_6,
        KeyKind::Ctrl => VK_CONTROL,
        KeyKind::Insert => VK_INSERT,
        KeyKind::A => VK_A,
        KeyKind::C => VK_C,
        KeyKind::D => VK_D,
        KeyKind::F => VK_F,
        KeyKind::Y => VK_Y,
        KeyKind::W => VK_W,
        KeyKind::R => VIRTUAL_KEY(0x52),
        KeyKind::F2 => VK_F2,
        KeyKind::F3 => VK_F3,
        KeyKind::F4 => VK_F4,
        KeyKind::F7 => VK_F7,
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
