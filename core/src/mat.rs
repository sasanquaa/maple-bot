use opencv::core::{CV_8UC4, Mat};
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

    pub fn get(&self) -> &Mat {
        &self.mat
    }

    pub fn get_mut(&mut self) -> &mut Mat {
        &mut self.mat
    }
}
