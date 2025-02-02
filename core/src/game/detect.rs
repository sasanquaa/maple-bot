use core::slice::SlicePattern;
use std::sync::LazyLock;

use anyhow::{Ok, Result, anyhow};
use opencv::{
    boxed_ref::BoxedRef,
    core::{
        CV_8U, CV_32FC3, CV_32S, CmpTypes, Mat, MatExprTraitConst, MatTrait, MatTraitConst,
        MatTraitConstManual, ModifyInplace, Point, Range, Rect, Scalar, Size, ToInputArray, Vector,
        add, add_weighted_def, bitwise_and_def, compare, divide2_def, find_non_zero, min_max_loc,
        no_array, subtract_def, transpose_nd,
    },
    imgcodecs::{self, IMREAD_GRAYSCALE},
    imgproc::{
        CC_STAT_AREA, CC_STAT_HEIGHT, CC_STAT_LEFT, CC_STAT_TOP, CC_STAT_WIDTH,
        CHAIN_APPROX_SIMPLE, COLOR_BGRA2GRAY, COLOR_BGRA2RGB, INTER_AREA, INTER_LINEAR, MORPH_RECT,
        RETR_EXTERNAL, THRESH_OTSU, TM_CCOEFF_NORMED, bounding_rect,
        connected_components_with_stats, cvt_color_def, dilate_def, find_contours_def,
        get_structuring_element_def, match_template_def, min_area_rect, resize, threshold,
    },
    traits::OpenCVIntoExternContainer,
};
use ort::{
    execution_providers::CUDAExecutionProvider,
    session::{Session, SessionInputValue, SessionOutputs, builder::GraphOptimizationLevel},
    value::Tensor,
};

static MINIMAP_MODEL: LazyLock<Session> = LazyLock::new(|| {
    Session::builder()
        .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
        .and_then(|b| b.with_execution_providers([CUDAExecutionProvider::default().build()]))
        .and_then(|b| b.commit_from_memory(include_bytes!(env!("MINIMAP_MODEL"))))
        .expect("unable to build minimap detection session")
});

static TEXT_DETECTION_MODEL: LazyLock<Session> = LazyLock::new(|| {
    Session::builder()
        .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
        .and_then(|b| b.with_execution_providers([CUDAExecutionProvider::default().build()]))
        .and_then(|b| b.commit_from_memory(include_bytes!(env!("TEXT_DETECTION_MODEL"))))
        .expect("unable to build minimap detection session")
});

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

pub fn detect_erda_shower(grayscale: &impl ToInputArray, threshold: f64) -> Result<Rect> {
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
        Err(anyhow!("erda shower skill not found"))
    }
}

pub fn detect_player(mat: &Mat, minimap: &Rect, threshold: f64) -> Result<Rect> {
    let mat = to_grayscale(mat, true)?;
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
        Err(anyhow!("player not found"))
    }
}

pub fn detect_minimap(mat: &Mat, confidence_threshold: f32) -> Result<Rect> {
    let size = mat.size()?;
    let (mat_in, w_ratio, h_ratio) = to_transformed_for_minimap(mat)?;
    let result = MINIMAP_MODEL.run([to_session_input_value(&mat_in)?])?;
    let mat_out = from_session_output_value(&result)?;
    let pred = (0..mat_out.rows())
        // SAFETY: 0..outputs.rows() is within Mat bounds
        .map(|i| {
            unsafe { mat_out.at_row_unchecked::<f32>(i) }
                .expect("unable to get row but row is within bound")
        })
        .max_by(|&a, &b| {
            // a and b have shapes [bbox(4) + class(1)]
            a[4].total_cmp(&b[4])
        });
    let bbox = pred.and_then(|pred| {
        if cfg!(debug_assertions) {
            println!("minimap detection: {:?}", pred);
        }
        if pred[4] < confidence_threshold {
            None
        } else {
            let tl_x = (pred[0] * w_ratio).max(0.0).min(size.width as f32);
            let tl_y = (pred[1] * h_ratio).max(0.0).min(size.height as f32);
            let br_x = (pred[2] * w_ratio).max(0.0).min(size.width as f32);
            let br_y = (pred[3] * h_ratio).max(0.0).min(size.height as f32);
            Some(Rect::from_points(
                Point::new(tl_x as i32, tl_y as i32),
                Point::new(br_x as i32, br_y as i32),
            ))
        }
    });
    // expands out a few pixels (2) to include the whole border
    fn expand_bbox(bbox: &Rect) -> Rect {
        let x = (bbox.x - 2).max(0);
        let y = (bbox.y - 2).max(0);
        let x_size = (bbox.x - x) * 2;
        let y_size = (bbox.y - y) * 2;
        Rect::new(x, y, bbox.width + x_size, bbox.height + y_size)
    }
    let minimap = bbox
        .map(|bbox| expand_bbox(&bbox))
        .and_then(|bbox| {
            // grayscale
            let ranges = to_ranges(&bbox).ok()?;
            let minimap = mat.ranges(&ranges).ok()?;
            to_grayscale(&minimap, false).ok()
        })
        .and_then(|mut mat| {
            // threshold
            unsafe {
                mat.modify_inplace(|mat, mut mat_mut| {
                    threshold(&mat, &mut mat_mut, 0.0, 255.0, THRESH_OTSU).ok()
                })?;
            }
            Some(mat)
        });
    let contours = minimap.and_then(|mat| {
        let mut vec = Vector::<Vector<Point>>::new();
        find_contours_def(&mat, &mut vec, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE).ok()?;
        Some(vec)
    });
    let bound = contours.and_then(|vec| {
        if cfg!(debug_assertions) {
            println!("minimap contours: {:?}", vec);
        }
        vec.into_iter()
            .map(|contour| {
                bounding_rect(&contour).expect("contour found but unable to retrieve bounding rect")
            })
            .max_by(|a, b| a.area().cmp(&b.area()))
    });
    bound
        .and_then(|bound| {
            let bbox = expand_bbox(&bbox.unwrap());
            let bound = Rect::from_points(bound.tl() + bbox.tl(), bound.br() + bbox.tl());
            if cfg!(debug_assertions) {
                println!(
                    "minimap bbox and contour areas: {:?} {:?}",
                    bbox.area(),
                    bound.area()
                );
            }
            // the detected contour should be contained
            // inside the detected minimap when expanded
            if (bbox & bound) == bound && (bbox.area() - bound.area()) >= 1500 {
                Some(bound)
            } else {
                None
            }
        })
        .ok_or(anyhow!("minimap not found"))
}

pub fn detect_minimap_name<'a>(mat: &Mat, minimap: &Rect, score_threshold: f64) -> Result<Rect> {
    fn extract_bboxes(
        mat: &BoxedRef<Mat>,
        w_ratio: f32,
        h_ratio: f32,
        offset: i32,
        score_threshold: f64,
    ) -> Result<Vec<Rect>> {
        let text_score_ranges =
            Vector::from_iter([Range::all()?, Range::all()?, Range::new(0, 1)?]);
        let text_score = mat.ranges(&text_score_ranges)?.clone_pointee();
        let text_score = text_score.reshape_nd(1, &text_score.mat_size().as_slice()[..2])?;
        let mut text_lower_score = Mat::default();
        threshold(&text_score, &mut text_lower_score, 0.4, 1.0, 0)?;
        let link_score_ranges =
            Vector::from_iter([Range::all()?, Range::all()?, Range::new(1, 2)?]);
        let mut link_score = Mat::default();
        threshold(
            &mat.ranges(&link_score_ranges)?,
            &mut link_score,
            0.4,
            1.0,
            0,
        )?;
        let mut combined_score = Mat::default();
        add(
            &text_lower_score,
            &link_score,
            &mut combined_score,
            &no_array(),
            CV_8U,
        )?;
        let mut bboxes = Vec::<Rect>::new();
        let mut labels = Mat::default();
        let mut stats = Mat::default();
        let labels_count = connected_components_with_stats(
            &combined_score,
            &mut labels,
            &mut stats,
            &mut Mat::default(),
            4,
            CV_32S,
        )?;
        for i in 1..labels_count {
            let area = *stats.at_2d::<i32>(i, CC_STAT_AREA)?;
            if area < 10 {
                continue;
            }

            let mut mask = Mat::default();
            let mut mask_max_score = 0.0f64;
            compare(
                &labels,
                &Scalar::all(i as f64),
                &mut mask,
                CmpTypes::CMP_EQ as i32,
            )?;
            min_max_loc(
                &text_score,
                None,
                Some(&mut mask_max_score),
                None,
                None,
                &mask,
            )?;
            if mask_max_score < score_threshold {
                continue;
            }

            let shape = mask.size()?;
            let x = *stats.at_2d::<i32>(i, CC_STAT_LEFT)?;
            let y = *stats.at_2d::<i32>(i, CC_STAT_TOP)?;
            let w = *stats.at_2d::<i32>(i, CC_STAT_WIDTH)?;
            let h = *stats.at_2d::<i32>(i, CC_STAT_HEIGHT)?;
            let size =
                ((area as f32 * w.min(h) as f32 / (w as f32 * h as f32)).sqrt() * 2.0) as i32;
            let sx = (x - size).max(0);
            let sy = (y - size).max(0);
            let ex = (x + w + size + 1).min(shape.width);
            let ey = (y + h + size + 1).min(shape.height);
            let kernel = get_structuring_element_def(MORPH_RECT, Size::new(size + 1, size + 1))?;

            let mut link_mask = Mat::default();
            let mut text_mask = Mat::default();
            let mut and_mask = Mat::default();
            let mut seg_map = Mat::zeros(shape.height, shape.width, CV_8U)?.to_mat()?;
            compare(
                &link_score,
                &Scalar::all(1.0),
                &mut link_mask,
                CmpTypes::CMP_EQ as i32,
            )?;
            compare(
                &text_score,
                &Scalar::all(0.0),
                &mut text_mask,
                CmpTypes::CMP_EQ as i32,
            )?;
            bitwise_and_def(&link_mask, &text_mask, &mut and_mask)?;
            seg_map.set_to(&Scalar::all(255.0), &mask)?;
            seg_map.set_to(&Scalar::all(255.0), &and_mask)?;

            let mut seg_contours = Vector::<Point>::new();
            let mut seg_roi =
                seg_map.roi_mut(Rect::from_points(Point::new(sx, sy), Point::new(ex, ey)))?;
            // SAFETY: all of the functions below can be called in place.
            unsafe {
                seg_roi.modify_inplace::<Result<()>>(|mat, mut mat_mut| {
                    dilate_def(&mat, &mut mat_mut, &kernel)?;
                    mat.copy_to(&mut mat_mut)?;
                    Ok(())
                })?
            }
            find_non_zero(&seg_map, &mut seg_contours)?;

            let rect = min_area_rect(&seg_contours)?.bounding_rect2f()?;
            let tl = rect.tl();
            let tl = Point::new(
                (tl.x * w_ratio * 2.0) as i32 + offset,
                (tl.y * h_ratio * 2.0) as i32,
            );
            let br = rect.br();
            let br = Point::new(
                (br.x * w_ratio * 2.0) as i32 + offset,
                (br.y * h_ratio * 2.0) as i32,
            );
            bboxes.push(Rect::from_points(tl, br));
        }
        Ok(bboxes)
    }

    let (mat, w_ratio, h_ratio, offset) = to_transformed_for_minimap_name(mat, minimap)?;
    let result = TEXT_DETECTION_MODEL.run([to_session_input_value(&mat)?])?;
    let mat_out = from_session_output_value(&result)?;
    let bboxes = extract_bboxes(&mat_out, w_ratio, h_ratio, offset, score_threshold)?;
    let bboxes_max_y = bboxes
        .iter()
        .max_by(|a, b| (a.y + a.height).cmp(&(b.y + b.height)))
        .map(|bbox| (bbox.y + bbox.height))
        .ok_or(anyhow!("minimap name not found"))?;
    let bbox = bboxes
        .into_iter()
        .filter_map(|bbox| {
            if bboxes_max_y - (bbox.y + bbox.height) <= 5 {
                Some(bbox)
            } else {
                None
            }
        })
        .reduce(|a, b| a | b);
    if cfg!(debug_assertions) {
        println!("minimap name detection: {:?}", bbox);
    }
    bbox.ok_or(anyhow!("minimap name not found"))
}

fn to_ranges(rect: &Rect) -> Result<Vector<Range>> {
    let mut vec = Vector::new();
    let rows = Range::new(rect.tl().y, rect.br().y)?;
    let cols = Range::new(rect.tl().x, rect.br().x)?;
    vec.push(rows);
    vec.push(cols);
    Ok(vec)
}

fn to_transformed_for_minimap(mat: &Mat) -> Result<(Mat, f32, f32)> {
    let mut mat = mat.clone();
    let (w_ratio, h_ratio) = to_width_height_ratio(mat.size()?, 640.0, 640.0);
    // SAFETY: all of the functions below can be called in place.
    unsafe {
        mat.modify_inplace::<Result<()>>(|mat, mut mat_mut| {
            cvt_color_def(&mat, &mut mat_mut, COLOR_BGRA2RGB)?;
            resize(
                &mat,
                &mut mat_mut,
                Size::new(640, 640),
                0.0,
                0.0,
                INTER_AREA,
            )?;
            mat.convert_to(&mut mat_mut, CV_32FC3, 1.0 / 255.0, 0.0)?;
            Ok(())
        })?
    }
    Ok((mat, w_ratio, h_ratio))
}

fn to_transformed_for_minimap_name(mat: &Mat, minimap: &Rect) -> Result<(Mat, f32, f32, i32)> {
    let bbox = Rect::from_points(
        Point::new(minimap.x, 0),
        Point::new(minimap.x + minimap.width, minimap.y),
    );
    let mut mat = mat.ranges(&to_ranges(&bbox)?)?.clone_pointee();
    let size = mat.size()?;
    let size_max = size.width.max(size.height) as f32;
    let target_size = (1.5 * size_max).min(1280.0);
    let target_ratio = target_size / size_max;

    let target_w = (target_ratio * size.width as f32) as i32;
    let target_w = target_w + (32 - target_w % 32);
    let target_w_ratio = size.width as f32 / target_w as f32;

    let target_h = (target_ratio * size.height as f32) as i32;
    let target_h = target_h + (32 - target_h % 32);
    let target_h_ratio = size.height as f32 / target_h as f32;
    // SAFETY: all of the below functions can be called in place
    unsafe {
        mat.modify_inplace::<Result<()>>(|mat, mut mat_mut| {
            cvt_color_def(&mat, &mut mat_mut, COLOR_BGRA2RGB)?;
            resize(
                &mat,
                &mut mat_mut,
                Size::new(target_w, target_h),
                0.0,
                0.0,
                INTER_LINEAR,
            )?;
            mat.convert_to(&mut mat_mut, CV_32FC3, 1.0, 0.0)?;
            subtract_def(
                &mat,
                &Scalar::new(123.675, 116.28, 103.53, 0.0),
                &mut mat_mut,
            )?;
            divide2_def(&mat, &Scalar::new(58.395, 57.12, 57.375, 1.0), &mut mat_mut)?;
            Ok(())
        })?
    }
    Ok((mat, target_w_ratio, target_h_ratio, minimap.x))
}

#[inline(always)]
fn to_width_height_ratio(from: Size, to_w: f32, to_h: f32) -> (f32, f32) {
    (from.width as f32 / to_w, from.height as f32 / to_h)
}

fn to_grayscale(mat: &impl MatTraitConst, add_contrast: bool) -> Result<Mat> {
    let mut mat = mat.try_clone()?;
    unsafe {
        // SAFETY: all of the functions below can be called in place.
        mat.modify_inplace::<Result<()>>(|mat, mut mat_mut| {
            cvt_color_def(mat, &mut mat_mut, COLOR_BGRA2GRAY)?;
            if add_contrast {
                add_weighted_def(&mat, 1.5, &mat, 0.0, -80.0, &mut mat_mut)?;
            }
            Ok(())
        })?
    }
    Ok(mat)
}

fn from_session_output_value<'a>(result: &SessionOutputs) -> Result<BoxedRef<'a, Mat>> {
    let (dims, outputs) = result["output0"].try_extract_raw_tensor::<f32>()?;
    let dims = dims.iter().map(|&dim| dim as i32).collect::<Vec<i32>>();
    let mat = Mat::new_nd_with_data(dims.as_slice(), outputs)?;
    let mat = mat.reshape_nd(1, &dims.as_slice()[1..])?;
    let mat = mat.opencv_into_extern_container_nofail();
    Ok(BoxedRef::from(mat))
}

fn to_session_input_value(mat: &Mat) -> Result<SessionInputValue> {
    let shape = [1]
        .into_iter()
        .chain(mat.mat_size().iter().copied())
        .chain([mat.channels()])
        .collect::<Vec<i32>>();
    let shape_n = (shape.len() - 1) as i32;
    let order = [0, shape_n]
        .into_iter()
        .chain(1..shape_n)
        .collect::<Vector<i32>>();
    let mat = mat.reshape_nd(1, shape.as_slice())?;
    let mut mat_t = Mat::default();
    // TODO: how to consume mat_t into a Vec so that Tensor::from_array won't copy?
    transpose_nd(&mat, &order, &mut mat_t)?;
    let shape = mat_t.mat_size();
    let input = (shape.as_slice(), mat_t.data_typed::<f32>()?);
    let tensor = Tensor::from_array(input)?;
    Ok(SessionInputValue::Owned(tensor.into_dyn()))
}
