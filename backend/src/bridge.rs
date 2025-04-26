use std::fmt::Debug;
use std::{any::Any, cell::RefCell};

use anyhow::Result;
#[cfg(test)]
use mockall::automock;
use platforms::windows::{
    BitBltCapture, Frame, Handle, KeyInputKind, KeyKind, Keys, WgcCapture, WindowBoxCapture,
};

use crate::{CaptureMode, context::MS_PER_TICK, rpc::KeysService};

/// The input method to use for key sender
///
/// Bridge enum between platforms and RPC
pub enum KeySenderMethod {
    Rpc(String),
    Default(Handle, KeyInputKind),
}

/// The inner kind of key sender
#[derive(Debug)]
enum KeySenderKind {
    Rpc(Option<RefCell<KeysService>>),
    Default(Keys),
}

/// A trait for sending keys
///
/// Mostly needed for tests
#[cfg_attr(test, automock)]
pub trait KeySender: Debug + Any {
    fn set_method(&mut self, method: KeySenderMethod);

    fn send(&self, kind: KeyKind) -> Result<()>;

    fn send_click_to_focus(&self) -> Result<()>;

    fn send_up(&self, kind: KeyKind) -> Result<()>;

    fn send_down(&self, kind: KeyKind) -> Result<()>;
}

#[derive(Debug)]
pub struct DefaultKeySender {
    kind: KeySenderKind,
}

impl DefaultKeySender {
    pub fn new(method: KeySenderMethod) -> Self {
        Self {
            kind: to_key_sender_kind_from(method),
        }
    }
}

impl KeySender for DefaultKeySender {
    fn set_method(&mut self, method: KeySenderMethod) {
        match &method {
            KeySenderMethod::Rpc(url) => {
                if let KeySenderKind::Rpc(ref option) = self.kind {
                    let service = option.as_ref();
                    let service_borrow = service.map(|service| service.borrow_mut());
                    if let Some(mut borrow) = service_borrow
                        && borrow.url() == url
                    {
                        borrow.reset();
                        return;
                    }
                }
            }
            KeySenderMethod::Default(_, _) => (),
        }
        self.kind = to_key_sender_kind_from(method);
    }

    fn send(&self, kind: KeyKind) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(service) => {
                if let Some(cell) = service {
                    cell.borrow_mut().send(kind)?;
                }
                Ok(())
            }
            KeySenderKind::Default(keys) => {
                keys.send(kind)?;
                Ok(())
            }
        }
    }

    fn send_click_to_focus(&self) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(_) => Ok(()),
            KeySenderKind::Default(keys) => {
                keys.send_click_to_focus()?;
                Ok(())
            }
        }
    }

    fn send_up(&self, kind: KeyKind) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(service) => {
                if let Some(cell) = service {
                    cell.borrow_mut().send_up(kind)?;
                }
                Ok(())
            }
            KeySenderKind::Default(keys) => {
                keys.send_up(kind)?;
                Ok(())
            }
        }
    }

    fn send_down(&self, kind: KeyKind) -> Result<()> {
        match &self.kind {
            KeySenderKind::Rpc(service) => {
                if let Some(cell) = service {
                    cell.borrow_mut().send_down(kind)?;
                }
                Ok(())
            }
            KeySenderKind::Default(keys) => {
                keys.send_down(kind)?;
                Ok(())
            }
        }
    }
}

/// A bridge enum for platform and database
#[derive(Debug)]
pub enum ImageCaptureKind {
    BitBlt(BitBltCapture),
    Wgc(Option<WgcCapture>),
    BitBltArea(WindowBoxCapture),
}

/// A struct for managing different capture modes
#[derive(Debug)]
pub struct ImageCapture {
    kind: ImageCaptureKind,
}

impl ImageCapture {
    pub fn new(handle: Handle, mode: CaptureMode) -> Self {
        Self {
            kind: to_image_capture_kind_from(handle, mode),
        }
    }

    pub fn kind(&self) -> &ImageCaptureKind {
        &self.kind
    }

    pub fn grab(&mut self) -> Option<Frame> {
        match &mut self.kind {
            ImageCaptureKind::BitBlt(capture) => capture.grab().ok(),
            ImageCaptureKind::Wgc(capture) => {
                capture.as_mut().and_then(|capture| capture.grab().ok())
            }
            ImageCaptureKind::BitBltArea(capture) => capture.grab().ok(),
        }
    }

    pub fn set_mode(&mut self, handle: Handle, mode: CaptureMode) {
        self.kind = to_image_capture_kind_from(handle, mode);
    }
}

#[inline]
fn to_key_sender_kind_from(method: KeySenderMethod) -> KeySenderKind {
    match method {
        KeySenderMethod::Rpc(url) => {
            KeySenderKind::Rpc(KeysService::connect(url).map(RefCell::new).ok())
        }
        KeySenderMethod::Default(handle, kind) => KeySenderKind::Default(Keys::new(handle, kind)),
    }
}

#[inline]
fn to_image_capture_kind_from(handle: Handle, mode: CaptureMode) -> ImageCaptureKind {
    match mode {
        CaptureMode::BitBlt => ImageCaptureKind::BitBlt(BitBltCapture::new(handle, false)),
        CaptureMode::WindowsGraphicsCapture => {
            ImageCaptureKind::Wgc(WgcCapture::new(handle, MS_PER_TICK).ok())
        }
        CaptureMode::BitBltArea => ImageCaptureKind::BitBltArea(WindowBoxCapture::default()),
    }
}
