use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use windows::Win32::UI::{
    HiDpi::{PROCESS_PER_MONITOR_DPI_AWARE, SetProcessDpiAwareness},
    WindowsAndMessaging::{DispatchMessageW, GetMessageW, MSG, TranslateMessage},
};

mod bitblt;
mod error;
mod handle;
mod keys;
mod wgc;

pub use {bitblt::*, error::*, handle::*, keys::*, wgc::*};

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
}

pub fn init() {
    static INITIALIZED: AtomicBool = AtomicBool::new(false);

    if INITIALIZED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::Acquire)
        .is_ok()
    {
        let barrier = Arc::new(Barrier::new(2));
        unsafe {
            // I really don't get it
            SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE).unwrap();
            let keys_barrier = barrier.clone();
            thread::spawn(move || {
                let _hook = keys::init();
                let mut msg = MSG::default();
                keys_barrier.wait();
                while GetMessageW(&raw mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&raw const msg);
                    let _ = DispatchMessageW(&raw const msg);
                }
            });
        }
        barrier.wait();
    }
}
