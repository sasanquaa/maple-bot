use std::ffi::c_void;

use opencv::{
    boxed_ref::{BoxedRef, BoxedRefMut},
    core::{
        _InputArray, _InputOutputArray, _OutputArray, CV_8UC4, Mat, MatTrait, MatTraitConst,
        ToInputArray, ToInputOutputArray, ToOutputArray,
    },
};
use platforms::windows::Frame;

// A Mat that owns the external buffer.
#[derive(Debug)]
pub struct OwnedMat {
    mat: BoxedRefMut<'static, Mat>,
    #[allow(unused)]
    data: Vec<u8>,
}

impl OwnedMat {
    pub fn new(frame: Frame) -> Self {
        let data = frame.data;
        let mat = BoxedRefMut::from(unsafe {
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

#[cfg(test)]
impl From<Mat> for OwnedMat {
    fn from(value: Mat) -> Self {
        Self {
            mat: BoxedRefMut::from(value),
            data: vec![],
        }
    }
}

impl ToInputArray for OwnedMat {
    fn input_array(&self) -> opencv::Result<BoxedRef<_InputArray>> {
        self.mat.input_array()
    }
}

impl ToOutputArray for OwnedMat {
    fn output_array(&mut self) -> opencv::Result<BoxedRefMut<_OutputArray>> {
        self.mat.output_array()
    }
}

impl ToInputOutputArray for OwnedMat {
    fn input_output_array(&mut self) -> opencv::Result<BoxedRefMut<_InputOutputArray>> {
        self.mat.input_output_array()
    }
}

impl MatTrait for OwnedMat {
    fn as_raw_mut_Mat(&mut self) -> *mut c_void {
        self.mat.as_raw_mut_Mat()
    }
}

impl MatTraitConst for OwnedMat {
    fn as_raw_Mat(&self) -> *const c_void {
        self.mat.as_raw_Mat()
    }
}
