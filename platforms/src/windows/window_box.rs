use std::{
    ffi::c_void,
    num::NonZeroU32,
    rc::Rc,
    sync::{
        Arc, Barrier, Mutex,
        mpsc::{self, Sender},
    },
    thread::{self},
};

use softbuffer::{Context, Surface};
use tao::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::windows::EventLoopBuilderExtWindows,
    rwh_06::{HasWindowHandle, RawWindowHandle},
    window::WindowBuilder,
};
use windows::Win32::Foundation::HWND;

use super::{BitBltCapture, Error, Frame, HandleCell};

enum Message {
    Show,
    Hide,
}

#[derive(Debug)]
pub struct WindowBoxCapture {
    position: Arc<Mutex<Option<PhysicalPosition<i32>>>>,
    msg_tx: Sender<Message>,
    capture: BitBltCapture,
}

impl Default for WindowBoxCapture {
    fn default() -> Self {
        let handle = Arc::new(Mutex::new(None));
        let handle_clone = handle.clone();
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();
        let position = Arc::new(Mutex::new(None));
        let position_clone = position.clone();
        let (msg_tx, msg_rx) = mpsc::channel::<Message>();

        thread::spawn(move || {
            let handle = handle_clone;
            let position = position_clone;
            let event_loop = EventLoopBuilder::new().with_any_thread(true).build();
            let window = WindowBuilder::new()
                .with_title("Capture Area")
                .with_decorations(true)
                .with_minimizable(false)
                .with_closable(false)
                .with_transparent(true)
                .with_resizable(true)
                .with_min_inner_size(PhysicalSize::new(800, 600))
                .with_max_inner_size(PhysicalSize::new(1920, 1080))
                .build(&event_loop)
                .unwrap();
            let window = Rc::new(window);
            let context = Context::new(window.clone()).unwrap();
            let mut surface = Surface::new(&context, window.clone()).unwrap();
            let window = Some(window);

            *handle.lock().unwrap() =
                window
                    .as_ref()
                    .unwrap()
                    .window_handle()
                    .ok()
                    .map(|handle| match handle.as_raw() {
                        RawWindowHandle::Win32(handle) => handle.hwnd,
                        _ => unreachable!(),
                    });
            *position.lock().unwrap() = window.as_ref().unwrap().inner_position().ok();
            barrier_clone.wait();

            event_loop.run(move |event, _, control_flow| {
                *control_flow = ControlFlow::Poll;
                if let Ok(msg) = msg_rx.try_recv() {
                    match msg {
                        Message::Show => {
                            if let Some(ref window) = window {
                                window.set_visible(true);
                            }
                        }
                        Message::Hide => {
                            if let Some(ref window) = window {
                                window.set_visible(false);
                            }
                        }
                    }
                }

                match event {
                    Event::WindowEvent {
                        window_id: _,
                        event: WindowEvent::Moved(updated),
                        ..
                    } => {
                        if let Some(ref window) = window {
                            *position.lock().unwrap() =
                                window.inner_position().ok().or(Some(updated));
                        }
                    }
                    Event::RedrawRequested(_) => {
                        if let Some(ref window) = window {
                            let size = window.inner_size();
                            let Some(width) = NonZeroU32::new(size.width) else {
                                return;
                            };
                            let Some(height) = NonZeroU32::new(size.height) else {
                                return;
                            };
                            surface.resize(width, height).unwrap();
                            let mut buffer = surface.buffer_mut().unwrap();
                            buffer.fill(0);
                            buffer.present().unwrap();
                        }
                    }
                    Event::MainEventsCleared => {
                        if let Some(ref window) = window {
                            window.request_redraw();
                        }
                    }
                    _ => (),
                }
            });
        });
        barrier.wait();
        let handle = HWND(handle.lock().unwrap().unwrap().get() as *mut c_void);
        let handle_cell = HandleCell::new_fixed(handle);
        let capture = BitBltCapture::new_from_cell(handle_cell, true);

        Self {
            position,
            msg_tx,
            capture,
        }
    }
}

impl WindowBoxCapture {
    pub fn grab(&mut self) -> Result<Frame, Error> {
        self.capture.grab_inner_offset(self.position())
    }

    #[inline]
    fn position(&self) -> Option<(i32, i32)> {
        self.position
            .lock()
            .unwrap()
            .map(|position| (position.x, position.y))
    }

    pub fn show(&self) {
        let _ = self.msg_tx.send(Message::Show);
    }

    pub fn hide(&self) {
        let _ = self.msg_tx.send(Message::Hide);
    }
}
