#![feature(str_from_raw_parts)]
#![feature(iter_array_chunks)]

use detector::{detect_minimap, detect_player, to_grayscale};
use error::Error;
use mat::OwnedMat;
use opencv::{
    core::Scalar,
    highgui::{WINDOW_GUI_NORMAL, WINDOW_KEEPRATIO, imshow, named_window, wait_key},
    imgcodecs::imwrite_def,
    imgproc::{LINE_8, rectangle_points},
};
use platforms::windows::{capture::DynamicCapture, handle::Handle};

mod detector;
mod error;
mod mat;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let handle = Handle::new(Some("MapleStoryClass"), None)?;
    let capture = DynamicCapture::new(handle).unwrap();
    let _ = named_window("Frame", WINDOW_KEEPRATIO | WINDOW_GUI_NORMAL);
    let mut check = false;
    loop {
        let Ok(mut mat) = capture.grab().map_err(Error::from).map(OwnedMat::new) else {
            continue;
        };
        if check {
            let mat = to_grayscale(mat.get(), Some(1.5), Some(-80.0)).unwrap();
            imwrite_def("test.png", &mat).unwrap();
            return Ok(());
        } else {
            let Ok((top_left, bottom_right)) = detect_minimap(mat.get()) else {
                continue;
            };
            let Ok((player_top_left, player_bottom_right)) =
                detect_player(mat.get(), (top_left, bottom_right))
            else {
                continue;
            };
            let _ = rectangle_points(
                mat.get_mut(),
                top_left,
                bottom_right,
                Scalar::from_array([255., 0., 0., 255.]),
                2,
                LINE_8,
                0,
            )
            .unwrap();
            let _ = rectangle_points(
                mat.get_mut(),
                player_top_left,
                player_bottom_right,
                Scalar::from_array([255., 0., 0., 255.]),
                1,
                LINE_8,
                0,
            )
            .unwrap();
            // let _ = imshow("Frame", &mat);
            let _ = imshow("Frame", mat.get());
            let _ = wait_key(1);
        }
    }
}
