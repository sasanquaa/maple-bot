use std::ffi::c_void;

use opencv::{
    boxed_ref::BoxedRef,
    core::{_InputArray, CV_8UC4, Mat, MatTraitConst, ToInputArray},
};
use platforms::windows::Frame;

// A Mat that owns the external buffer.
#[derive(Debug)]
pub struct OwnedMat {
    mat: BoxedRef<'static, Mat>,
    #[allow(unused)]
    data: Vec<u8>,
}

impl OwnedMat {
    pub fn new(frame: Frame) -> Self {
        let data = frame.data;
        let mat = BoxedRef::from(unsafe {
            Mat::new_nd_with_data_unsafe_def(
                &[frame.height, frame.width],
                CV_8UC4,
                data.as_ptr().cast_mut().cast(),
            )
            .unwrap()
        });
        Self { mat, data }
    }
}

#[cfg(debug_assertions)]
impl From<Mat> for OwnedMat {
    fn from(value: Mat) -> Self {
        Self {
            mat: BoxedRef::from(value),
            data: vec![],
        }
    }
}

impl ToInputArray for OwnedMat {
    fn input_array(&self) -> opencv::Result<BoxedRef<_InputArray>> {
        self.mat.input_array()
    }
}

impl MatTraitConst for OwnedMat {
    fn as_raw_Mat(&self) -> *const c_void {
        self.mat.as_raw_Mat()
    }
}
