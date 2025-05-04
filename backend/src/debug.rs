use std::env;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, path::PathBuf};

use opencv::core::ModifyInplace;
use opencv::core::Point;
use opencv::core::Rect;
use opencv::core::Scalar;
use opencv::core::Size;
use opencv::core::add_weighted_def;
use opencv::core::{Mat, ToInputArray};
use opencv::core::{MatTraitConst, Vector};
use opencv::highgui::destroy_all_windows;
use opencv::imgproc::cvt_color_def;
use opencv::imgproc::line_def;
use opencv::imgproc::rectangle;
use opencv::imgproc::{COLOR_BGRA2GRAY, draw_contours_def};
use opencv::imgproc::{FONT_HERSHEY_SIMPLEX, put_text_def};
use opencv::imgproc::{LINE_8, circle_def};
use opencv::{
    highgui::{imshow, wait_key},
    imgcodecs::imwrite_def,
};
use platforms::windows::KeyKind;
use rand::distr::{Alphanumeric, SampleString};

static DATASET_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("dataset");
    fs::create_dir_all(dir.clone()).unwrap();
    dir
});

static DATASET_MINIMAP_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = DATASET_DIR.join("minimap");
    fs::create_dir_all(dir.clone()).unwrap();
    dir
});

static DATASET_RUNE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let dir = DATASET_DIR.join("rune");
    fs::create_dir_all(dir.clone()).unwrap();
    dir
});

#[allow(unused)]
pub fn debug_spinning_arrows(
    mat: &impl MatTraitConst,
    spin_arrow_contours: &Vector<Vector<Point>>,
    spin_arrow_region: Rect,
    spin_arrow_last_head: Point,
    spin_arrow_cur_head: Point,
    spin_arrow_centroid: Point,
) {
    let mut mat = mat.try_clone().unwrap();
    let contours = spin_arrow_contours
        .clone()
        .into_iter()
        .map(|points| {
            points
                .into_iter()
                .map(|pt| pt + spin_arrow_region.tl())
                .collect::<Vector<Point>>()
        })
        .collect::<Vector<Vector<Point>>>();

    draw_contours_def(&mut mat, &contours, 0, Scalar::new(255.0, 0.0, 0.0, 0.0));
    circle_def(
        &mut mat,
        spin_arrow_last_head + spin_arrow_centroid,
        3,
        Scalar::new(0.0, 255.0, 0.0, 0.0),
    );
    circle_def(
        &mut mat,
        spin_arrow_cur_head + spin_arrow_centroid,
        3,
        Scalar::new(255.0, 0.0, 0.0, 0.0),
    );
    circle_def(
        &mut mat,
        spin_arrow_centroid,
        3,
        Scalar::new(0.0, 0.0, 255.0, 0.0),
    );
    debug_mat("Spin Arrow", &mat, 0, &[]);
}

#[allow(unused)]
pub fn debug_pathing_points(mat: &impl MatTraitConst, minimap: Rect, points: &[Point]) {
    let mut mat = mat.roi(minimap).unwrap().clone_pointee();
    for i in 0..points.len() - 1 {
        let pt1 = points[i];
        let pt2 = points[i + 1];
        line_def(
            &mut mat,
            Point::new(pt1.x, minimap.height - pt1.y),
            Point::new(pt2.x, minimap.height - pt2.y),
            Scalar::new(
                rand::random_range(100.0..255.0),
                rand::random_range(100.0..255.0),
                rand::random_range(100.0..255.0),
                0.0,
            ),
        )
        .unwrap();
    }
    debug_mat("Pathing", &mat, 1, &[]);
}

#[allow(unused)]
pub fn debug_mat(name: &str, mat: &impl MatTraitConst, wait: i32, bboxes: &[(Rect, &str)]) -> i32 {
    let mut mat = mat.try_clone().unwrap();
    for (bbox, text) in bboxes {
        let _ = rectangle(
            &mut mat,
            *bbox,
            Scalar::new(255.0, 0.0, 0.0, 0.0),
            1,
            LINE_8,
            0,
        );
        let _ = put_text_def(
            &mut mat,
            text,
            bbox.tl() - Point::new(0, 10),
            FONT_HERSHEY_SIMPLEX,
            0.9,
            Scalar::new(0.0, 255.0, 0.0, 0.0),
        );
    }
    imshow(name, &mat).unwrap();
    let result = wait_key(wait).unwrap();
    destroy_all_windows().unwrap();
    result
}

#[allow(unused)]
pub fn save_image_for_training(mat: &impl MatTraitConst, is_grayscale: bool, view: bool) {
    save_image_for_training_to(mat, None, is_grayscale, view);
}

#[allow(unused)]
pub fn save_image_for_training_to(
    mat: &impl MatTraitConst,
    folder: Option<String>,
    is_grayscale: bool,
    view: bool,
) {
    let name = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let mat = if is_grayscale {
        to_grayscale(mat)
    } else {
        mat.try_clone().unwrap() // No point in cloning except for having the same type
    };
    let folder = if let Some(id) = folder {
        let dir = DATASET_DIR.join(id.as_str());
        if !dir.exists() {
            fs::create_dir_all(dir.clone()).unwrap();
        }
        dir
    } else {
        DATASET_DIR.clone()
    };
    let image = folder.join(format!("{name}.png"));

    if view {
        debug_mat("Image", &mat, 0, &[]);
    }

    imwrite_def(image.to_str().unwrap(), &mat).unwrap();
}

#[allow(unused)]
pub fn debug_rune(mat: &Mat, preds: &Vec<&[f32]>, w_ratio: f32, h_ratio: f32) {
    let size = mat.size().unwrap();
    let bboxes = preds
        .iter()
        .map(|pred| map_bbox_from_prediction(pred, size, w_ratio, h_ratio))
        .collect::<Vec<Rect>>();
    let texts = preds
        .iter()
        .map(|pred| match pred[5] as i32 {
            0 => "up",
            1 => "down",
            2 => "left",
            3 => "right",
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    debug_mat(
        "Rune",
        mat,
        1,
        &bboxes.into_iter().zip(texts).collect::<Vec<_>>(),
    );
}

#[allow(unused)]
pub fn save_rune_for_training<T: MatTraitConst + ToInputArray>(
    mat: &T,
    preds: &[Vec<f32>],
    arrows: &[KeyKind; 4],
    w_ratio: f32,
    h_ratio: f32,
) {
    let name = Alphanumeric.sample_string(&mut rand::rng(), 8);
    let size = mat.size().unwrap();
    let bboxes = preds
        .iter()
        .map(|pred| map_bbox_from_prediction(pred, size, w_ratio, h_ratio))
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

    let key = debug_mat(
        "Training",
        mat,
        0,
        &bboxes.clone().into_iter().zip(texts).collect::<Vec<_>>(),
    );
    if key == 97 {
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
                to_yolo_format(label, size, *bbox)
            })
            .collect::<Vec<String>>()
            .join("\n");

        let dataset = &DATASET_RUNE_DIR;
        let label = dataset.join(format!("{name}.txt"));
        let image = dataset.join(format!("{name}.png"));

        imwrite_def(image.to_str().unwrap(), mat).unwrap();
        fs::write(label, labels).unwrap();
    }
}

#[allow(unused)]
pub fn save_mobs_for_training(mat: &Mat, mobs: &[Rect]) {
    let name = Alphanumeric.sample_string(&mut rand::rng(), 8);
    let dataset = LazyLock::force(&DATASET_DIR);
    let label = dataset.join(format!("{name}.txt"));
    let image = dataset.join(format!("{name}.png"));
    let mut labels = Vec::<String>::new();
    for mob in mobs.iter().copied() {
        labels.push(to_yolo_format(0, mat.size().unwrap(), mob));
    }

    let key = debug_mat(
        "Training",
        mat,
        0,
        &mobs
            .iter()
            .copied()
            .map(|bbox| (bbox, "Mobs"))
            .collect::<Vec<_>>(),
    );
    if key == 97 {
        imwrite_def(image.to_str().unwrap(), mat).unwrap();
        fs::write(label, labels.join("\n")).unwrap();
    }
}

#[allow(unused)]
pub fn save_minimap_for_training<T: MatTraitConst + ToInputArray>(mat: &T, minimap: Rect) {
    let name = Alphanumeric.sample_string(&mut rand::rng(), 8);
    let dataset = &DATASET_MINIMAP_DIR;
    let label = dataset.join(format!("{name}.txt"));
    let image = dataset.join(format!("{name}.png"));

    let key = debug_mat("Training", mat, 0, &[(minimap, "Minimap")]);
    if key == 97 {
        imwrite_def(image.to_str().unwrap(), mat).unwrap();
        fs::write(label, to_yolo_format(0, mat.size().unwrap(), minimap)).unwrap();
    }
}

fn map_bbox_from_prediction(pred: &[f32], size: Size, w_ratio: f32, h_ratio: f32) -> Rect {
    let tl_x = (pred[0] / w_ratio).max(0.0).min(size.width as f32) as i32;
    let tl_y = (pred[1] / h_ratio).max(0.0).min(size.height as f32) as i32;
    let br_x = (pred[2] / w_ratio).max(0.0).min(size.width as f32) as i32;
    let br_y = (pred[3] / h_ratio).max(0.0).min(size.height as f32) as i32;
    Rect::from_points(Point::new(tl_x, tl_y), Point::new(br_x, br_y))
}

fn to_yolo_format(label: u32, size: Size, bbox: Rect) -> String {
    let x_center = bbox.x + bbox.width / 2;
    let y_center = bbox.y + bbox.height / 2;
    let x_center = x_center as f32 / size.width as f32;
    let y_center = y_center as f32 / size.height as f32;
    let width = bbox.width as f32 / size.width as f32;
    let height = bbox.height as f32 / size.height as f32;
    format!("{label} {x_center} {y_center} {width} {height}")
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
