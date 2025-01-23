use std::sync::LazyLock;

use opencv::{
    core::{
        Mat, MatTrait, MatTraitConst, Point, Range, Vector, add_weighted_def, min_max_loc, no_array,
    },
    imgcodecs::{self, IMREAD_GRAYSCALE},
    imgproc::{COLOR_BGR2GRAY, TM_CCOEFF_NORMED, cvt_color_def, match_template_def},
};

use crate::error::Error;

static MINIMAP_TOP_LEFT: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!("..\\resources\\minimap_top_left_3.png"),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

static MINIMAP_BOTTOM_RIGHT: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!("..\\resources\\minimap_bottom_right_3.png"),
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

pub fn detect_player(mat: &Mat, minimap: (Point, Point)) -> Result<(Point, Point), Error> {
    let template = &*PLAYER;
    let ranges = to_ranges(minimap)?;
    let mat = to_grayscale(mat, Some(1.5), Some(-100.))?;
    let sub_mat = mat.ranges(&ranges)?;
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

    let top_left = top_left + minimap.0;
    let bottom_right = top_left + Point::from_size(template.size().unwrap());
    if cfg!(debug_assertions) {
        println!("Player: {:?} - {:?} -> {}", top_left, bottom_right, score);
    }
    if score >= 0.80 {
        Ok((top_left, bottom_right))
    } else {
        Err(Error::PlayerNotFound)
    }
}

pub fn detect_minimap(mat: &Mat) -> Result<(Point, Point), Error> {
    let mat = to_grayscale(mat, Some(1.5), Some(-80.))?;
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
            "Minimap: {:?} / {} - {:?} / {} -> {}",
            top_left, top_left_score, bottom_right, bottom_right_score, score
        );
    }

    if score >= 0.8 {
        Ok((top_left, bottom_right + Point::from_size(bottom_right_size)))
    } else {
        Err(Error::MinimapNotFound)
    }
}

fn to_ranges(minimap: (Point, Point)) -> Result<Vector<Range>, Error> {
    let mut vec = Vector::new();
    let rows = Range::new(minimap.0.y, minimap.1.y)?;
    let cols = Range::new(minimap.0.x, minimap.1.x)?;
    vec.push(rows);
    vec.push(cols);
    Ok(vec)
}

pub fn to_grayscale(
    mat: &Mat,
    contrast: Option<f64>,
    brightness: Option<f64>,
) -> Result<Mat, Error> {
    let mut gray_mat = Mat::default();
    cvt_color_def(mat, &mut gray_mat, COLOR_BGR2GRAY)?;
    if contrast.is_some() || brightness.is_some() {
        let mut contrast_mat = Mat::default();
        let contrast = contrast.unwrap_or(1.);
        let brightness = brightness.unwrap_or(0.);
        add_weighted_def(
            &gray_mat,
            contrast,
            &gray_mat,
            0.,
            brightness,
            &mut contrast_mat,
        )?;
        gray_mat = contrast_mat;
    }
    Ok(gray_mat)
}
