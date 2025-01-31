use std::sync::{LazyLock, Mutex};

use ndarray::{ArrayView, Axis, s};
use opencv::{
    core::{
        CV_32FC3, Mat, MatTraitConst, ModifyInplace, Point, Range, Rect, Rect2f, Scalar, Size,
        ToInputArray, Vector, add_weighted_def, min_max_loc, no_array,
    },
    dnn::{
        ModelTrait, TextDetectionModel_DB, TextDetectionModel_DBTrait,
        TextDetectionModelTraitConst, TextRecognitionModel, TextRecognitionModelTrait,
        TextRecognitionModelTraitConst,
    },
    highgui::{imshow, wait_key},
    imgcodecs::{self, IMREAD_GRAYSCALE},
    imgproc::{
        COLOR_BGRA2GRAY, COLOR_BGRA2RGB, INTER_AREA, LINE_8, TM_CCOEFF_NORMED, cvt_color_def,
        match_template_def, polylines, rectangle_def, resize,
    },
};
use ort::{
    execution_providers::CUDAExecutionProvider,
    session::{Session, builder::GraphOptimizationLevel},
};

use crate::error::Error;

static MINIMAP_MODEL: LazyLock<Session> = LazyLock::new(|| {
    Session::builder()
        .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
        .and_then(|b| b.with_execution_providers([CUDAExecutionProvider::default().build()]))
        .and_then(|b| b.commit_from_memory(include_bytes!(env!("MINIMAP_MODEL"))))
        .expect("unable to build minimap detection session")
});

static TEXT_RECOGNITION_MODEL: LazyLock<Mutex<TextRecognitionModel>> = LazyLock::new(|| {
    Mutex::new(
        TextRecognitionModel::from_file_def(env!("TEXT_RECOGNITION_MODEL"))
            .and_then(|mut model| model.set_decode_type("CTC-greedy"))
            .and_then(|mut model| {
                let vocab = include_str!(env!("TEXT_RECOGNITION_VOCAB"))
                    .lines()
                    .collect::<Vector<String>>();
                model.set_vocabulary(&vocab)
            })
            .and_then(|mut model| {
                model
                    .set_input_params(
                        1.0 / 127.5,
                        Size::new(100, 32),
                        Scalar::new(127.5, 127.5, 127.5, 0.0),
                        false,
                        false,
                    )
                    .map(|_| model)
            })
            .expect("unable to build text recognition model"),
    )
});

static TEXT_DETECTION_MODEL: LazyLock<Mutex<TextDetectionModel_DB>> = LazyLock::new(|| {
    Mutex::new(
        TextDetectionModel_DB::new_def(env!("TEXT_DETECTION_MODEL"))
            .and_then(|mut model| model.set_binary_threshold(0.3))
            .and_then(|mut model| model.set_polygon_threshold(0.5))
            .and_then(|mut model| model.set_max_candidates(200))
            .and_then(|mut model| model.set_unclip_ratio(2.0))
            .and_then(|mut model| {
                model
                    .set_input_params(
                        1.0 / 255.0,
                        Size::new(736, 736),
                        Scalar::new(122.67891434, 116.66876762, 104.00698793, 0.0),
                        false,
                        false,
                    )
                    .map(|_| model)
            })
            .expect("unable to build text detection model"),
    )
});

const MINIMAP_WIDTH: i32 = 640;

const MINIMAP_HEIGHT: i32 = 640;

static PLAYER: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(include_bytes!(env!("PLAYER_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
});

static ERDA_SHOWER: LazyLock<Mat> = LazyLock::new(|| {
    imgcodecs::imdecode(
        include_bytes!(env!("ERDA_SHOWER_TEMPLATE")),
        IMREAD_GRAYSCALE,
    )
    .unwrap()
});

pub fn detect_erda_shower(grayscale: &impl ToInputArray, threshold: f64) -> Result<Rect, Error> {
    let template = &*ERDA_SHOWER;
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
    if score >= threshold {
        Ok(Rect::from_points(tl, br))
    } else {
        Err(Error::PlayerNotFound)
    }
}

pub fn detect_player(mat: &Mat, minimap: &Rect, threshold: f64) -> Result<Rect, Error> {
    let mat = to_grayscale(mat)?;
    let mat = mat.ranges(&to_ranges(minimap)?)?;
    let template = LazyLock::force(&PLAYER);
    let mut result = Mat::default();
    let mut score = 0f64;
    let mut tl = Point::default();

    match_template_def(&mat, template, &mut result, TM_CCOEFF_NORMED)?;
    min_max_loc(
        &result,
        None,
        Some(&mut score),
        None,
        Some(&mut tl),
        &no_array(),
    )?;

    let br = tl + Point::from_size(template.size().unwrap());
    if score >= threshold {
        Ok(Rect::from_points(tl + minimap.tl(), br + minimap.tl()))
    } else {
        Err(Error::PlayerNotFound)
    }
}

pub fn detect_minimap(mat: &Mat, threshold: f32) -> Result<Rect, Error> {
    let original_size = mat.size()?;
    let mut test = mat.clone();
    let mat = to_transformed(mat)?;
    let size = mat.size()?;
    let w_ratio = original_size.width as f32 / size.width as f32;
    let h_ratio = original_size.height as f32 / size.height as f32;
    // SAFETY: TODO
    let array = unsafe {
        ArrayView::from_shape_ptr(
            [
                1,
                size.height as usize,
                size.width as usize,
                mat.channels() as usize,
            ],
            mat.data().cast::<f32>(),
        )
    }
    .permuted_axes([0, 3, 1, 2]);
    let result = MINIMAP_MODEL.run(ort::inputs![array]?)?;
    let outputs = result["output0"].try_extract_tensor::<f32>()?;
    let outputs = outputs.slice(s![0, .., ..]);
    let p = outputs.axis_iter(Axis(0)).max_by(|a, b| {
        // SAFETY: a and b have shapes [bbox(4) + class(1)]
        let a = unsafe { a.uget(4usize) };
        let b = unsafe { b.uget(4usize) };
        a.total_cmp(b)
    });
    let bbox = p.map(|p| {
        // SAFETY: p has shape [bbox(4) + class(1)]
        let tl_x = unsafe { p.uget(0usize) } * w_ratio;
        let tl_y = unsafe { p.uget(1usize) } * h_ratio;
        let br_x = unsafe { p.uget(2usize) } * w_ratio;
        let br_y = unsafe { p.uget(3usize) } * h_ratio;
        Rect::from_points(
            Point::new(tl_x as i32, tl_y as i32),
            Point::new(br_x as i32, br_y as i32),
        )
    });
    // println!("{:?}", bbox);
    // let mut nms = Vec::<(Rect2f, f32)>::new();
    // let mut nms_truncate = 0;
    // for p in outputs.axis_iter(Axis(0)) {
    //     // SAFETY: the output has shape [batch, bbox(4) + class(1), preds]
    //     let conf = unsafe { *p.uget(4usize) };
    //     if conf < threshold {
    //         continue;
    //     }
    //     let cx = unsafe { p.uget(0usize) } * w_ratio;
    //     let cy = unsafe { p.uget(1usize) } * h_ratio;
    //     let w = unsafe { p.uget(2usize) } * w_ratio;
    //     let h = unsafe { p.uget(3usize) } * h_ratio;
    //     let x = f32::min(f32::max(cx - w / 2.0, 0.0), original_size.width as f32);
    //     let y = f32::min(f32::max(cy - h / 2.0, 0.0), original_size.height as f32);
    //     let bbox = Rect2f::new(x, y, w, h);
    //     nms.push((bbox, conf));
    // }
    // nms.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    // for i in 0..nms.len() {
    //     let mut drop = false;
    //     for j in 0..nms_truncate {
    //         let bbox_i = nms[i].0;
    //         let bbox_j = nms[j].0;
    //         let intersection = (bbox_i & bbox_j).area();
    //         let union = bbox_i.area() + bbox_j.area() - intersection;
    //         let iou = intersection / union;
    //         if iou > 0.45 {
    //             drop = true;
    //             break;
    //         }
    //     }
    //     if !drop {
    //         nms.swap(nms_truncate, i);
    //         nms_truncate += 1;
    //     }
    // }
    // if cfg!(debug_assertions) {
    //     println!("minimap detection results: {:?}", nms);
    // }
    // nms.first()
    //     .map(|result| {
    //         Rect::new(
    //             result.0.x as i32,
    //             result.0.y as i32,
    //             result.0.width as i32,
    //             result.0.height as i32,
    //         )
    //     })
    //     .ok_or(Error::MinimapNotFound)
    Err(Error::MinimapNotFound)
}

pub fn detect_minimap_name<'a>(mat: &Mat, minimap: &Rect) -> Result<String, Error> {
    let bbox = Rect::from_points(
        Point::new(minimap.x, 0),
        Point::new(minimap.x + minimap.width, minimap.y),
    );
    let mut mat = mat
        .ranges(&to_ranges(&bbox)?)?
        .try_clone()
        .and_then(|mut mat| {
            // SAFETY: cvt_color_def can be called in place
            unsafe {
                mat.modify_inplace(|mat, mut mat_mut| {
                    cvt_color_def(&mat, &mut mat_mut, COLOR_BGRA2RGB)
                })
            }
            .map(|_| mat)
        })?;
    let mut detections = Vector::<Vector<Point>>::new();
    TEXT_DETECTION_MODEL
        .lock()
        .map_err(|_| Error::MinimapNotFound)?
        .detect(&mat, &mut detections)?;
    println!("{:?}", detections);
    let bboxes = detections
        .iter()
        .flatten()
        .max_by(|a, b| a.y.cmp(&b.y))
        .map(|p| p.y)
        .map(|y| {
            detections
                .iter()
                .filter_map(|bbox| {
                    // SAFETY: bbox has shape [bl, tl, tr, br]
                    let bl_y = unsafe { bbox.get_unchecked(0).y };
                    let br_y = unsafe { bbox.get_unchecked(3).y };
                    if y - bl_y <= 3 && y - br_y <= 3 {
                        Some(Rect::from_points(
                            unsafe { bbox.get_unchecked(1) },
                            unsafe { bbox.get_unchecked(3) },
                        ))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
    // polylines(
    //     &mut mat,
    //     &detections,
    //     true,
    //     Scalar::new(0.0, 255.0, 0.0, 0.0),
    //     2,
    //     LINE_8,
    //     0,
    // );
    // imshow("winname", &mat);
    // wait_key(0);
    let bbox = bboxes
        .and_then(|bboxes| bboxes.into_iter().reduce(|a, b| a | b))
        .map(|bbox| Rect::new(bbox.x, bbox.y, minimap.width - bbox.x, bbox.height))
        .ok_or(Error::MinimapNotFound)?;
    println!("alo {:?} {:?}", bbox, mat.size());
    let mat = mat.ranges(&to_ranges(&bbox)?)?;
    imshow("winname2", &mat);
    wait_key(0);

    let text = TEXT_RECOGNITION_MODEL
        .lock()
        .map_err(|_| Error::MinimapNotFound)?
        .recognize(&mat);
    println!("{:?}", text);
    text.map_err(|_| Error::MinimapNotFound)
}

fn to_ranges(rect: &Rect) -> Result<Vector<Range>, Error> {
    let mut vec = Vector::new();
    let rows = Range::new(rect.tl().y, rect.br().y)?;
    let cols = Range::new(rect.tl().x, rect.br().x)?;
    vec.push(rows);
    vec.push(cols);
    Ok(vec)
}

fn to_transformed(mat: &Mat) -> Result<Mat, Error> {
    let mut mat = mat.clone();
    // SAFETY: cvt_color_def, resize and convert_to can be
    // used in place
    unsafe {
        mat.modify_inplace::<opencv::Result<()>>(|mat, mut mat_mut| {
            cvt_color_def(&mat, &mut mat_mut, COLOR_BGRA2RGB)
                .and_then(|_| {
                    resize(
                        &mat,
                        &mut mat_mut,
                        Size::new(MINIMAP_WIDTH, MINIMAP_HEIGHT),
                        0.0,
                        0.0,
                        INTER_AREA,
                    )
                })
                .and_then(|_| mat.convert_to(&mut mat_mut, CV_32FC3, 1.0 / 255.0, 0.0))
        })?
    };
    Ok(mat)
}

fn to_grayscale(mat: &Mat) -> Result<Mat, Error> {
    let mut mat = mat.clone();
    unsafe {
        // SAFETY: cvt_color_def, add_weighted_def can be called in place.
        mat.modify_inplace::<opencv::Result<()>>(|mat, mut mat_mut| {
            cvt_color_def(mat, &mut mat_mut, COLOR_BGRA2GRAY)
                .and_then(|_| add_weighted_def(&mat, 1.5, &mat, 0.0, -80.0, &mut mat_mut))
        })?
    };
    Ok(mat)
}
