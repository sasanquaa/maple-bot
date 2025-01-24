use thiserror::Error;

use windows::{Win32::Foundation::GetLastError, core::HRESULT};

#[derive(Error, Debug)]
pub enum Error {
    #[error("at least either class or title must be provided")]
    InvalidHandle,
    #[error("the window size `{0} {1}` does not match provided size")]
    InvalidWindowSize(i32, i32),
    #[error("key was not sent due to the window not focused or other error")]
    KeyNotSent,
    #[error("window matching provided class and title cannot be found")]
    WindowNotFound,
    #[error("window matching provided class and title cannot be found")]
    Win32(#[from] windows::core::Error),
}

impl Error {
    pub unsafe fn from_last_win_error() -> Error {
        Error::Win32(HRESULT::from(unsafe { GetLastError() }).into())
    }
}
