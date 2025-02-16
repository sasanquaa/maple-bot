use log::debug;
use opencv::core::Mat;
use opencv::core::MatTraitConst;
use opencv::core::ModifyInplace;
use opencv::core::Point;
use opencv::core::Rect;
use opencv::core::Scalar;
use opencv::core::Size;
use opencv::core::add_weighted_def;
use opencv::imgproc::COLOR_BGRA2GRAY;
use opencv::imgproc::cvt_color_def;
use opencv::imgproc::{FONT_HERSHEY_SIMPLEX, put_text_def, rectangle_def};
use opencv::{
    highgui::{imshow, wait_key},
    imgcodecs::imwrite_def,
};
use platforms::windows::keys::KeyKind;
use std::sync::LazyLock;
use std::{fs, path::PathBuf, str::FromStr};

use rand::distr::{Alphanumeric, SampleString};

static DATASET_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = PathBuf::from_str(env!("OUT_DIR")).unwrap().join("dataset");
    fs::create_dir_all(dir.clone()).unwrap();
    dir
});

#[allow(unused)]
pub fn debug_mat(mat: &impl MatTraitConst, wait: i32, bboxes: &[Rect], text: &[&str]) {
    let mut mat = mat.try_clone().unwrap();
    for (bbox, &text) in bboxes.iter().zip(text) {
        let _ = rectangle_def(&mut mat, *bbox, Scalar::new(255.0, 0.0, 0.0, 0.0));
        let _ = put_text_def(
            &mut mat,
            text,
            bbox.tl() - Point::new(0, 10),
            FONT_HERSHEY_SIMPLEX,
            0.9,
            Scalar::new(0.0, 255.0, 0.0, 0.0),
        );
    }
    let _ = imshow("Debug", &mat);
    let _ = wait_key(wait);
}

#[allow(unused)]
pub fn save_image_for_training(mat: &Mat) {
    let name = Alphanumeric.sample_string(&mut rand::rng(), 8);
    let mat = to_grayscale(mat);
    let image = LazyLock::force(&DATASET_DIR).join(format!("{name}.png"));

    debug_mat(&mat, 0, &[], &[]);

    imwrite_def(image.to_str().unwrap(), &mat).unwrap();
}

#[allow(unused)]
pub fn save_rune_for_training(
    mat: &Mat,
    preds: &Vec<&[f32]>,
    arrows: &[KeyKind; 4],
    w_ratio: f32,
    h_ratio: f32,
) {
    let name = Alphanumeric.sample_string(&mut rand::rng(), 8);
    let size = mat.size().unwrap();
    let bboxes = preds
        .iter()
        .map(|pred| {
            let tl_x = (pred[0] * w_ratio).max(0.0).min(size.width as f32) as i32;
            let tl_y = (pred[1] * h_ratio).max(0.0).min(size.height as f32) as i32;
            let br_x = (pred[2] * w_ratio).max(0.0).min(size.width as f32) as i32;
            let br_y = (pred[3] * h_ratio).max(0.0).min(size.height as f32) as i32;
            Rect::from_points(Point::new(tl_x, tl_y), Point::new(br_x, br_y))
        })
        .collect::<Vec<Rect>>();
    let texts = arrows
        .iter()
        .map(|arrow| match arrow {
            KeyKind::Up => "up",
            KeyKind::Down => "down",
            KeyKind::Left => "left",
            KeyKind::Right => "right",
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    debug!("{preds:?}");
    debug_mat(mat, 0, &bboxes, &texts);

    let labels = bboxes
        .iter()
        .zip(arrows)
        .map(|(bbox, arrow)| {
            let label = match arrow {
                KeyKind::Up => 0,
                KeyKind::Down => 1,
                KeyKind::Left => 2,
                KeyKind::Right => 3,
                _ => unreachable!(),
            };
            to_yolo_format(label, size, bbox)
        })
        .collect::<Vec<String>>()
        .join("\n");

    let dataset = LazyLock::force(&DATASET_DIR);
    let label = dataset.join(format!("{name}.txt"));
    let image = dataset.join(format!("{name}.png"));

    imwrite_def(image.to_str().unwrap(), mat).unwrap();
    fs::write(label, labels).unwrap();
}

#[allow(unused)]
fn save_minimap_for_training(mat: &Mat, minimap: &Rect) {
    let name = Alphanumeric.sample_string(&mut rand::rng(), 8);
    let dataset = LazyLock::force(&DATASET_DIR);
    let label = dataset.join(format!("{name}.txt"));
    let image = dataset.join(format!("{name}.png"));

    debug_mat(&mat.roi(*minimap).unwrap(), 0, &[], &[]);

    imwrite_def(image.to_str().unwrap(), mat).unwrap();
    fs::write(label, to_yolo_format(0, mat.size().unwrap(), minimap)).unwrap();
}

fn to_yolo_format(label: u32, size: Size, bbox: &Rect) -> String {
    let x_center = bbox.x + bbox.width / 2;
    let y_center = bbox.y + bbox.height / 2;
    let x_center = x_center as f32 / size.width as f32;
    let y_center = y_center as f32 / size.height as f32;
    let width = bbox.width as f32 / size.width as f32;
    let height = bbox.height as f32 / size.height as f32;
    format!("{} {} {} {} {}", label, x_center, y_center, width, height)
}

fn to_grayscale(mat: &impl MatTraitConst) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: all of the functions below can be called in place.
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2GRAY).unwrap();
            add_weighted_def(mat, 1.5, mat, 0.0, -80.0, mat_mut).unwrap();
        });
    }
    mat
}
