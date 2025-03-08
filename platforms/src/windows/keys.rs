use std::cell::RefCell;

use bit_vec::BitVec;
use windows::Win32::{
    Foundation::{ERROR_INVALID_HANDLE, ERROR_INVALID_WINDOW_HANDLE, HWND, RECT},
    UI::{
        Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
            KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC_EX, MOUSEEVENTF_ABSOLUTE,
            MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT, MapVirtualKeyW,
            SendInput, VIRTUAL_KEY, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6, VK_7, VK_8, VK_9,
            VK_A, VK_B, VK_C, VK_CONTROL, VK_D, VK_DELETE, VK_DOWN, VK_E, VK_END, VK_ESCAPE, VK_F,
            VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8, VK_F9, VK_F10, VK_F11, VK_F12,
            VK_G, VK_H, VK_HOME, VK_I, VK_INSERT, VK_J, VK_K, VK_L, VK_LEFT, VK_M, VK_MENU, VK_N,
            VK_NEXT, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3, VK_OEM_7, VK_OEM_COMMA, VK_OEM_PERIOD,
            VK_P, VK_PRIOR, VK_Q, VK_R, VK_RETURN, VK_RIGHT, VK_S, VK_SHIFT, VK_SPACE, VK_T, VK_U,
            VK_UP, VK_V, VK_W, VK_X, VK_Y, VK_Z,
        },
        WindowsAndMessaging::{
            GetForegroundWindow, GetSystemMetrics, GetWindowRect, SM_CXSCREEN, SM_CYSCREEN,
            SetForegroundWindow,
        },
    },
};

use super::{error::Error, handle::Handle};

#[derive(Debug)]
pub struct Keys {
    handle: RefCell<Handle>,
    key_down: RefCell<BitVec>,
}

#[derive(Clone, Copy, Default, Debug)]
pub enum KeyKind {
    #[default]
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Ctrl,
    Enter,
    Space,
    Tilde,
    Quote,
    Semicolon,
    Comma,
    Period,
    Slash,
    Esc,
    Shift,
    Alt,
}

impl Keys {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle: RefCell::new(handle),
            key_down: RefCell::new(BitVec::from_elem(256, false)),
        }
    }

    pub fn send(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_down(kind)?;
        self.send_up(kind)?;
        Ok(())
    }

    pub fn send_click_to_focus(&self) -> Result<(), Error> {
        self.reset_handle_if_error(self.send_click_to_focus_inner())
    }

    // FIXME: hack for now
    pub fn send_click_to_focus_inner(&self) -> Result<(), Error> {
        let handle = self.get_handle()?;
        unsafe { SetForegroundWindow(handle).ok()? };
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
        self.reset_handle_if_error(self.send_input(kind, true))
    }

    pub fn send_down(&self, kind: KeyKind) -> Result<(), Error> {
        self.reset_handle_if_error(self.send_input(kind, false))
    }

    #[inline]
    fn reset_handle_if_error(&self, result: Result<(), Error>) -> Result<(), Error> {
        if let Err(Error::Win32(code, _)) = result {
            if code == ERROR_INVALID_HANDLE.0 || code == ERROR_INVALID_WINDOW_HANDLE.0 {
                self.handle.borrow_mut().reset_inner();
            }
        }
        result
    }

    #[inline]
    fn send_input(&self, kind: KeyKind, is_up: bool) -> Result<(), Error> {
        let handle = self.get_handle()?;
        if !is_foreground(handle) {
            return Err(Error::NotSent);
        }
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

    #[inline]
    fn get_handle(&self) -> Result<HWND, Error> {
        self.handle.borrow_mut().as_inner()
    }
}

#[inline]
fn is_foreground(handle: HWND) -> bool {
    let handle_fg = unsafe { GetForegroundWindow() };
    !handle_fg.is_invalid() && handle_fg == handle
}

#[inline]
fn send_input(input: [INPUT; 1]) -> Result<(), Error> {
    let result = unsafe { SendInput(&input, size_of::<INPUT>() as i32) };
    // could be UIPI
    if result == 0 {
        Err(Error::from_last_win_error())
    } else {
        Ok(())
    }
}

#[inline]
fn to_vkey(kind: KeyKind) -> VIRTUAL_KEY {
    match kind {
        KeyKind::A => VK_A,
        KeyKind::B => VK_B,
        KeyKind::C => VK_C,
        KeyKind::D => VK_D,
        KeyKind::E => VK_E,
        KeyKind::F => VK_F,
        KeyKind::G => VK_G,
        KeyKind::H => VK_H,
        KeyKind::I => VK_I,
        KeyKind::J => VK_J,
        KeyKind::K => VK_K,
        KeyKind::L => VK_L,
        KeyKind::M => VK_M,
        KeyKind::N => VK_N,
        KeyKind::O => VK_O,
        KeyKind::P => VK_P,
        KeyKind::Q => VK_Q,
        KeyKind::R => VK_R,
        KeyKind::S => VK_S,
        KeyKind::T => VK_T,
        KeyKind::U => VK_U,
        KeyKind::V => VK_V,
        KeyKind::W => VK_W,
        KeyKind::X => VK_X,
        KeyKind::Y => VK_Y,
        KeyKind::Z => VK_Z,
        KeyKind::Zero => VK_0,
        KeyKind::One => VK_1,
        KeyKind::Two => VK_2,
        KeyKind::Three => VK_3,
        KeyKind::Four => VK_4,
        KeyKind::Five => VK_5,
        KeyKind::Six => VK_6,
        KeyKind::Seven => VK_7,
        KeyKind::Eight => VK_8,
        KeyKind::Nine => VK_9,
        KeyKind::F1 => VK_F1,
        KeyKind::F2 => VK_F2,
        KeyKind::F3 => VK_F3,
        KeyKind::F4 => VK_F4,
        KeyKind::F5 => VK_F5,
        KeyKind::F6 => VK_F6,
        KeyKind::F7 => VK_F7,
        KeyKind::F8 => VK_F8,
        KeyKind::F9 => VK_F9,
        KeyKind::F10 => VK_F10,
        KeyKind::F11 => VK_F11,
        KeyKind::F12 => VK_F12,
        KeyKind::Up => VK_UP,
        KeyKind::Down => VK_DOWN,
        KeyKind::Left => VK_LEFT,
        KeyKind::Right => VK_RIGHT,
        KeyKind::Home => VK_HOME,
        KeyKind::End => VK_END,
        KeyKind::PageUp => VK_PRIOR,
        KeyKind::PageDown => VK_NEXT,
        KeyKind::Insert => VK_INSERT,
        KeyKind::Delete => VK_DELETE,
        KeyKind::Ctrl => VK_CONTROL,
        KeyKind::Enter => VK_RETURN,
        KeyKind::Space => VK_SPACE,
        KeyKind::Tilde => VK_OEM_3,
        KeyKind::Quote => VK_OEM_7,
        KeyKind::Semicolon => VK_OEM_1,
        KeyKind::Comma => VK_OEM_COMMA,
        KeyKind::Period => VK_OEM_PERIOD,
        KeyKind::Slash => VK_OEM_2,
        KeyKind::Esc => VK_ESCAPE,
        KeyKind::Shift => VK_SHIFT,
        KeyKind::Alt => VK_MENU,
    }
}

#[inline]
fn to_scan_code(key: VIRTUAL_KEY) -> (u16, bool) {
    let scan_code = unsafe { MapVirtualKeyW(key.0 as u32, MAPVK_VK_TO_VSC_EX) } as u16;
    let code = scan_code & 0xFF;
    let is_extended = match key {
        // I don't know why, please help
        key if key == VK_INSERT => true,
        _ => (scan_code & 0xFF00) != 0,
    };
    (code, is_extended)
}

#[inline]
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
