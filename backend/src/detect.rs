use core::slice::SlicePattern;
use std::{
    collections::HashMap,
    env,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow};
use log::{debug, info};
use opencv::{
    boxed_ref::BoxedRef,
    core::{
        CMP_EQ, CMP_GT, CV_8U, CV_32FC3, CV_32S, Mat, MatExprTraitConst, MatTrait, MatTraitConst,
        MatTraitConstManual, ModifyInplace, Point, Range, Rect, Scalar, Size, ToInputArray, Vec4b,
        Vector, add, add_weighted_def, bitwise_and_def, compare, divide2_def, find_non_zero,
        min_max_loc, no_array, subtract_def, transpose_nd,
    },
    dnn::{
        ModelTrait, TextRecognitionModel, TextRecognitionModelTrait,
        TextRecognitionModelTraitConst, read_net_from_onnx_buffer,
    },
    imgcodecs::{self, IMREAD_COLOR, IMREAD_GRAYSCALE},
    imgproc::{
        CC_STAT_AREA, CC_STAT_HEIGHT, CC_STAT_LEFT, CC_STAT_TOP, CC_STAT_WIDTH,
        CHAIN_APPROX_SIMPLE, COLOR_BGRA2BGR, COLOR_BGRA2GRAY, COLOR_BGRA2RGB, INTER_AREA,
        INTER_CUBIC, MORPH_RECT, RETR_EXTERNAL, THRESH_BINARY, THRESH_OTSU, TM_CCOEFF_NORMED,
        bounding_rect, connected_components_with_stats, cvt_color_def, dilate_def,
        find_contours_def, get_structuring_element_def, match_template_def, min_area_rect, resize,
        threshold,
    },
    traits::OpenCVIntoExternContainer,
};
use ort::{
    session::{Session, SessionInputValue, SessionOutputs},
    value::Tensor,
};
use platforms::windows::keys::KeyKind;

#[cfg(debug_assertions)]
use crate::debug::debug_mat;
#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait Detector {
    fn mat(&self) -> &Mat;

    /// Detects the minimap.
    ///
    /// `confidence_threshold` determines the threshold for the detection to consider a match.
    /// And the `border_threshold` determines the "whiteness" of the minimap's white border.
    fn detect_minimap(&mut self, border_threshold: u8) -> Result<Rect>;

    /// Detects the minimap name from the given `minimap` rectangle.
    ///
    /// `minimap` provides the previously detected minimap region so it can be cropped into.
    /// `score_threshold` determines the threshold for selecting text from the minimap region.
    fn detect_minimap_name(&mut self, minimap: Rect) -> Result<String>;

    /// Detects the rune from the given `minimap` rectangle.
    fn detect_minimap_rune(&mut self, minimap: Rect) -> Result<Rect>;

    /// Detects whether the player in the provided `minimap` rectangle.
    fn detect_player(&mut self, minimap: Rect) -> Result<Rect>;

    /// Detects whether the player is in cash shop.
    fn detect_player_in_cash_shop(&mut self) -> bool;

    /// Detects whether the player has a rune buff.
    fn detect_player_rune_buff(&mut self) -> bool;

    /// Detects whether the player has a x3 exp coupon buff.
    fn detect_player_exp_coupon_x3_buff(&mut self) -> bool;

    /// Detects whether the player has a bonus exp coupon buff.
    fn detect_player_bonus_exp_coupon_buff(&mut self) -> bool;

    /// Detects whether the player has a legion wealth buff.
    fn detect_player_legion_wealth_buff(&mut self) -> bool;

    /// Detects whether the player has a legion luck buff.
    fn detect_player_legion_luck_buff(&mut self) -> bool;

    /// Detects whether the player has a sayram elixir buff.
    fn detect_player_sayram_elixir_buff(&mut self) -> bool;

    /// Detects rune arrows from the given RGBA image `Mat`.
    fn detect_rune_arrows(&mut self) -> Result<[KeyKind; 4]>;

    /// Detects the Erda Shower skill from the given BGRA `Mat` image.
    fn detect_erda_shower(&mut self) -> Result<Rect>;
}

/// A detector temporary caches transformed `Mat`.
///
/// It is useful when there are multiple detections in a single tick that rely on grayscale (e.g. buffs).
#[derive(Debug)]
pub struct CachedDetector<'a> {
    mat: &'a Mat,
    grayscale: Option<Mat>,
}

impl<'a> CachedDetector<'a> {
    pub fn new(mat: &'a Mat) -> CachedDetector<'a> {
        Self {
            mat,
            grayscale: None,
        }
    }

    fn grayscale(&mut self) -> &Mat {
        if self.grayscale.is_none() {
            self.grayscale = Some(to_grayscale(self.mat, true));
        }
        self.grayscale.as_ref().unwrap()
    }
}

impl Detector for CachedDetector<'_> {
    fn mat(&self) -> &Mat {
        self.mat
    }

    fn detect_minimap(&mut self, border_threshold: u8) -> Result<Rect> {
        detect_minimap(self.mat, border_threshold)
    }

    fn detect_minimap_name(&mut self, minimap: Rect) -> Result<String> {
        detect_minimap_name(self.mat, minimap)
    }

    fn detect_minimap_rune(&mut self, minimap: Rect) -> Result<Rect> {
        let minimap_grayscale = self.grayscale().roi(minimap).unwrap();
        detect_minimap_rune(&minimap_grayscale, minimap.tl())
    }

    fn detect_player(&mut self, minimap: Rect) -> Result<Rect> {
        let minimap_grayscale = self.grayscale().roi(minimap).unwrap();
        let result = detect_player(&minimap_grayscale, minimap.tl());
        #[cfg(debug_assertions)]
        {
            if let Ok(bbox) = result {
                debug_mat("Minimap", &minimap_grayscale, 1, &[bbox - minimap.tl()], &[
                    "Player",
                ]);
            }
        }
        result
    }

    fn detect_player_in_cash_shop(&mut self) -> bool {
        detect_cash_shop(self.grayscale())
    }

    fn detect_player_rune_buff(&mut self) -> bool {
        detect_player_rune_buff(&crop_to_buffs_region(self.grayscale()))
    }

    fn detect_player_exp_coupon_x3_buff(&mut self) -> bool {
        detect_player_exp_coupon_x3_buff(&crop_to_buffs_region(self.grayscale()))
    }

    fn detect_player_bonus_exp_coupon_buff(&mut self) -> bool {
        detect_player_bonus_exp_coupon_buff(&crop_to_buffs_region(self.grayscale()))
    }

    fn detect_player_legion_wealth_buff(&mut self) -> bool {
        detect_player_legion_wealth_buff(&to_bgr(&crop_to_buffs_region(self.mat)))
    }

    fn detect_player_legion_luck_buff(&mut self) -> bool {
        detect_player_legion_luck_buff(&to_bgr(&crop_to_buffs_region(self.mat)))
    }

    fn detect_player_sayram_elixir_buff(&mut self) -> bool {
        detect_player_sayram_elixir_buff(&crop_to_buffs_region(self.grayscale()))
    }

    fn detect_rune_arrows(&mut self) -> Result<[KeyKind; 4]> {
        detect_rune_arrows(self.mat)
    }

    fn detect_erda_shower(&mut self) -> Result<Rect> {
        detect_erda_shower(self.grayscale())
    }
}

fn crop_to_buffs_region(mat: &Mat) -> BoxedRef<Mat> {
    let size = mat.size().unwrap();
    // crop to top right of the image for buffs region
    let crop_x = size.width / 3;
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(size.width - crop_x, 0, crop_x, crop_y);
    mat.roi(crop_bbox).unwrap()
}

fn detect_minimap_rune(minimap: &impl ToInputArray, offset: Point) -> Result<Rect> {
    /// TODO: Support default ratio
    static RUNE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(minimap, LazyLock::force(&RUNE), offset, 0.7, Some("rune"))
}

fn detect_cash_shop(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static CASH_SHOP: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("CASH_SHOP_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(
        mat,
        LazyLock::force(&CASH_SHOP),
        Point::default(),
        0.9,
        Some("cash shop"),
    )
    .is_ok()
}

fn detect_player_rune_buff(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static RUNE_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_BUFF_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(
        mat,
        LazyLock::force(&RUNE_BUFF),
        Point::default(),
        0.75,
        Some("rune buff"),
    )
    .is_ok()
}

fn detect_player_exp_coupon_x3_buff(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static EXP_COUPON_X3_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_COUPON_X3_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    detect_template(
        mat,
        LazyLock::force(&EXP_COUPON_X3_BUFF),
        Point::default(),
        0.75,
        Some("exp coupon x3 buff"),
    )
    .is_ok()
}

fn detect_player_bonus_exp_coupon_buff(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static BONUS_EXP_COUPON_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("BONUS_EXP_COUPON_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    detect_template(
        mat,
        LazyLock::force(&BONUS_EXP_COUPON_BUFF),
        Point::default(),
        0.75,
        Some("bonus exp coupon buff"),
    )
    .is_ok()
}

fn detect_player_legion_wealth_buff(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static LEGION_WEALTH_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_WEALTH_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    detect_template(
        mat,
        LazyLock::force(&LEGION_WEALTH_BUFF),
        Point::default(),
        0.75,
        Some("legion wealth buff"),
    )
    .is_ok()
}

fn detect_player_legion_luck_buff(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static LEGION_WEALTH_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_LUCK_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    detect_template(
        mat,
        LazyLock::force(&LEGION_WEALTH_BUFF),
        Point::default(),
        0.75,
        Some("legion luck buff"),
    )
    .is_ok()
}

fn detect_player_sayram_elixir_buff(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static SAYRAM_ELIXIR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("SAYRAM_ELIXIR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    detect_template(
        mat,
        LazyLock::force(&SAYRAM_ELIXIR_BUFF),
        Point::default(),
        0.75,
        Some("sayram elixir buff"),
    )
    .is_ok()
}

fn detect_erda_shower(mat: &Mat) -> Result<Rect> {
    /// TODO: Support default ratio
    static ERDA_SHOWER: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ERDA_SHOWER_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let size = mat.size().unwrap();
    debug!(target: "erda shower", "{size:?}");
    // crop to bottom right of the image for skill bar
    let crop_x = size.width / 2;
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(size.width - crop_x, size.height - crop_y, crop_x, crop_y);
    let skill_bar = mat.roi(crop_bbox).unwrap();
    #[cfg(debug_assertions)]
    {
        debug_mat("Skill bar", &skill_bar, 1, &[], &[]);
    }
    detect_template(
        &skill_bar,
        LazyLock::force(&ERDA_SHOWER),
        crop_bbox.tl(),
        0.96,
        Some("erda shower"),
    )
}

fn detect_player(mat: &impl ToInputArray, offset: Point) -> Result<Rect> {
    const PLAYER_IDEAL_RATIO_THRESHOLD: f64 = 0.8;
    const PLAYER_DEFAULT_RATIO_THRESHOLD: f64 = 0.6;
    static PLAYER_IDEAL_RATIO: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_IDEAL_RATIO_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static PLAYER_DEFAULT_RATIO: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_DEFAULT_RATIO_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static WAS_IDEAL_RATIO: AtomicBool = AtomicBool::new(false);

    let was_ideal_ratio = WAS_IDEAL_RATIO.load(Ordering::Acquire);
    let template = if was_ideal_ratio {
        LazyLock::force(&PLAYER_IDEAL_RATIO)
    } else {
        LazyLock::force(&PLAYER_DEFAULT_RATIO)
    };
    let threshold = if was_ideal_ratio {
        PLAYER_IDEAL_RATIO_THRESHOLD
    } else {
        PLAYER_DEFAULT_RATIO_THRESHOLD
    };
    let result = detect_template(mat, template, offset, threshold, None);
    if result.is_err() {
        WAS_IDEAL_RATIO.store(!was_ideal_ratio, Ordering::Release);
    }
    result
}

/// Detects the `template` from the given BGRA image `Mat`.
#[inline(always)]
fn detect_template(
    mat: &impl ToInputArray,
    template: &Mat,
    offset: Point,
    threshold: f64,
    log: Option<&str>,
) -> Result<Rect> {
    let mut result = Mat::default();
    let mut score = 0f64;
    let mut loc = Point::default();

    match_template_def(mat, template, &mut result, TM_CCOEFF_NORMED).unwrap();
    min_max_loc(
        &result,
        None,
        Some(&mut score),
        None,
        Some(&mut loc),
        &no_array(),
    )
    .unwrap();

    let tl = loc + offset;
    let br = tl + Point::from_size(template.size().unwrap());
    if let Some(target) = log {
        debug!(target: target, "detected with score: {} / {}", score, threshold);
    }
    if score >= threshold {
        Ok(Rect::from_points(tl, br))
    } else {
        Err(anyhow!("template not found"))
    }
}

fn detect_rune_arrows(mat: &Mat) -> Result<[KeyKind; 4]> {
    static RUNE_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("RUNE_MODEL"))))
            .expect("unable to build rune detection session")
    });

    fn map_arrow(pred: &[f32]) -> KeyKind {
        match pred[5] as i32 {
            0 => KeyKind::Up,
            1 => KeyKind::Down,
            2 => KeyKind::Left,
            3 => KeyKind::Right,
            _ => unreachable!(),
        }
    }

    let (mat_in, _, _) = preprocess_for_yolo(mat);
    let result = RUNE_MODEL.run([norm_rgb_to_input_value(&mat_in)]).unwrap();
    let mat_out = from_output_value(&result);
    let mut preds = (0..mat_out.rows())
        // SAFETY: 0..outputs.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|&pred| {
            // pred has shapes [bbox(4) + conf + class]
            pred[4] >= 0.8
        })
        .collect::<Vec<_>>();
    if preds.len() != 4 {
        info!(target: "player", "failed to detect rune arrows {preds:?}");
        return Err(anyhow!("failed to detect rune arrows"));
    }
    // sort by x for arrow order
    preds.sort_by(|&a, &b| a[0].total_cmp(&b[0]));

    let first = map_arrow(preds[0]);
    let second = map_arrow(preds[1]);
    let third = map_arrow(preds[2]);
    let fourth = map_arrow(preds[3]);
    info!(
        target: "player",
        "solving rune result {first:?} ({}), {second:?} ({}), {third:?} ({}), {fourth:?} ({})",
        preds[0][4],
        preds[1][4],
        preds[2][4],
        preds[3][4]
    );
    Ok([first, second, third, fourth])
}

fn detect_minimap(mat: &Mat, border_threshold: u8) -> Result<Rect> {
    static MINIMAP_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("MINIMAP_MODEL"))))
            .expect("unable to build minimap detection session")
    });
    // expands out a few pixels to include the whole white border for thresholding
    // after yolo detection
    fn expand_bbox(bbox: &Rect) -> Rect {
        let count = (bbox.width.max(bbox.height) as f32 * 0.008).ceil() as i32;
        debug!(target: "minimap", "expand border by {count}");
        let x = (bbox.x - count).max(0);
        let y = (bbox.y - count).max(0);
        let x_size = (bbox.x - x) * 2;
        let y_size = (bbox.y - y) * 2;
        Rect::new(x, y, bbox.width + x_size, bbox.height + y_size)
    }

    let size = mat.size().unwrap();
    let (preprocessed, w_ratio, h_ratio) = preprocess_for_yolo(mat);
    let result = MINIMAP_MODEL
        .run([norm_rgb_to_input_value(&preprocessed)])
        .unwrap();
    let result = from_output_value(&result);
    let pred = (0..result.rows())
        // SAFETY: 0..result.rows() is within Mat bounds
        .map(|i| unsafe { result.at_row_unchecked::<f32>(i).unwrap() })
        .max_by(|&a, &b| {
            // a and b have shapes [bbox(4) + class(1)]
            a[4].total_cmp(&b[4])
        });
    let bbox = pred.and_then(|pred| {
        debug!(target: "minimap", "yolo detection: {pred:?}");
        if pred[4] < 0.5 {
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
    let minimap = bbox.map(|bbox| {
        let bbox = expand_bbox(&bbox);
        let mut minimap = to_grayscale(&mat.roi(bbox).unwrap(), false);
        unsafe {
            // SAFETY: threshold can be called in place.
            minimap.modify_inplace(|mat, mat_mut| {
                threshold(mat, mat_mut, 0.0, 255.0, THRESH_OTSU).unwrap()
            });
        }
        minimap
    });
    // get only the outer contours
    let contours = minimap.map(|mat| {
        let mut vec = Vector::<Vector<Point>>::new();
        find_contours_def(&mat, &mut vec, RETR_EXTERNAL, CHAIN_APPROX_SIMPLE).unwrap();
        vec
    });
    // pick the contour with maximum area
    let contour = contours.and_then(|vec| {
        debug!(target: "minimap", "contours detection: {vec:?}");
        vec.into_iter()
            .map(|contour| bounding_rect(&contour).unwrap())
            .max_by(|a, b| a.area().cmp(&b.area()))
    });
    let contour = contour.and_then(|contour| {
        let bbox = expand_bbox(&bbox.unwrap());
        let contour = Rect::from_points(contour.tl() + bbox.tl(), contour.br() + bbox.tl());
        debug!(
            target: "minimap",
            "yolo bbox and contour bbox areas: {:?} {:?}",
            bbox.area(),
            contour.area()
        );
        // the detected contour should be contained inside the detected yolo minimap when expanded
        // 1500 is a fixed value for ensuring the contour is tight to the minimap white border
        if (bbox & contour) == contour && (bbox.area() - contour.area()) >= 1500 {
            Some(contour)
        } else {
            None
        }
    });
    // crop the white border
    let crop = contour.and_then(|bound| {
        // offset in by 10% to avoid the round border
        // and use top border as basis
        let range = (bound.width as f32 * 0.1) as i32;
        let start = bound.x + range;
        let end = bound.x + bound.width - range + 1;
        let mut counts = HashMap::<i32, i32>::new();
        for col in start..end {
            let mut count = 0;
            for row in bound.y..(bound.y + bound.height) {
                if mat
                    .at_2d::<Vec4b>(row, col)
                    .unwrap()
                    .iter()
                    .all(|v| *v >= border_threshold)
                {
                    count += 1;
                } else {
                    break;
                }
            }
            counts.entry(count).and_modify(|c| *c += 1).or_insert(1);
        }
        debug!(target: "minimap", "border pixel count {:?}", counts);
        counts.into_iter().max_by(|a, b| a.1.cmp(&b.1)).map(|e| e.0)
    });
    crop.map(|count| {
        let contour = contour.unwrap();
        Rect::new(
            contour.x + count,
            contour.y + count,
            contour.width - count * 2,
            contour.height - count * 2,
        )
    })
    .ok_or(anyhow!("minimap not found"))
}

fn detect_minimap_name(mat: &Mat, minimap: Rect) -> Result<String> {
    const TEXT_SCORE_THRESHOLD: f64 = 0.7;
    const LINK_SCORE_THRESHOLD: f64 = 0.4;
    static TEXT_RECOGNITION_MODEL: LazyLock<Mutex<TextRecognitionModel>> = LazyLock::new(|| {
        let model = read_net_from_onnx_buffer(&Vector::from_slice(include_bytes!(env!(
            "TEXT_RECOGNITION_MODEL"
        ))))
        .unwrap();
        Mutex::new(
            TextRecognitionModel::new(&model)
                .and_then(|mut m| {
                    m.set_input_params(
                        1.0 / 127.5,
                        Size::new(100, 32),
                        Scalar::new(127.5, 127.5, 127.5, 0.0),
                        false,
                        false,
                    )?;
                    m.set_decode_type("CTC-greedy")?.set_vocabulary(
                        &include_str!(env!("TEXT_RECOGNITION_ALPHABET"))
                            .lines()
                            .collect::<Vector<String>>(),
                    )
                })
                .expect("unable to build text recognition model"),
        )
    });
    static TEXT_DETECTION_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("TEXT_DETECTION_MODEL"))))
            .expect("unable to build minimap name detection session")
    });

    // this function is adapted from
    // https://github.com/clovaai/CRAFT-pytorch/blob/e332dd8b718e291f51b66ff8f9ef2c98ee4474c8/craft_utils.py#L19
    // with minor changes
    fn extract_bboxes(
        mat: &BoxedRef<Mat>,
        w_ratio: f32,
        h_ratio: f32,
        x_offset: i32,
        y_offset: i32,
    ) -> Vec<Rect> {
        let text_score = mat
            .ranges(&Vector::from_iter([
                Range::all().unwrap(),
                Range::all().unwrap(),
                Range::new(0, 1).unwrap(),
            ]))
            .unwrap()
            .clone_pointee();
        // remove last channel (not sure what other way to do it without clone_pointee first)
        let text_score = text_score
            .reshape_nd(1, &text_score.mat_size().as_slice()[..2])
            .unwrap();

        let mut text_low_score = Mat::default();
        threshold(
            &text_score,
            &mut text_low_score,
            LINK_SCORE_THRESHOLD,
            1.0,
            0,
        )
        .unwrap();

        let mut link_score = mat
            .ranges(&Vector::from_iter([
                Range::all().unwrap(),
                Range::all().unwrap(),
                Range::new(1, 2).unwrap(),
            ]))
            .unwrap()
            .clone_pointee();
        // remove last channel (not sure what other way to do it without clone_pointee first)
        let mut link_score = link_score
            .reshape_nd_mut(1, &link_score.mat_size().as_slice()[..2])
            .unwrap();
        // SAFETY: can be modified in place
        unsafe {
            link_score.modify_inplace(|mat, mat_mut| {
                threshold(mat, mat_mut, LINK_SCORE_THRESHOLD, 1.0, 0).unwrap();
            });
        }

        let mut combined_score = Mat::default();
        let mut gt_one_mask = Mat::default();
        add(
            &text_low_score,
            &link_score,
            &mut combined_score,
            &no_array(),
            CV_8U,
        )
        .unwrap();
        compare(&combined_score, &Scalar::all(1.0), &mut gt_one_mask, CMP_GT).unwrap();
        combined_score
            .set_to(&Scalar::all(1.0), &gt_one_mask)
            .unwrap();

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
        )
        .unwrap();
        for i in 1..labels_count {
            let area = *stats.at_2d::<i32>(i, CC_STAT_AREA).unwrap();
            if area < 210 {
                // skip too small single character (number)
                // and later re-detect afterward
                continue;
            }

            let mut mask = Mat::default();
            let mut max_score = 0.0f64;
            compare(&labels, &Scalar::all(i as f64), &mut mask, CMP_EQ).unwrap();
            min_max_loc(&text_score, None, Some(&mut max_score), None, None, &mask).unwrap();
            if max_score < TEXT_SCORE_THRESHOLD {
                continue;
            }

            let shape = mask.size().unwrap();
            // SAFETY: The position (row, col) is guaranteed by OpenCV
            let x = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_LEFT).unwrap() };
            let y = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_TOP).unwrap() };
            let w = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_WIDTH).unwrap() };
            let h = unsafe { *stats.at_2d_unchecked::<i32>(i, CC_STAT_HEIGHT).unwrap() };
            let size = area as f64 * w.min(h) as f64 / (w as f64 * h as f64);
            let size = ((size).sqrt() * 2.0) as i32;
            let sx = (x - size + 1).max(0);
            let sy = (y - size + 1).max(0);
            let ex = (x + w + size + 1).min(shape.width);
            let ey = (y + h + size + 1).min(shape.height);
            let kernel_pad = if area < 250 { 6 } else { 4 };
            let kernel = get_structuring_element_def(
                MORPH_RECT,
                Size::new(size + kernel_pad, size + kernel_pad),
            )
            .unwrap();

            let mut link_mask = Mat::default();
            let mut text_mask = Mat::default();
            let mut and_mask = Mat::default();
            let mut seg_map = Mat::zeros(shape.height, shape.width, CV_8U)
                .unwrap()
                .to_mat()
                .unwrap();
            compare(&link_score, &Scalar::all(1.0), &mut link_mask, CMP_EQ).unwrap();
            compare(&text_score, &Scalar::all(0.0), &mut text_mask, CMP_EQ).unwrap();
            bitwise_and_def(&link_mask, &text_mask, &mut and_mask).unwrap();
            seg_map.set_to(&Scalar::all(255.0), &mask).unwrap();
            seg_map.set_to(&Scalar::all(0.0), &and_mask).unwrap();

            let mut seg_contours = Vector::<Point>::new();
            let mut seg_roi = seg_map
                .roi_mut(Rect::from_points(Point::new(sx, sy), Point::new(ex, ey)))
                .unwrap();
            // SAFETY: all of the functions below can be called in place.
            unsafe {
                seg_roi.modify_inplace(|mat, mat_mut| {
                    dilate_def(mat, mat_mut, &kernel).unwrap();
                    mat.copy_to(mat_mut).unwrap();
                });
            }
            find_non_zero(&seg_map, &mut seg_contours).unwrap();

            let contour = min_area_rect(&seg_contours)
                .unwrap()
                .bounding_rect2f()
                .unwrap();
            let tl = contour.tl();
            let tl = Point::new(
                (tl.x * w_ratio * 2.0) as i32 + x_offset,
                (tl.y * h_ratio * 2.0) as i32 + y_offset,
            );
            let br = contour.br();
            let br = Point::new(
                (br.x * w_ratio * 2.0) as i32 + x_offset,
                (br.y * h_ratio * 2.0) as i32 + y_offset,
            );
            bboxes.push(Rect::from_points(tl, br));
        }
        bboxes
    }

    let (mat_in, w_ratio, h_ratio, x_offset, y_offset) = preprocess_for_minimap_name(mat, minimap);
    let result = TEXT_DETECTION_MODEL
        .run([norm_rgb_to_input_value(&mat_in)])
        .unwrap();
    let mat_out = from_output_value(&result);
    let bboxes = extract_bboxes(&mat_out, w_ratio, h_ratio, x_offset, y_offset);
    // find the text boxes with y
    // closes to the minimap
    let mut bbox_match_y = None::<i32>;
    let mut bbox_min_y_diff = i32::MAX;
    for bbox in &bboxes {
        let y = bbox.y + bbox.height;
        let diff = minimap.y - y;
        if diff > 8 && diff < bbox_min_y_diff {
            bbox_match_y = Some(y);
            bbox_min_y_diff = diff;
        }
    }
    let bbox_match_y = bbox_match_y.ok_or(anyhow!("minimap name not found"))?;
    let bbox_match_x = minimap.x + minimap.width;
    let mut bboxes = bboxes
        .into_iter()
        .filter(|bbox| {
            let diff = bbox_match_y - (bbox.y + bbox.height);
            diff <= 5 && (bbox.x + bbox.width) <= bbox_match_x
        })
        .collect::<Vec<Rect>>();
    bboxes.sort_by(|a, b| a.x.cmp(&b.x));

    // the model doesn't detect well on a single character level
    // but it is crucial to be able to the detect the last character (a single number)
    // as it helps distinguish between different map variations
    // if the model is able to detect the digit, the number_bbox
    // should contain all black pixels
    let number_bbox = bboxes
        .last()
        .map(|bbox| {
            let x = bbox.x + bbox.width;
            let y = bbox.y;
            let w = (minimap.x + minimap.width) - x;
            let h = bbox.height;
            Rect::new(x, y, w, h)
        })
        .and_then(|bbox| {
            let mut number = to_grayscale(&mat.roi(bbox).unwrap(), true);
            unsafe {
                // SAFETY: threshold can be called in place.
                number.modify_inplace(|mat, mat_mut| {
                    let kernel = get_structuring_element_def(MORPH_RECT, Size::new(5, 5)).unwrap();
                    threshold(mat, mat_mut, 180.0, 255.0, THRESH_BINARY).unwrap();
                    dilate_def(mat, mat_mut, &kernel).unwrap();
                });
            }
            bounding_rect(&number)
                .ok()
                .take_if(|bbox| bbox.area() > 0)
                .map(|number| number + bbox.tl())
        });
    if let Some(bbox) = number_bbox {
        debug!(target: "minimap", "detected trailing number identifier {bbox:?}");
        bboxes.push(bbox);
    }

    let recognizier = TEXT_RECOGNITION_MODEL.lock().unwrap();
    let name = bboxes
        .into_iter()
        .filter_map(|word| {
            let mut mat = mat.roi(word).unwrap().clone_pointee();
            unsafe {
                mat.modify_inplace(|mat, mat_mut| {
                    cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
                });
            }
            recognizier.recognize(&mat).ok()
        })
        .reduce(|a, b| a + &b);
    debug!(target: "minimap", "name detection result {name:?}");
    name.ok_or(anyhow!("minimap name not found"))
}

/// Preprocesses a BGRA `Mat` image to a normalized and resized RGB `Mat` image with type `f32` for YOLO detection.
///
/// Returns a triplet of `(Mat, width_ratio, height_ratio)` with the ratios calculed from
/// `old_size / new_size`.
#[inline(always)]
fn preprocess_for_yolo(mat: &Mat) -> (Mat, f32, f32) {
    let mut mat = mat.clone();
    let (w_ratio, h_ratio) = resize_w_h_ratio(mat.size().unwrap(), 640.0, 640.0);
    // SAFETY: all of the functions below can be called in place.
    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize(mat, mat_mut, Size::new(640, 640), 0.0, 0.0, INTER_AREA).unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0 / 255.0, 0.0).unwrap();
        });
    }
    (mat, w_ratio, h_ratio)
}

/// Preprocesses a BGRA `Mat` image to a normalized and resized RGB `Mat` image with type `f32` for minimap name detection.
///
/// The preprocess is adapted from: https://github.com/clovaai/CRAFT-pytorch/blob/master/imgproc.py.
///
/// Returns a `(Mat, width_ratio, height_ratio, x_offset, y_offset)`.
#[inline(always)]
fn preprocess_for_minimap_name(mat: &Mat, minimap: Rect) -> (Mat, f32, f32, i32, i32) {
    let x_offset = minimap.x;
    let y_offset = (minimap.y - minimap.height).max(0);
    let bbox = Rect::from_points(
        Point::new(x_offset, y_offset),
        Point::new(minimap.x + minimap.width, minimap.y),
    );
    let mut mat = mat.roi(bbox).unwrap().clone_pointee();
    let size = mat.size().unwrap();
    let size_w = size.width as f32;
    let size_h = size.height as f32;
    let size_max = size_w.max(size_h);
    let resize_size = 5.0 * size_max;
    let resize_ratio = resize_size / size_max;

    let resize_w = (resize_ratio * size_w) as i32;
    let resize_w = (resize_w + 31) & !31; // rounds to multiple of 32
    let resize_w_ratio = size_w / resize_w as f32;

    let resize_h = (resize_ratio * size_h) as i32;
    let resize_h = (resize_h + 31) & !31;
    let resize_h_ratio = size_h / resize_h as f32;
    // SAFETY: all of the below functions can be called in place
    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize(
                mat,
                mat_mut,
                Size::new(resize_w, resize_h),
                0.0,
                0.0,
                INTER_CUBIC,
            )
            .unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0, 0.0).unwrap();
            // these values are pre-multiplied from the above link in normalizeMeanVariance
            subtract_def(mat, &Scalar::new(123.675, 116.28, 103.53, 0.0), mat_mut).unwrap();
            divide2_def(&mat, &Scalar::new(58.395, 57.12, 57.375, 1.0), mat_mut).unwrap();
        });
    }
    (mat, resize_w_ratio, resize_h_ratio, x_offset, y_offset)
}

/// Retrieves `(width, height)` ratios for resizing.
#[inline(always)]
fn resize_w_h_ratio(from: Size, to_w: f32, to_h: f32) -> (f32, f32) {
    (from.width as f32 / to_w, from.height as f32 / to_h)
}

/// Converts an BGRA `Mat` image to BGR.
#[inline(always)]
fn to_bgr(mat: &impl MatTraitConst) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: can be modified inplace
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2BGR).unwrap();
        });
    }
    mat
}

/// Converts an BGRA `Mat` image to grayscale.
///
/// `add_contrast` can be set to `true` in order to increase contrast by a fixed amount
/// used for template matching.
#[inline(always)]
fn to_grayscale(mat: &impl MatTraitConst, add_contrast: bool) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: all of the functions below can be called in place.
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2GRAY).unwrap();
            if add_contrast {
                // TODO: is this needed?
                add_weighted_def(mat, 1.5, mat, 0.0, -80.0, mat_mut).unwrap();
            }
        });
    }
    mat
}

/// Extracts a borrowed `Mat` from `SessionOutputs`.
///
/// The returned `BoxedRef<'_, Mat>` has shape `[..dims]` with batch size (1) removed.
#[inline(always)]
fn from_output_value<'a>(result: &SessionOutputs) -> BoxedRef<'a, Mat> {
    let (dims, outputs) = result["output0"].try_extract_raw_tensor::<f32>().unwrap();
    let dims = dims.iter().map(|&dim| dim as i32).collect::<Vec<i32>>();
    let mat = Mat::new_nd_with_data(dims.as_slice(), outputs).unwrap();
    let mat = mat.reshape_nd(1, &dims.as_slice()[1..]).unwrap();
    let mat = mat.opencv_into_extern_container_nofail();
    BoxedRef::from(mat)
}

/// Converts a continuous, normalized `f32` RGB `Mat` image to `SessionInputValue`.
///
/// The input `Mat` is assumed to be continuous, normalized RGB `f32` data type and will panic if not.
/// The `Mat` is reshaped to single channel, tranposed to `[1, 3, H, W]` and converted to `SessionInputValue`.
#[inline(always)]
fn norm_rgb_to_input_value(mat: &Mat) -> SessionInputValue {
    let mat = mat.reshape_nd(1, &[1, mat.rows(), mat.cols(), 3]).unwrap();
    let mut mat_t = Mat::default();
    transpose_nd(&mat, &Vector::from_slice(&[0, 3, 1, 2]), &mut mat_t).unwrap();
    let shape = mat_t.mat_size();
    let input = (shape.as_slice(), mat_t.data_typed::<f32>().unwrap());
    let tensor = Tensor::from_array(input).unwrap();
    SessionInputValue::Owned(tensor.into_dyn())
}
