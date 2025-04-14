use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::UI::HiDpi::{PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness};

mod capture;
mod error;
mod handle;
mod keys;

pub use {capture::*, error::*, handle::*, keys::*};

pub fn init() {
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

    if INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Acquire)
        .is_ok()
    {
        unsafe {
            // I really don't get it
            SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).unwrap();
            keys::init();
        }
    }
}
