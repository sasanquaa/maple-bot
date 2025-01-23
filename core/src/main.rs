#![feature(str_from_raw_parts)]
#![feature(iter_array_chunks)]

use detector::{detect_minimap, detect_player};
use error::Error;
use opencv::{
    core::{Mat, Scalar, Vec3b},
    highgui::{WINDOW_GUI_NORMAL, WINDOW_KEEPRATIO, imshow, named_window, wait_key},
    imgproc::{LINE_8, cvt_color, rectangle_points, rectangle_points_def},
};
use platforms::windows::capture::Capture;

mod detector;
mod error;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let capture = Capture::new(Some("MapleStoryClass"), None, 800, 600).unwrap();
    let _ = named_window("Frame", WINDOW_KEEPRATIO | WINDOW_GUI_NORMAL);
    let mut points = None;
    loop {
        let Ok((frame, top_left, bottom_right)) =
            capture.grab().map_err(Error::from).and_then(|f| {
                if points.is_none() {
                    points = Some(detect_minimap(&f)?);
                }
                let points = points.unwrap();
                Ok((f, points.0, points.1))
            })
        else {
            continue;
        };
        let Ok((player_top_left, player_bottom_right)) =
            detect_player(&frame, (top_left, bottom_right))
        else {
            continue;
        };
        let sizes = [frame.height, frame.width];
        let data = frame
            .data
            .into_iter()
            .array_chunks::<4>()
            .map(|chunk| Vec3b::from_array([chunk[0], chunk[1], chunk[2]]))
            .collect::<Vec<_>>();
        let mut mat = Mat::new_nd_with_data(&sizes, data.as_slice())
            .unwrap()
            .clone_pointee();
        let _ = rectangle_points(
            &mut mat,
            top_left,
            bottom_right,
            Scalar::from_array([255., 0., 0., 255.]),
            2,
            LINE_8,
            0,
        )
        .unwrap();
        let _ = rectangle_points(
            &mut mat,
            player_top_left,
            player_bottom_right,
            Scalar::from_array([255., 0., 0., 255.]),
            2,
            LINE_8,
            0,
        )
        .unwrap();
        let _ = imshow("Frame", &mat);
        let _ = wait_key(1);
    }
}
