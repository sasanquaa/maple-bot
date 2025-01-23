use std::sync::LazyLock;

use opencv::{
    core::{
        Mat, MatTrait, MatTraitConst, Point, Range, Scalar, Vec3b, Vector, add_weighted_def,
        min_max_loc, no_array,
    },
    highgui::{imshow, wait_key},
    imgcodecs::{self, IMREAD_GRAYSCALE},
    imgproc::{
        COLOR_BGR2GRAY, LINE_8, TM_CCOEFF_NORMED, cvt_color_def, match_template_def,
        rectangle_points,
    },
};
use platforms::windows::capture::Frame;

use crate::error::Error;

static MINIMAP_TOP_LEFT: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!("..\\resources\\minimap_top_left.png"),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

static MINIMAP_BOTTOM_RIGHT: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!("..\\resources\\minimap_bottom_right.png"),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

static PLAYER: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!("..\\resources\\player.png"),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

fn to_ranges(minimap: (Point, Point)) -> Result<Vector<Range>, Error> {
    let mut vec = Vector::new();
    let rows = Range::new(minimap.0.y, minimap.1.y)?;
    let cols = Range::new(minimap.0.x, minimap.1.x)?;
    vec.push(rows);
    vec.push(cols);
    Ok(vec)
}

pub fn detect_player(frame: &Frame, minimap: (Point, Point)) -> Result<(Point, Point), Error> {
    let template = &*PLAYER;
    let ranges = to_ranges(minimap)?;
    let mut mat = with_contrast(to_grayscale(frame)?)?;
    let mut sub_mat = mat.ranges_mut(&ranges)?;
    let mut result = Mat::default();
    let mut score = 0f64;
    let mut top_left = Point::default();

    match_template_def(&sub_mat, template, &mut result, TM_CCOEFF_NORMED)?;
    min_max_loc(
        &result,
        None,
        Some(&mut score),
        None,
        Some(&mut top_left),
        &no_array(),
    )?;

    let bottom_right = top_left + Point::from_size(template.size().unwrap());
    let _ = rectangle_points(
        &mut sub_mat,
        top_left,
        bottom_right,
        Scalar::from_array([255., 0., 0., 255.]),
        2,
        LINE_8,
        0,
    )
    .unwrap();
    imshow("hmm", &sub_mat);
    wait_key(0);
    println!("{:?}", score);
    // if score >= 0.88 {
    Ok((top_left + minimap.0, bottom_right + minimap.0))
    // } else {
    //     Err(Error::PlayerNotFound)
    // }
}

pub fn detect_minimap(frame: &Frame) -> Result<(Point, Point), Error> {
    let mat = with_contrast(to_grayscale(frame)?)?;
    let top_left_template = &*MINIMAP_TOP_LEFT;
    let bottom_right_template = &*MINIMAP_BOTTOM_RIGHT;
    let bottom_right_size = bottom_right_template.size().unwrap();
    let mut top_left = Point::default();
    let mut top_left_score = 0f64;
    let mut top_left_result = Mat::default();
    let mut bottom_right = Point::default();
    let mut bottom_right_result = Mat::default();
    let mut bottom_right_score = 0f64;

    match_template_def(
        &mat,
        top_left_template,
        &mut top_left_result,
        TM_CCOEFF_NORMED,
    )?;
    match_template_def(
        &mat,
        bottom_right_template,
        &mut bottom_right_result,
        TM_CCOEFF_NORMED,
    )?;
    min_max_loc(
        &top_left_result,
        None,
        Some(&mut top_left_score),
        None,
        Some(&mut top_left),
        &no_array(),
    )?;
    min_max_loc(
        &bottom_right_result,
        None,
        Some(&mut bottom_right_score),
        None,
        Some(&mut bottom_right),
        &no_array(),
    )?;

    let score = (top_left_score + bottom_right_score) / 2.;
    if cfg!(debug_assertions) {
        println!(
            "{:?} / {} - {:?} / {} -> {}",
            top_left, top_left_score, bottom_right, bottom_right_score, score
        );
    }

    // if score >= 0.88 {
    Ok((top_left, bottom_right + Point::from_size(bottom_right_size)))
    // } else {
    //     Err(Error::MinimapNotFound)
    // }
}

fn with_contrast(gray_mat: Mat) -> Result<Mat, Error> {
    let mut mat = Mat::default();
    add_weighted_def(&gray_mat, 1.5, &gray_mat, 0., -40., &mut mat)?;
    Ok(mat)
}

fn to_grayscale(frame: &Frame) -> Result<Mat, Error> {
    let sizes = [frame.height, frame.width];
    let data = frame
        .data
        .iter()
        .array_chunks::<4>()
        .map(|chunk| Vec3b::from_array([*chunk[0], *chunk[1], *chunk[2]]))
        .collect::<Vec<_>>();
    let mat = Mat::new_nd_with_data(&sizes, data.as_slice())?;
    let mut gray_mat = Mat::default();
    cvt_color_def(&mat, &mut gray_mat, COLOR_BGR2GRAY)?;
    Ok(gray_mat)
}
