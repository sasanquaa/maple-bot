use platforms::windows;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("OpenCV error: `{0}`")]
    Cv(#[from] opencv::Error),
    #[error("onnxruntime error: `{0}`")]
    Ort(#[from] ort::Error),
    #[error("win32 error: `{0}`")]
    Win32(#[from] windows::error::Error),
    #[error("failed to detect minimap")]
    MinimapNotFound,
    #[error("failed to detect player")]
    PlayerNotFound,
    #[error("failed to detect skill")]
    SkillNotFound,
}
