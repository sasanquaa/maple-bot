use std::{
    cell::RefCell,
    mem::{self},
    sync::LazyLock,
};

use bit_vec::BitVec;
use tokio::sync::broadcast::{self, Receiver, Sender};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{IntersectRect, MONITOR_DEFAULTTONULL, MonitorFromWindow},
        System::Threading::GetCurrentProcessId,
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBD_EVENT_FLAGS, KEYBDINPUT,
                KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC_EX, MOUSEEVENTF_ABSOLUTE,
                MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MOVE, MOUSEINPUT,
                MapVirtualKeyW, SendInput, VIRTUAL_KEY, VK_0, VK_1, VK_2, VK_3, VK_4, VK_5, VK_6,
                VK_7, VK_8, VK_9, VK_A, VK_B, VK_C, VK_CONTROL, VK_D, VK_DELETE, VK_DOWN, VK_E,
                VK_END, VK_ESCAPE, VK_F, VK_F1, VK_F2, VK_F3, VK_F4, VK_F5, VK_F6, VK_F7, VK_F8,
                VK_F9, VK_F10, VK_F11, VK_F12, VK_G, VK_H, VK_HOME, VK_I, VK_INSERT, VK_J, VK_K,
                VK_L, VK_LEFT, VK_M, VK_MENU, VK_N, VK_NEXT, VK_O, VK_OEM_1, VK_OEM_2, VK_OEM_3,
                VK_OEM_7, VK_OEM_COMMA, VK_OEM_PERIOD, VK_P, VK_PRIOR, VK_Q, VK_R, VK_RETURN,
                VK_RIGHT, VK_S, VK_SHIFT, VK_SPACE, VK_T, VK_U, VK_UP, VK_V, VK_W, VK_X, VK_Y,
                VK_Z,
            },
            WindowsAndMessaging::{
                CallNextHookEx, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
                GetWindowThreadProcessId, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED,
                LLKHF_LOWER_IL_INJECTED, SM_CXSCREEN, SM_CYSCREEN, SetForegroundWindow,
                SetWindowsHookExW, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
            },
        },
    },
    core::Owned,
};

use super::{HandleCell, error::Error, handle::Handle};

static KEY_CHANNEL: LazyLock<Sender<KeyKind>> = LazyLock::new(|| broadcast::channel(1).0);
static PROCESS_ID: LazyLock<u32> = LazyLock::new(|| unsafe { GetCurrentProcessId() });

pub(crate) fn init() -> Owned<HHOOK> {
    unsafe extern "system" fn keyboard_ll(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let msg = wparam.0 as u32;
        if code as u32 == HC_ACTION && (msg == WM_KEYUP || msg == WM_KEYDOWN) {
            let lparam_ptr = lparam.0 as *mut KBDLLHOOKSTRUCT;
            let mut key = unsafe { lparam_ptr.read() };
            let vkey = unsafe { mem::transmute::<u16, VIRTUAL_KEY>(key.vkCode as u16) };
            let key_kind = KeyKind::try_from(vkey);
            let ignore = key.dwExtraInfo == *PROCESS_ID as usize;
            if !ignore
                && msg == WM_KEYUP
                && let Ok(key) = key_kind
            {
                let _ = KEY_CHANNEL.send(key);
            } else if ignore {
                // Won't work if the hook is not on the top of the chain
                key.flags &= !LLKHF_INJECTED;
                key.flags &= !LLKHF_LOWER_IL_INJECTED;
                unsafe {
                    lparam_ptr.write(key);
                }
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
    unsafe { Owned::new(SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_ll), None, 0).unwrap()) }
}

#[derive(Debug)]
pub struct KeyReceiver {
    handle: HandleCell,
    key_input_kind: KeyInputKind,
    rx: Receiver<KeyKind>,
}

impl KeyReceiver {
    pub fn new(handle: Handle, key_input_kind: KeyInputKind) -> Self {
        Self {
            handle: HandleCell::new(handle),
            key_input_kind,
            rx: KEY_CHANNEL.subscribe(),
        }
    }

    pub fn try_recv(&mut self) -> Option<KeyKind> {
        self.rx
            .try_recv()
            .ok()
            .and_then(|key| self.can_process_key().then_some(key))
    }

    // TODO: Is this good?
    fn can_process_key(&self) -> bool {
        let fg = unsafe { GetForegroundWindow() };
        let mut fg_pid = 0;
        unsafe { GetWindowThreadProcessId(fg, Some(&raw mut fg_pid)) };
        if fg_pid == *PROCESS_ID {
            return true;
        }
        self.handle
            .as_inner()
            .map(|handle| is_foreground(handle, self.key_input_kind))
            .unwrap_or_default()
    }
}

#[derive(Debug, Copy, Clone)]
pub enum KeyInputKind {
    /// Sends input only if [`Keys::handle`] is in the foreground and focused
    Fixed,
    ///
    /// Sends input only if the foreground window is not [`Keys::handle`], on top of
    /// [`Keys::handle`] window and is focused
    Foreground,
}

#[derive(Debug, Clone)]
pub struct Keys {
    handle: HandleCell,
    key_input_kind: KeyInputKind,
    key_down: RefCell<BitVec>,
}

#[derive(PartialEq, Eq, Clone, Copy, Default, Debug)]
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
    pub fn new(handle: Handle, kind: KeyInputKind) -> Self {
        Self {
            handle: HandleCell::new(handle),
            key_input_kind: kind,
            key_down: RefCell::new(BitVec::from_elem(256, false)),
        }
    }

    pub fn send(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_down(kind)?;
        self.send_up(kind)?;
        Ok(())
    }

    pub fn send_click_to_focus(&self) -> Result<(), Error> {
        self.send_click_to_focus_inner()
    }

    // FIXME: hack for now
    pub fn send_click_to_focus_inner(&self) -> Result<(), Error> {
        let mut handle = self.get_handle()?;
        match self.key_input_kind {
            KeyInputKind::Fixed => unsafe { SetForegroundWindow(handle).ok()? },
            KeyInputKind::Foreground => {
                if !is_foreground(handle, KeyInputKind::Foreground) {
                    return Err(Error::WindowNotFound);
                }
                handle = unsafe { GetForegroundWindow() };
            }
        }
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
        self.send_input(kind, false)
    }

    pub fn send_down(&self, kind: KeyKind) -> Result<(), Error> {
        self.send_input(kind, true)
    }

    #[inline]
    fn send_input(&self, kind: KeyKind, is_down: bool) -> Result<(), Error> {
        let handle = self.get_handle()?;
        if !is_foreground(handle, self.key_input_kind) {
            return Err(Error::KeyNotSent);
        }
        let key = kind.into();
        let (scan_code, is_extended) = to_scan_code(key);
        let mut key_down = self.key_down.borrow_mut();
        // SAFETY: VIRTUAL_KEY is from range 0..254 (inclusive) and BitVec
        // was initialized with 256 elements
        let was_key_down = unsafe { key_down.get_unchecked(key.0 as usize) };
        match (is_down, was_key_down) {
            (false, false) => return Err(Error::KeyNotSent),
            _ => {
                key_down.set(key.0 as usize, is_down);
            }
        }
        send_input(to_input(key, scan_code, is_extended, is_down))
    }

    #[inline]
    fn get_handle(&self) -> Result<HWND, Error> {
        self.handle.as_inner().ok_or(Error::WindowNotFound)
    }
}

impl TryFrom<VIRTUAL_KEY> for KeyKind {
    type Error = Error;

    fn try_from(value: VIRTUAL_KEY) -> Result<Self, Error> {
        Ok(match value {
            VK_A => KeyKind::A,
            VK_B => KeyKind::B,
            VK_C => KeyKind::C,
            VK_D => KeyKind::D,
            VK_E => KeyKind::E,
            VK_F => KeyKind::F,
            VK_G => KeyKind::G,
            VK_H => KeyKind::H,
            VK_I => KeyKind::I,
            VK_J => KeyKind::J,
            VK_K => KeyKind::K,
            VK_L => KeyKind::L,
            VK_M => KeyKind::M,
            VK_N => KeyKind::N,
            VK_O => KeyKind::O,
            VK_P => KeyKind::P,
            VK_Q => KeyKind::Q,
            VK_R => KeyKind::R,
            VK_S => KeyKind::S,
            VK_T => KeyKind::T,
            VK_U => KeyKind::U,
            VK_V => KeyKind::V,
            VK_W => KeyKind::W,
            VK_X => KeyKind::X,
            VK_Y => KeyKind::Y,
            VK_Z => KeyKind::Z,
            VK_0 => KeyKind::Zero,
            VK_1 => KeyKind::One,
            VK_2 => KeyKind::Two,
            VK_3 => KeyKind::Three,
            VK_4 => KeyKind::Four,
            VK_5 => KeyKind::Five,
            VK_6 => KeyKind::Six,
            VK_7 => KeyKind::Seven,
            VK_8 => KeyKind::Eight,
            VK_9 => KeyKind::Nine,
            VK_F1 => KeyKind::F1,
            VK_F2 => KeyKind::F2,
            VK_F3 => KeyKind::F3,
            VK_F4 => KeyKind::F4,
            VK_F5 => KeyKind::F5,
            VK_F6 => KeyKind::F6,
            VK_F7 => KeyKind::F7,
            VK_F8 => KeyKind::F8,
            VK_F9 => KeyKind::F9,
            VK_F10 => KeyKind::F10,
            VK_F11 => KeyKind::F11,
            VK_F12 => KeyKind::F12,
            VK_UP => KeyKind::Up,
            VK_DOWN => KeyKind::Down,
            VK_LEFT => KeyKind::Left,
            VK_RIGHT => KeyKind::Right,
            VK_HOME => KeyKind::Home,
            VK_END => KeyKind::End,
            VK_PRIOR => KeyKind::PageUp,
            VK_NEXT => KeyKind::PageDown,
            VK_INSERT => KeyKind::Insert,
            VK_DELETE => KeyKind::Delete,
            VK_CONTROL => KeyKind::Ctrl,
            VK_RETURN => KeyKind::Enter,
            VK_SPACE => KeyKind::Space,
            VK_OEM_3 => KeyKind::Tilde,
            VK_OEM_7 => KeyKind::Quote,
            VK_OEM_1 => KeyKind::Semicolon,
            VK_OEM_COMMA => KeyKind::Comma,
            VK_OEM_PERIOD => KeyKind::Period,
            VK_OEM_2 => KeyKind::Slash,
            VK_ESCAPE => KeyKind::Esc,
            VK_SHIFT => KeyKind::Shift,
            VK_MENU => KeyKind::Alt,
            _ => return Err(crate::windows::Error::KeyNotFound),
        })
    }
}

impl From<KeyKind> for VIRTUAL_KEY {
    fn from(value: KeyKind) -> Self {
        match value {
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
}

// TODO: Is this good?
#[inline]
fn is_foreground(handle: HWND, kind: KeyInputKind) -> bool {
    let handle_fg = unsafe { GetForegroundWindow() };
    if handle_fg.is_invalid() {
        return false;
    }
    match kind {
        KeyInputKind::Fixed => handle_fg == handle,
        KeyInputKind::Foreground => {
            if handle_fg == handle {
                return false;
            }
            // Null != Null?
            if unsafe {
                MonitorFromWindow(handle_fg, MONITOR_DEFAULTTONULL)
                    != MonitorFromWindow(handle, MONITOR_DEFAULTTONULL)
            } {
                return false;
            }
            let mut rect_fg = RECT::default();
            let mut rect_handle = RECT::default();
            let mut rect_intersect = RECT::default();
            unsafe {
                if GetWindowRect(handle_fg, &mut rect_fg).is_err()
                    || GetWindowRect(handle, &mut rect_handle).is_err()
                {
                    return false;
                }
                IntersectRect(
                    &raw mut rect_intersect,
                    &raw const rect_fg,
                    &raw const rect_handle,
                )
                .as_bool()
            }
        }
    }
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
fn to_scan_code(key: VIRTUAL_KEY) -> (u16, bool) {
    let scan_code = unsafe { MapVirtualKeyW(key.0 as u32, MAPVK_VK_TO_VSC_EX) } as u16;
    let code = scan_code & 0xFF;
    let is_extended = if VK_INSERT == key {
        true
    } else {
        (scan_code & 0xFF00) != 0
    };
    (code, is_extended)
}

#[inline]
fn to_input(key: VIRTUAL_KEY, scan_code: u16, is_extended: bool, is_down: bool) -> [INPUT; 1] {
    let is_extended = if is_extended {
        KEYEVENTF_EXTENDEDKEY
    } else {
        KEYBD_EVENT_FLAGS::default()
    };
    let is_up = if is_down {
        KEYBD_EVENT_FLAGS::default()
    } else {
        KEYEVENTF_KEYUP
    };
    [INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: scan_code,
                dwFlags: is_extended | is_up,
                dwExtraInfo: *PROCESS_ID as usize,
                ..KEYBDINPUT::default()
            },
        },
    }]
}
