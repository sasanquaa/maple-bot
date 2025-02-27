use windows::Win32::UI::HiDpi::{PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness};

mod capture;
mod error;
mod handle;
mod keys;

pub use {capture::*, error::*, handle::*, keys::*};

pub fn init() {
    unsafe {
        // I really don't get it
        SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).unwrap();
    }
}
