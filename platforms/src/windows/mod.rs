use windows::Win32::UI::HiDpi::{PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness};

pub mod capture;

pub mod handle;

pub mod keys;

pub mod error;

pub fn init() {
    unsafe {
        // I really don't get it
        SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).unwrap();
    }
}
