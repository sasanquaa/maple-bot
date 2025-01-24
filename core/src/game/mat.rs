use std::ops::{Deref, DerefMut};

use opencv::{
    boxed_ref::BoxedRef,
    core::{_InputArray, CV_8UC4, Mat, ToInputArray},
};
use platforms::windows::capture::Frame;

#[derive(Debug)]
pub struct OwnedMat {
    mat: Mat,
    #[allow(unused)]
    data: Vec<u8>,
}

impl OwnedMat {
    pub fn new(frame: Frame) -> Self {
        let data = frame.data;
        let mat = unsafe {
            Mat::new_nd_with_data_unsafe_def(
                &[frame.height, frame.width],
                CV_8UC4,
                data.as_ptr().cast_mut().cast(),
            )
        }
        .expect("failed to convert Frame to Mat");
        Self { mat, data }
    }
}

impl ToInputArray for OwnedMat {
    fn input_array(&self) -> opencv::Result<BoxedRef<_InputArray>> {
        self.mat.input_array()
    }
}

#[cfg(debug_assertions)]
impl Deref for OwnedMat {
    type Target = Mat;
    fn deref(&self) -> &Self::Target {
        &self.mat
    }
}

#[cfg(debug_assertions)]
impl DerefMut for OwnedMat {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mat
    }
}
