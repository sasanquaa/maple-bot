use std::sync::LazyLock;

use opencv::{
    core::{
        Mat, MatTraitConst, Point, Range, Rect, Size, ToInputArray, Vector, add_weighted_def,
        min_max_loc, no_array,
    },
    imgcodecs::{self, IMREAD_GRAYSCALE},
    imgproc::{COLOR_BGR2GRAY, TM_CCOEFF_NORMED, cvt_color_def, match_template_def},
};

use crate::error::Error;

static MINIMAP_TOP_LEFT: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("MINIMAP_TOP_LEFT_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

static MINIMAP_BOTTOM_RIGHT: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("MINIMAP_BOTTOM_RIGHT_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

static PLAYER: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(include_bytes!(env!("PLAYER_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
});

pub fn minimap_top_left_template_size() -> Size {
    (&*MINIMAP_TOP_LEFT)
        .size()
        .expect("failed to retrieve minimap template size")
}

pub fn minimap_bottom_right_template_size() -> Size {
    (&*MINIMAP_BOTTOM_RIGHT)
        .size()
        .expect("failed to retrieve minimap template size")
}

pub fn detect_player(grayscale: &impl ToInputArray, threshold: f64) -> Result<Rect, Error> {
    let template = &*PLAYER;
    let mut result = Mat::default();
    let mut score = 0f64;
    let mut tl = Point::default();

    match_template_def(grayscale, template, &mut result, TM_CCOEFF_NORMED)?;
    min_max_loc(
        &result,
        None,
        Some(&mut score),
        None,
        Some(&mut tl),
        &no_array(),
    )?;

    let br = tl + Point::from_size(template.size().unwrap());
    if cfg!(debug_assertions) {
        println!("player detection: {:?} - {:?} -> {}", tl, br, score);
    }
    if score >= threshold {
        Ok(Rect::from_points(tl, br))
    } else {
        Err(Error::PlayerNotFound)
    }
}

pub fn detect_minimap(grayscale: &impl ToInputArray, threshold: f64) -> Result<Rect, Error> {
    let tl_template = &*MINIMAP_TOP_LEFT;
    let br_template = &*MINIMAP_BOTTOM_RIGHT;
    let br_size = br_template.size().unwrap();
    let mut tl = Point::default();
    let mut tl_score = 0f64;
    let mut tl_result = Mat::default();
    let mut br = Point::default();
    let mut br_score = 0f64;
    let mut br_result = Mat::default();

    match_template_def(grayscale, tl_template, &mut tl_result, TM_CCOEFF_NORMED)?;
    match_template_def(grayscale, br_template, &mut br_result, TM_CCOEFF_NORMED)?;
    min_max_loc(
        &tl_result,
        None,
        Some(&mut tl_score),
        None,
        Some(&mut tl),
        &no_array(),
    )?;
    min_max_loc(
        &br_result,
        None,
        Some(&mut br_score),
        None,
        Some(&mut br),
        &no_array(),
    )?;

    let score = (tl_score + br_score) / 2.;
    if cfg!(debug_assertions) {
        println!(
            "minimap detection: {:?} / {} - {:?} / {} -> {}",
            tl, tl_score, br, br_score, score
        );
    }

    if score >= threshold {
        Ok(Rect::from_points(tl, br + Point::from_size(br_size)))
    } else {
        Err(Error::MinimapNotFound)
    }
}

pub fn to_ranges(rect: &Rect) -> Result<Vector<Range>, Error> {
    let mut vec = Vector::new();
    let rows = Range::new(rect.tl().y, rect.br().y)?;
    let cols = Range::new(rect.tl().x, rect.br().x)?;
    vec.push(rows);
    vec.push(cols);
    Ok(vec)
}

pub fn to_grayscale(
    mat: &impl ToInputArray,
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
