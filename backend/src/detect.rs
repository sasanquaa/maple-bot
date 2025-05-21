use core::slice::SlicePattern;
use std::{
    collections::HashMap,
    env,
    fmt::Debug,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::{Result, anyhow, bail};
use dyn_clone::DynClone;
use log::{debug, error, info};
#[cfg(test)]
use mockall::mock;
use opencv::{
    boxed_ref::BoxedRef,
    core::{
        BORDER_CONSTANT, CMP_EQ, CMP_GT, CV_8U, CV_32FC3, CV_32S, Mat, MatExprTraitConst, MatTrait,
        MatTraitConst, MatTraitConstManual, ModifyInplace, Point, Point2f, Range, Rect, Scalar,
        Size, ToInputArray, Vec3b, Vec4b, Vector, add, add_weighted_def, bitwise_and_def, compare,
        copy_make_border, divide2_def, extract_channel, find_non_zero, min_max_loc, no_array,
        subtract_def, transpose_nd,
    },
    dnn::{
        ModelTrait, TextRecognitionModel, TextRecognitionModelTrait,
        TextRecognitionModelTraitConst, read_net_from_onnx_buffer,
    },
    imgcodecs::{self, IMREAD_COLOR, IMREAD_GRAYSCALE},
    imgproc::{
        CC_STAT_AREA, CC_STAT_HEIGHT, CC_STAT_LEFT, CC_STAT_TOP, CC_STAT_WIDTH,
        CHAIN_APPROX_SIMPLE, COLOR_BGR2HSV_FULL, COLOR_BGRA2BGR, COLOR_BGRA2GRAY, COLOR_BGRA2RGB,
        INTER_CUBIC, INTER_LINEAR, MORPH_RECT, RETR_EXTERNAL, THRESH_BINARY, TM_CCOEFF_NORMED,
        bounding_rect, connected_components_with_stats, cvt_color_def, dilate_def,
        find_contours_def, get_structuring_element_def, match_template, min_area_rect, resize,
        threshold,
    },
};
use ort::{
    session::{Session, SessionInputValue, SessionOutputs},
    value::Tensor,
};
use platforms::windows::KeyKind;

#[cfg(debug_assertions)]
use crate::debug::{debug_mat, debug_spinning_arrows};
use crate::{array::Array, buff::BuffKind, mat::OwnedMat};

const MAX_ARROWS: usize = 4;
const MAX_SPIN_ARROWS: usize = 2; // PRAY

/// Struct for storing information about the spinning arrows
#[derive(Debug, Copy, Clone)]
struct SpinArrow {
    /// The centroid of the spinning arrow relative to the whole image
    centroid: Point,
    /// The region of the spinning arrow relative to the whole image
    region: Rect,
    /// The last arrow head relative to the centroid
    last_arrow_head: Option<Point>,
    final_arrow: Option<KeyKind>,
    #[cfg(debug_assertions)]
    is_spin_testing: bool,
}

/// The current arrows detection/calibration state
#[derive(Debug)]
pub enum ArrowsState {
    Calibrating(ArrowsCalibrating),
    Complete([KeyKind; MAX_ARROWS]),
}

/// Struct representing arrows calibration in-progress
#[derive(Debug, Copy, Clone, Default)]
pub struct ArrowsCalibrating {
    spin_arrows: Option<Array<SpinArrow, MAX_SPIN_ARROWS>>,
    spin_arrows_calibrated: bool,
    rune_region: Option<Rect>,
    normal_arrows: Option<Array<(Rect, KeyKind), MAX_ARROWS>>,
    #[cfg(debug_assertions)]
    is_spin_testing: bool,
}

impl ArrowsCalibrating {
    #[inline]
    pub fn has_spin_arrows(&self) -> bool {
        self.spin_arrows_calibrated && self.spin_arrows.is_some()
    }

    #[cfg(debug_assertions)]
    pub fn enable_spin_test(&mut self) {
        self.is_spin_testing = true;
    }
}

#[derive(Clone, Copy, Debug)]
pub enum OtherPlayerKind {
    Guildie,
    Stranger,
    Friend,
}

pub trait Detector: 'static + Send + DynClone + Debug {
    fn mat(&self) -> &OwnedMat;

    /// Detects a list of mobs.
    ///
    /// Returns a list of mobs coordinate relative to minimap coordinate.
    fn detect_mobs(&self, minimap: Rect, bound: Rect, player: Point) -> Result<Vec<Point>>;

    /// Detects whether to press ESC for unstucking.
    fn detect_esc_settings(&self) -> bool;

    /// Detects whether there is an elite boss bar.
    fn detect_elite_boss_bar(&self) -> bool;

    /// Detects the minimap.
    ///
    /// The `border_threshold` determines the "whiteness" (grayscale value from 0..255) of
    /// the minimap's white border.
    fn detect_minimap(&self, border_threshold: u8) -> Result<Rect>;

    /// Detects the portals from the given `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_minimap_portals(&self, minimap: Rect) -> Result<Vec<Rect>>;

    /// Detects the rune from the given `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect>;

    /// Detects the player in the provided `minimap` rectangle.
    ///
    /// Returns `Rect` relative to `minimap` coordinate.
    fn detect_player(&self, minimap: Rect) -> Result<Rect>;

    /// Detects whether a player of `kind` is in the minimap.
    fn detect_player_kind(&self, minimap: Rect, kind: OtherPlayerKind) -> bool;

    /// Detects whether the player is dead.
    fn detect_player_is_dead(&self) -> bool;

    /// Detects whether the player is in cash shop.
    fn detect_player_in_cash_shop(&self) -> bool;

    /// Detects the player health bar.
    fn detect_player_health_bar(&self) -> Result<Rect>;

    /// Detects the player current and max health bars.
    fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)>;

    /// Detects the player current health and max health.
    fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)>;

    /// Detects whether the player has a buff specified by `kind`.
    fn detect_player_buff(&self, kind: BuffKind) -> bool;

    /// Detects arrows from the given RGBA `Mat` image.
    ///
    /// `calibrating` represents the previous calibrating state returned by
    /// [`ArrowsState::Calibrating`]
    fn detect_rune_arrows(&self, calibrating: ArrowsCalibrating) -> Result<ArrowsState>;

    /// Detects the Erda Shower skill from the given BGRA `Mat` image.
    fn detect_erda_shower(&self) -> Result<Rect>;
}

#[cfg(test)]
mock! {
    pub Detector {}

    impl Detector for Detector {
        fn mat(&self) -> &OwnedMat;
        fn detect_mobs(&self, minimap: Rect, bound: Rect, player: Point) -> Result<Vec<Point>>;
        fn detect_esc_settings(&self) -> bool;
        fn detect_elite_boss_bar(&self) -> bool;
        fn detect_minimap(&self, border_threshold: u8) -> Result<Rect>;
        fn detect_minimap_portals(&self, minimap: Rect) -> Result<Vec<Rect>>;
        fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect>;
        fn detect_player(&self, minimap: Rect) -> Result<Rect>;
        fn detect_player_kind(&self, minimap: Rect, kind: OtherPlayerKind) -> bool;
        fn detect_player_is_dead(&self) -> bool;
        fn detect_player_in_cash_shop(&self) -> bool;
        fn detect_player_health_bar(&self) -> Result<Rect>;
        fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)>;
        fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)>;
        fn detect_player_buff(&self, kind: BuffKind) -> bool;
        fn detect_rune_arrows<'a>(
            &self,
            calibrating: ArrowsCalibrating,
        ) -> Result<ArrowsState>;
        fn detect_erda_shower(&self) -> Result<Rect>;
    }

    impl Debug for Detector {
        fn fmt<'a, 'b, 'c>(&'a self, f: &'b mut std::fmt::Formatter<'c> ) -> std::fmt::Result;
    }

    impl Clone for Detector {
        fn clone(&self) -> Self;
    }
}

type MatFn = Box<dyn FnOnce() -> Mat + Send>;

/// A detector that temporary caches the transformed `Mat`.
///
/// It is useful when there are multiple detections in a single tick that
/// rely on grayscale (e.g. buffs).
///
/// TODO: Is it really useful?
#[derive(Clone, Debug)]
pub struct CachedDetector {
    mat: Arc<OwnedMat>,
    grayscale: Arc<LazyLock<Mat, MatFn>>,
    buffs_grayscale: Arc<LazyLock<Mat, MatFn>>,
}

impl CachedDetector {
    pub fn new(mat: OwnedMat) -> CachedDetector {
        let mat = Arc::new(mat);
        let grayscale = mat.clone();
        let grayscale = Arc::new(LazyLock::<Mat, MatFn>::new(Box::new(move || {
            to_grayscale(&*grayscale, true)
        })));
        let buffs_grayscale = grayscale.clone();
        let buffs_grayscale = Arc::new(LazyLock::<Mat, MatFn>::new(Box::new(move || {
            crop_to_buffs_region(&**buffs_grayscale).clone_pointee()
        })));
        Self {
            mat,
            grayscale,
            buffs_grayscale,
        }
    }
}

impl Detector for CachedDetector {
    fn mat(&self) -> &OwnedMat {
        &self.mat
    }

    fn detect_mobs(&self, minimap: Rect, bound: Rect, player: Point) -> Result<Vec<Point>> {
        detect_mobs(&*self.mat, minimap, bound, player)
    }

    fn detect_esc_settings(&self) -> bool {
        detect_esc_settings(&**self.grayscale)
    }

    fn detect_elite_boss_bar(&self) -> bool {
        detect_elite_boss_bar(&**self.grayscale)
    }

    fn detect_minimap(&self, border_threshold: u8) -> Result<Rect> {
        detect_minimap(&*self.mat, border_threshold)
    }

    fn detect_minimap_portals(&self, minimap: Rect) -> Result<Vec<Rect>> {
        let minimap_color = to_bgr(&self.mat.roi(minimap)?);
        detect_minimap_portals(minimap_color)
    }

    fn detect_minimap_rune(&self, minimap: Rect) -> Result<Rect> {
        let minimap_color = to_bgr(&self.mat.roi(minimap)?);
        detect_minimap_rune(&minimap_color)
    }

    fn detect_player(&self, minimap: Rect) -> Result<Rect> {
        let minimap_color = to_bgr(&self.mat.roi(minimap)?);
        detect_player(&minimap_color)
    }

    fn detect_player_kind(&self, minimap: Rect, kind: OtherPlayerKind) -> bool {
        let minimap_color = to_bgr(&self.mat.roi(minimap).unwrap());
        detect_player_kind(&minimap_color, kind)
    }

    fn detect_player_is_dead(&self) -> bool {
        detect_player_is_dead(&**self.grayscale)
    }

    fn detect_player_in_cash_shop(&self) -> bool {
        detect_player_in_cash_shop(&**self.grayscale)
    }

    fn detect_player_health_bar(&self) -> Result<Rect> {
        detect_player_health_bar(&**self.grayscale)
    }

    fn detect_player_current_max_health_bars(&self, health_bar: Rect) -> Result<(Rect, Rect)> {
        detect_player_current_max_health_bars(&*self.mat, &**self.grayscale, health_bar)
    }

    fn detect_player_health(&self, current_bar: Rect, max_bar: Rect) -> Result<(u32, u32)> {
        detect_player_health(&*self.mat, current_bar, max_bar)
    }

    fn detect_player_buff(&self, kind: BuffKind) -> bool {
        let mat = match kind {
            BuffKind::Rune
            | BuffKind::SayramElixir
            | BuffKind::AureliaElixir
            | BuffKind::ExpCouponX3
            | BuffKind::BonusExpCoupon => &**self.buffs_grayscale,
            BuffKind::LegionWealth
            | BuffKind::LegionLuck
            | BuffKind::WealthAcquisitionPotion
            | BuffKind::ExpAccumulationPotion
            | BuffKind::ExtremeRedPotion
            | BuffKind::ExtremeBluePotion
            | BuffKind::ExtremeGreenPotion
            | BuffKind::ExtremeGoldPotion => &to_bgr(&crop_to_buffs_region(&*self.mat)),
        };
        detect_player_buff(mat, kind)
    }

    fn detect_rune_arrows(&self, calibrating: ArrowsCalibrating) -> Result<ArrowsState> {
        detect_rune_arrows(&*self.mat, calibrating)
    }

    fn detect_erda_shower(&self) -> Result<Rect> {
        detect_erda_shower(&**self.grayscale)
    }
}

fn crop_to_buffs_region(mat: &impl MatTraitConst) -> BoxedRef<Mat> {
    let size = mat.size().unwrap();
    // crop to top right of the image for buffs region
    let crop_x = size.width / 3;
    let crop_y = size.height / 4;
    let crop_bbox = Rect::new(size.width - crop_x, 0, crop_x, crop_y);
    mat.roi(crop_bbox).unwrap()
}

fn detect_mobs(
    mat: &impl MatTraitConst,
    minimap: Rect,
    bound: Rect,
    player: Point,
) -> Result<Vec<Point>> {
    static MOB_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("MOB_MODEL"))))
            .expect("unable to build mob detection session")
    });

    /// Approximates the mob coordinate on screen to mob coordinate on minimap.
    ///
    /// This function tries to approximate the delta (dx, dy) that the player needs to move
    /// in relative to the minimap coordinate in order to reach the mob. Returns the mob
    /// coordinate on the minimap by adding the delta to the player position.
    ///
    /// Note: It is not that accurate but that is that and this is this. Hey it seems better than
    /// the previous alchemy.
    #[inline]
    fn to_minimap_coordinate(
        mob_bbox: Rect,
        minimap_bbox: Rect,
        mobbing_bound: Rect,
        player: Point,
        mat_size: Size,
    ) -> Option<Point> {
        // These numbers are for scaling dx/dy on the screen to dx/dy on the minimap.
        // They are approximated in 1280x720 resolution by going from one point to another point
        // from the middle of the screen with both points visible on screen before traveling. Take
        // the distance traveled on the minimap and divide it by half of the resolution
        // (e.g. tralveled minimap x / 640). Whether it is correct or not, time will tell.
        const X_SCALE: f32 = 0.059_375;
        const Y_SCALE: f32 = 0.036_111;

        // The main idea is to calculate the offset of the detected mob from the middle of screen
        // and use that distance as dx/dy to move the player. This assumes the player will
        // most of the time be near or very close to the middle of the screen. This is already
        // not accurate in the sense that the camera will have a bit of lag before
        // it is centered again on the player. And when the player is near edges of the map,
        // this function is just plain wrong. For better accuracy, detecting where the player is
        // on the screen and use that as the basis is required.
        let x_screen_mid = mat_size.width / 2;
        let x_mob_mid = mob_bbox.x + mob_bbox.width / 2;
        let x_screen_delta = x_screen_mid - x_mob_mid;
        let x_minimap_delta = (x_screen_delta as f32 * X_SCALE) as i32;

        // For dy, if the whole mob bounding box is above the screen mid point, then the
        // box top edge is used to increase the dy distance as to help the player move up. The same
        // goes for moving down. If the bounding box overlaps with the screen mid point, the box
        // mid point is used as to to help the player stay in place.
        let y_screen_mid = mat_size.height / 2;
        let y_mob = if mob_bbox.y + mob_bbox.height < y_screen_mid {
            mob_bbox.y
        } else if mob_bbox.y > y_screen_mid {
            mob_bbox.y + mob_bbox.height
        } else {
            mob_bbox.y + mob_bbox.height / 2
        };
        let y_screen_delta = y_screen_mid - y_mob;
        let y_minimap_delta = (y_screen_delta as f32 * Y_SCALE) as i32;

        let point_x = if x_minimap_delta > 0 {
            (player.x - x_minimap_delta).max(0)
        } else {
            (player.x - x_minimap_delta).min(minimap_bbox.width)
        };
        let point_y = (player.y + y_minimap_delta).max(0).min(minimap_bbox.height);
        // Minus the y by minimap height to make it relative to the minimap top edge
        let point = Point::new(point_x, minimap_bbox.height - point_y);
        if point.x < mobbing_bound.x
            || point.x > mobbing_bound.x + mobbing_bound.width
            || point.y < mobbing_bound.y
            || point.y > mobbing_bound.y + mobbing_bound.height
        {
            None
        } else {
            Some(point)
        }
    }

    let size = mat.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(mat);
    let result = MOB_MODEL.run([norm_rgb_to_input_value(&mat_in)]).unwrap();
    let result = from_output_value(&result);
    // SAFETY: 0..result.rows() is within Mat bounds
    let points = (0..result.rows())
        .map(|i| unsafe { result.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|pred| pred[4] >= 0.5)
        .map(|pred| remap_from_yolo(pred, size, w_ratio, h_ratio, left, top))
        .filter_map(|bbox| to_minimap_coordinate(bbox, minimap, bound, player, size))
        .collect::<Vec<_>>();
    Ok(points)
}

fn detect_esc_settings(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static ESC_SETTINGS: LazyLock<[Mat; 7]> = LazyLock::new(|| {
        [
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_SETTING_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
            imgcodecs::imdecode(include_bytes!(env!("ESC_MENU_TEMPLATE")), IMREAD_GRAYSCALE)
                .unwrap(),
            imgcodecs::imdecode(include_bytes!(env!("ESC_EVENT_TEMPLATE")), IMREAD_GRAYSCALE)
                .unwrap(),
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_COMMUNITY_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_CHARACTER_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
            imgcodecs::imdecode(include_bytes!(env!("ESC_OK_TEMPLATE")), IMREAD_GRAYSCALE).unwrap(),
            imgcodecs::imdecode(
                include_bytes!(env!("ESC_CANCEL_TEMPLATE")),
                IMREAD_GRAYSCALE,
            )
            .unwrap(),
        ]
    });

    for template in &*ESC_SETTINGS {
        if detect_template(mat, template, Point::default(), 0.85).is_ok() {
            return true;
        }
    }
    false
}

fn detect_elite_boss_bar(mat: &impl MatTraitConst) -> bool {
    /// TODO: Support default ratio
    static TEMPLATE_1: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ELITE_BOSS_BAR_1_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static TEMPLATE_2: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ELITE_BOSS_BAR_2_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let size = mat.size().unwrap();
    // crop to top part of the image for boss bar
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(0, 0, size.width, crop_y);
    let boss_bar = mat.roi(crop_bbox).unwrap();
    let template_1 = &*TEMPLATE_1;
    let template_2 = &*TEMPLATE_2;
    detect_template(&boss_bar, template_1, Point::default(), 0.9).is_ok()
        || detect_template(&boss_bar, template_2, Point::default(), 0.9).is_ok()
}

fn detect_minimap(mat: &impl MatTraitConst, border_threshold: u8) -> Result<Rect> {
    static MINIMAP_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("MINIMAP_MODEL"))))
            .expect("unable to build minimap detection session")
    });

    enum Border {
        Top,
        Bottom,
        Left,
        Right,
    }

    fn scan_border(minimap: &impl MatTraitConst, border: Border, border_threshold: u8) -> i32 {
        let mut counts = HashMap::<u32, u32>::new();
        match border {
            Border::Top | Border::Bottom => {
                let col_start = (minimap.cols() as f32 * 0.1) as i32 - 1;
                let col_end = minimap.cols() - col_start;
                for col in col_start..col_end {
                    let mut count = 0;
                    for row in 0..minimap.rows() {
                        let row = if matches!(border, Border::Bottom) {
                            minimap.rows() - row - 1
                        } else {
                            row
                        };
                        let pixel = minimap.at_2d::<Vec4b>(row, col).unwrap();
                        if pixel.into_iter().all(|v| v >= border_threshold) {
                            count += 1;
                        } else {
                            break;
                        }
                    }
                    counts.entry(count).and_modify(|c| *c += 1).or_insert(1);
                }
            }
            Border::Left | Border::Right => {
                let row_start = (minimap.rows() as f32 * 0.1) as i32 - 1;
                let row_end = minimap.rows() - row_start;
                for row in row_start..row_end {
                    let mut count = 0;
                    for col in 0..minimap.cols() {
                        let col = if matches!(border, Border::Right) {
                            minimap.cols() - col - 1
                        } else {
                            col
                        };
                        let pixel = minimap.at_2d::<Vec4b>(row, col).unwrap();
                        if pixel.into_iter().all(|v| v >= border_threshold) {
                            count += 1;
                        } else {
                            break;
                        }
                    }
                    counts.entry(count).and_modify(|c| *c += 1).or_insert(1);
                }
            }
        };
        counts
            .into_iter()
            .max_by_key(|e| e.1)
            .map(|e| e.0)
            .unwrap_or_default() as i32
    }

    let size = mat.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(mat);
    let result = MINIMAP_MODEL
        .run([norm_rgb_to_input_value(&mat_in)])
        .unwrap();
    let mat_out = from_output_value(&result);
    let pred = (0..mat_out.rows())
        // SAFETY: 0..result.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .max_by(|&a, &b| {
            // a and b have shapes [bbox(4) + class(1)]
            a[4].total_cmp(&b[4])
        })
        .filter(|pred| pred[4] >= 0.7)
        .ok_or(anyhow!("minimap detection failed"))?;

    debug!(target: "minimap", "yolo detection: {pred:?}");

    // Extract the thresholded minimap
    let minimap_bbox = remap_from_yolo(pred, size, w_ratio, h_ratio, left, top);
    let mut minimap_thresh = to_grayscale(&mat.roi(minimap_bbox).unwrap(), true);
    unsafe {
        // SAFETY: threshold can be called in place.
        minimap_thresh.modify_inplace(|mat, mat_mut| {
            threshold(mat, mat_mut, border_threshold as f64, 255.0, THRESH_BINARY).unwrap()
        });
    }

    // Find the contours with largest area
    let mut contours = Vector::<Vector<Point>>::new();
    find_contours_def(
        &minimap_thresh,
        &mut contours,
        RETR_EXTERNAL,
        CHAIN_APPROX_SIMPLE,
    )
    .unwrap();
    let contour_bbox = contours
        .into_iter()
        .map(|contour| bounding_rect(&contour).unwrap())
        .max_by_key(|bbox| bbox.area())
        .ok_or(anyhow!("minimap contours is empty"))?
        + minimap_bbox.tl();
    let intersection = (contour_bbox & minimap_bbox).area() as f32;
    let union = (contour_bbox | minimap_bbox).area() as f32;
    let iou = intersection / union;
    if iou < 0.8 {
        bail!("wrong minimap likely caused by detection during map switching")
    }

    // Scan the 4 borders and crop
    let minimap = mat.roi(contour_bbox).unwrap();
    let top = scan_border(&minimap, Border::Top, border_threshold);
    let bottom = scan_border(&minimap, Border::Bottom, border_threshold);
    // Left side gets a discount because it is darker than the other three borders
    let left = scan_border(&minimap, Border::Left, border_threshold.saturating_sub(10));
    let right = scan_border(&minimap, Border::Right, border_threshold);

    debug!(target: "minimap", "crop white border left {left}, top {top}, bottom {bottom}, right {right}");

    let bbox = Rect::new(
        left,
        top,
        minimap.cols() - right - left,
        minimap.rows() - bottom - top,
    );
    debug!(target: "minimap", "bbox {bbox:?}");

    Ok(bbox + contour_bbox.tl())
}

fn detect_minimap_portals<T: MatTraitConst + ToInputArray>(minimap: T) -> Result<Vec<Rect>> {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("PORTAL_TEMPLATE")), IMREAD_COLOR).unwrap()
    });

    let template = &*TEMPLATE;
    let mut result = Mat::default();
    let mut points = Vector::<Point>::new();
    match_template(
        &minimap,
        template,
        &mut result,
        TM_CCOEFF_NORMED,
        &no_array(),
    )
    .unwrap();
    // SAFETY: threshold can be called inplace
    unsafe {
        result.modify_inplace(|mat, mat_mut| {
            threshold(mat, mat_mut, 0.8, 1.0, THRESH_BINARY).unwrap();
        });
    }
    find_non_zero(&result, &mut points).unwrap();
    let portals = points
        .into_iter()
        .map(|point| {
            let size = 5;
            let x = (point.x - size).max(0);
            let xd = point.x - x;
            let y = (point.y - size).max(0);
            let yd = point.y - y;
            let width = template.cols() + xd * 2 + (size - xd);
            let height = template.rows() + yd * 2 + (size - yd);
            Rect::new(x, y, width, height)
        })
        .collect::<Vec<_>>();
    Ok(portals)
}

fn detect_minimap_rune(minimap: &impl ToInputArray) -> Result<Rect> {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_TEMPLATE")), IMREAD_COLOR).unwrap()
    });
    static TEMPLATE_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_MASK_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    // Expands by 2 pixels to preserve previous position calculation. Previous template is 11x11
    // while the current template is 9x9
    detect_template_single(minimap, &*TEMPLATE, &*TEMPLATE_MASK, Point::default(), 0.75)
        .map(|(rect, _)| Rect::new(rect.x - 1, rect.y - 1, rect.width + 2, rect.height + 2))
}

fn detect_player(mat: &impl ToInputArray) -> Result<Rect> {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("PLAYER_TEMPLATE")), IMREAD_COLOR).unwrap()
    });

    // Expands by 2 pixels to preserve previous position calculation. Previous template is 10x10
    // while the current template is 8x8.
    detect_template_single(mat, &*TEMPLATE, no_array(), Point::default(), 0.75)
        .map(|(rect, _)| Rect::new(rect.x - 1, rect.y - 1, rect.width + 2, rect.height + 2))
}

fn detect_player_kind(mat: &impl ToInputArray, kind: OtherPlayerKind) -> bool {
    /// TODO: Support default ratio
    static STRANGER_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_STRANGER_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static GUILDIE_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("PLAYER_GUILDIE_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static FRIEND_TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("PLAYER_FRIEND_TEMPLATE")), IMREAD_COLOR).unwrap()
    });

    match kind {
        OtherPlayerKind::Stranger => {
            detect_template(mat, &*STRANGER_TEMPLATE, Point::default(), 0.85).is_ok()
        }
        OtherPlayerKind::Guildie => {
            detect_template(mat, &*GUILDIE_TEMPLATE, Point::default(), 0.85).is_ok()
        }
        OtherPlayerKind::Friend => {
            detect_template(mat, &*FRIEND_TEMPLATE, Point::default(), 0.85).is_ok()
        }
    }
}

fn detect_player_is_dead(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static TEMPLATE: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("TOMB_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(mat, &*TEMPLATE, Point::default(), 0.8).is_ok()
}

fn detect_player_in_cash_shop(mat: &impl ToInputArray) -> bool {
    /// TODO: Support default ratio
    static CASH_SHOP: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("CASH_SHOP_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    detect_template(mat, &*CASH_SHOP, Point::default(), 0.7).is_ok()
}

fn detect_player_health_bar(mat: &impl ToInputArray) -> Result<Rect> {
    /// TODO: Support default ratio
    static HP_START: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HP_START_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });
    static HP_END: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HP_END_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });

    let hp_start = detect_template(mat, &*HP_START, Point::default(), 0.8)?;
    let hp_start_to_edge_x = hp_start.x + hp_start.width;
    let hp_end = detect_template(mat, &*HP_END, Point::default(), 0.8)?;
    Ok(Rect::new(
        hp_start_to_edge_x,
        hp_start.y,
        hp_end.x - hp_start_to_edge_x,
        hp_start.height,
    ))
}

fn detect_player_current_max_health_bars(
    mat: &impl MatTraitConst,
    grayscale: &impl MatTraitConst,
    hp_bar: Rect,
) -> Result<(Rect, Rect)> {
    /// TODO: Support default ratio
    static HP_SEPARATOR_1: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HP_SEPARATOR_1_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static HP_SEPARATOR_2: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("HP_SEPARATOR_2_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static HP_SHIELD: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("HP_SHIELD_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });
    static HP_SEPARATOR_TYPE_1: AtomicBool = AtomicBool::new(true);

    let hp_separator_type_1 = HP_SEPARATOR_TYPE_1.load(Ordering::Relaxed);
    let hp_separator_template = if hp_separator_type_1 {
        &*HP_SEPARATOR_1
    } else {
        &*HP_SEPARATOR_2
    };
    let hp_separator = detect_template(
        &grayscale.roi(hp_bar).unwrap(),
        hp_separator_template,
        hp_bar.tl(),
        0.7,
    )
    .inspect_err(|_| {
        HP_SEPARATOR_TYPE_1.store(!hp_separator_type_1, Ordering::Release);
    })?;
    let hp_shield = detect_template(
        &grayscale.roi(hp_bar).unwrap(),
        &*HP_SHIELD,
        hp_bar.tl(),
        0.8,
    )
    .ok();
    let left = mat
        .roi(Rect::new(
            hp_bar.x,
            hp_bar.y,
            hp_separator.x - hp_bar.x,
            hp_bar.height,
        ))
        .unwrap();
    let (left_in, left_w_ratio, left_h_ratio) = preprocess_for_text_bboxes(&left);
    let left_bbox = extract_text_bboxes(&left_in, left_w_ratio, left_h_ratio, hp_bar.x, hp_bar.y)
        .into_iter()
        .min_by_key(|bbox| ((bbox.x + bbox.width) - hp_separator.x).abs())
        .ok_or(anyhow!("failed to detect current health bar"))?;
    let left_bbox_x = hp_shield
        .map(|bbox| bbox.x + bbox.width)
        .unwrap_or(left_bbox.x); // When there is shield, skips past it
    let left_bbox = Rect::new(
        left_bbox_x,
        left_bbox.y - 1, // Add some space so the bound is not too tight
        hp_separator.x - left_bbox_x + 1, // Help thin character like '1' detectable
        left_bbox.height + 2,
    );
    let right = mat
        .roi(Rect::new(
            hp_separator.x + hp_separator.width,
            hp_bar.y,
            (hp_bar.x + hp_bar.width) - (hp_separator.x + hp_separator.width),
            hp_bar.height,
        ))
        .unwrap();
    let (right_in, right_w_ratio, right_h_ratio) = preprocess_for_text_bboxes(&right);
    let right_bbox = extract_text_bboxes(
        &right_in,
        right_w_ratio,
        right_h_ratio,
        hp_separator.x + hp_separator.width,
        hp_bar.y,
    )
    .into_iter()
    .reduce(|acc, cur| acc | cur)
    .ok_or(anyhow!("failed to detect max health bar"))?;
    Ok((left_bbox, right_bbox))
}

fn detect_player_health(
    mat: &impl MatTraitConst,
    current_bar: Rect,
    max_bar: Rect,
) -> Result<(u32, u32)> {
    let current_health = extract_texts(mat, &[current_bar]);
    let current_health = current_health
        .first()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(anyhow!("cannot detect current health"))?;
    let max_health = extract_texts(mat, &[max_bar]);
    let max_health = max_health
        .first()
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(anyhow!("cannot detect max health"))?;
    Ok((current_health.min(max_health), max_health))
}

fn detect_player_buff<T: MatTraitConst + ToInputArray>(mat: &T, kind: BuffKind) -> bool {
    /// TODO: Support default ratio
    static RUNE_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(include_bytes!(env!("RUNE_BUFF_TEMPLATE")), IMREAD_GRAYSCALE).unwrap()
    });
    static SAYRAM_ELIXIR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("SAYRAM_ELIXIR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static AURELIA_ELIXIR_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("AURELIA_ELIXIR_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static EXP_COUPON_X3_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_COUPON_X3_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static BONUS_EXP_COUPON_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("BONUS_EXP_COUPON_BUFF_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static LEGION_WEALTH_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_WEALTH_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static LEGION_LUCK_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_LUCK_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static LEGION_WEALTH_LUCK_BUFF_MASK: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("LEGION_WEALTH_LUCK_BUFF_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });
    static WEALTH_EXP_POTION_MASK: LazyLock<Mat> = LazyLock::new(|| {
        let mut mat = imgcodecs::imdecode(
            include_bytes!(env!("WEALTH_EXP_POTION_MASK_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap();
        unsafe {
            mat.modify_inplace(|mat, mat_mut| {
                mat.convert_to(mat_mut, CV_32FC3, 1.0 / 255.0, 0.0).unwrap();
            });
        }
        mat
    });
    static WEALTH_ACQUISITION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("WEALTH_ACQUISITION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXP_ACCUMULATION_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXP_ACCUMULATION_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_RED_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_RED_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_BLUE_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_BLUE_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_GREEN_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_GREEN_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });
    static EXTREME_GOLD_POTION_BUFF: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("EXTREME_GOLD_POTION_BUFF_TEMPLATE")),
            IMREAD_COLOR,
        )
        .unwrap()
    });

    let threshold = match kind {
        BuffKind::Rune | BuffKind::AureliaElixir => 0.8,
        BuffKind::LegionWealth
        | BuffKind::WealthAcquisitionPotion
        | BuffKind::ExpAccumulationPotion => 0.7,
        BuffKind::SayramElixir
        | BuffKind::ExpCouponX3
        | BuffKind::BonusExpCoupon
        | BuffKind::LegionLuck
        | BuffKind::ExtremeRedPotion
        | BuffKind::ExtremeBluePotion
        | BuffKind::ExtremeGreenPotion
        | BuffKind::ExtremeGoldPotion => 0.75,
    };
    let template = match kind {
        BuffKind::Rune => &*RUNE_BUFF,
        BuffKind::SayramElixir => &*SAYRAM_ELIXIR_BUFF,
        BuffKind::AureliaElixir => &*AURELIA_ELIXIR_BUFF,
        BuffKind::ExpCouponX3 => &*EXP_COUPON_X3_BUFF,
        BuffKind::BonusExpCoupon => &*BONUS_EXP_COUPON_BUFF,
        BuffKind::LegionWealth => &*LEGION_WEALTH_BUFF,
        BuffKind::LegionLuck => &*LEGION_LUCK_BUFF,
        BuffKind::WealthAcquisitionPotion => &*WEALTH_ACQUISITION_POTION_BUFF,
        BuffKind::ExpAccumulationPotion => &*EXP_ACCUMULATION_POTION_BUFF,
        BuffKind::ExtremeRedPotion => &*EXTREME_RED_POTION_BUFF,
        BuffKind::ExtremeBluePotion => &*EXTREME_BLUE_POTION_BUFF,
        BuffKind::ExtremeGreenPotion => &*EXTREME_GREEN_POTION_BUFF,
        BuffKind::ExtremeGoldPotion => &*EXTREME_GOLD_POTION_BUFF,
    };

    match kind {
        BuffKind::WealthAcquisitionPotion | BuffKind::ExpAccumulationPotion => {
            // Because the two potions are really similar, detecting one may mis-detect for the other.
            // Can't really think of a better way to do this.... But this seems working just fine.
            // Also tested with the who-use-this? Invicibility Potion and Resistance Potion. Those two
            // doesn't match at all so this should be fine.
            let matches = detect_template_multiple(
                mat,
                template,
                &*WEALTH_EXP_POTION_MASK,
                Point::default(),
                2,
                threshold,
            )
            .into_iter()
            .filter_map(|result| result.ok())
            .collect::<Vec<_>>();
            if matches.is_empty() {
                return false;
            }
            // Likely both potions are active
            if matches.len() == 2 {
                return true;
            }

            let template_other = if matches!(kind, BuffKind::WealthAcquisitionPotion) {
                &*EXP_ACCUMULATION_POTION_BUFF
            } else {
                &*WEALTH_ACQUISITION_POTION_BUFF
            };
            let match_current = matches.into_iter().next().unwrap();
            let match_other = detect_template_single(
                mat,
                template_other,
                &*WEALTH_EXP_POTION_MASK,
                Point::default(),
                threshold,
            );

            match_other.is_err()
                || match_other.as_ref().copied().unwrap().0 != match_current.0
                || match_other.unwrap().1 < match_current.1
        }
        BuffKind::LegionWealth | BuffKind::LegionLuck => detect_template_single(
            mat,
            template,
            &*LEGION_WEALTH_LUCK_BUFF_MASK,
            Point::default(),
            threshold,
        )
        .is_ok(),
        _ => detect_template(mat, template, Point::default(), threshold).is_ok(),
    }
}

fn detect_rune_arrows_with_scores_regions(mat: &impl MatTraitConst) -> Vec<(Rect, KeyKind, f32)> {
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

    let size = mat.size().unwrap();
    let (mat_in, w_ratio, h_ratio, left, top) = preprocess_for_yolo(mat);
    let result = RUNE_MODEL.run([norm_rgb_to_input_value(&mat_in)]).unwrap();
    let mat_out = from_output_value(&result);
    let mut vec = (0..mat_out.rows())
        // SAFETY: 0..outputs.rows() is within Mat bounds
        .map(|i| unsafe { mat_out.at_row_unchecked::<f32>(i).unwrap() })
        .filter(|pred| pred[4] >= 0.2)
        .map(|pred| {
            (
                remap_from_yolo(pred, size, w_ratio, h_ratio, left, top),
                map_arrow(pred),
                pred[4],
            )
        })
        .collect::<Vec<_>>();
    vec.sort_by_key(|a| a.0.x);
    vec
}

fn detect_rune_arrows(
    mat: &impl MatTraitConst,
    mut calibrating: ArrowsCalibrating,
) -> Result<ArrowsState> {
    /// The minimum region width required to contain 4 arrows
    ///
    /// Based on the rectangular region in-game with round border when detecting arrows.
    const RUNE_REGION_MIN_WIDTH: i32 = 300;
    const SCORE_THRESHOLD: f32 = 0.8;

    if calibrating.rune_region.is_none() {
        let result = detect_rune_arrows_with_scores_regions(mat);
        calibrating.rune_region = result
            .clone()
            .into_iter()
            .map(|(r, _, _)| r)
            .reduce(|acc, cur| acc | cur)
            .filter(|region| region.width >= RUNE_REGION_MIN_WIDTH);

        // Cache result for later
        let filtered = result
            .into_iter()
            .filter_map(|(rect, arrow, score)| (score >= SCORE_THRESHOLD).then_some((rect, arrow)))
            .collect::<Vec<_>>();
        if !filtered.is_empty() && filtered.len() <= MAX_ARROWS {
            calibrating.normal_arrows = Some(Array::from_iter(filtered));
        }
    }

    #[cfg(debug_assertions)]
    if calibrating.is_spin_testing {
        calibrating.rune_region = Some(Rect::new(0, 0, mat.cols(), mat.rows()));
    }

    let rune_region = calibrating
        .rune_region
        .ok_or(anyhow!("rune region not found"))?;

    // If there is no previous calibrating, try it once
    if !calibrating.spin_arrows_calibrated {
        calibrating.spin_arrows_calibrated = true;
        calibrate_for_spin_arrows(mat, rune_region, &mut calibrating)?;
        return Ok(ArrowsState::Calibrating(calibrating));
    }

    // After calibration is complete and there are spin arrows, prioritize its detection
    if let Some(ref mut spin_arrows) = calibrating.spin_arrows
        && spin_arrows.iter().any(|arrow| arrow.final_arrow.is_none())
    {
        for spin_arrow in spin_arrows
            .iter_mut()
            .filter(|arrow| arrow.final_arrow.is_none())
        {
            detect_spin_arrow(mat, spin_arrow)?;
        }
        return Ok(ArrowsState::Calibrating(calibrating));
    }

    // Reuse cached result if any
    if let Some(arrows) = calibrating.normal_arrows {
        calibrating.normal_arrows = None;

        if calibrating.spin_arrows.is_none() && arrows.len() == MAX_ARROWS {
            debug!(target: "rune", "reuse cached arrows result");
            return Ok(ArrowsState::Complete(extract_rune_arrows_to_slice(
                arrows.into_iter().collect::<Vec<_>>(),
            )));
        }

        if let Some(ref spin_arrows) = calibrating.spin_arrows {
            let mut final_arrows = Vec::new();

            for arrow in spin_arrows {
                final_arrows.push((arrow.region, arrow.final_arrow.unwrap()));
            }
            for (arrow_region, arrow) in arrows {
                let mut use_arrow = true;
                for region in spin_arrows.iter().map(|arrow| arrow.region) {
                    let intersection = (arrow_region & region).area() as f32;
                    let union = (arrow_region | region).area() as f32;
                    let iou = intersection / union;
                    if iou >= 0.5 {
                        use_arrow = false;
                        debug!(target: "rune", "skip using cached result for normal {arrow_region:?} and spin {region:?} with IoU {iou}");
                        break;
                    }
                }
                if use_arrow {
                    final_arrows.push((arrow_region, arrow));
                }
            }

            if final_arrows.len() == MAX_ARROWS {
                debug!(target: "rune", "reuse cached arrows result with spin arrows");
                final_arrows.sort_by_key(|(region, _)| region.x);
                return Ok(ArrowsState::Complete(extract_rune_arrows_to_slice(
                    final_arrows,
                )));
            }
        }

        debug!(target: "rune", "cached result not used");
    }

    // Normal detection path
    let mut mat = mat.roi(rune_region)?;
    if calibrating.spin_arrows.is_some() {
        //  Set all spin arrow regions to black pixels
        let mut mat_copy = mat.clone_pointee();
        for region in calibrating
            .spin_arrows
            .as_ref()
            .unwrap()
            .iter()
            .map(|arrow| arrow.region)
        {
            mat_copy
                .roi_mut(region - rune_region.tl())?
                .set_scalar(Scalar::default())?;
        }
        mat = BoxedRef::from(mat_copy);
    }

    let result = detect_rune_arrows_with_scores_regions(&mat)
        .into_iter()
        .filter_map(|(rect, arrow, score)| (score >= SCORE_THRESHOLD).then_some((rect, arrow)))
        .collect::<Vec<_>>();
    // TODO: If there are spinning arrows, either set the limit internally
    // or ensure caller only try to solve rune for a fixed time frame. Otherwise, it may
    // return `[ArrowsState::Calibrating]` forever.
    if calibrating.spin_arrows.is_some() {
        if result.len() != MAX_ARROWS / 2 {
            return Ok(ArrowsState::Calibrating(calibrating));
        }
        let mut vec = calibrating
            .spin_arrows
            .take()
            .unwrap()
            .into_iter()
            .map(|arrow| (arrow.region - rune_region.tl(), arrow.final_arrow.unwrap()))
            .chain(result)
            .collect::<Vec<_>>();
        vec.sort_by_key(|a| a.0.x);
        return Ok(ArrowsState::Complete(extract_rune_arrows_to_slice(vec)));
    }

    if result.len() == MAX_ARROWS {
        Ok(ArrowsState::Complete(extract_rune_arrows_to_slice(result)))
    } else {
        Err(anyhow!("no rune arrow detected"))
    }
}

fn calibrate_for_spin_arrows(
    mat: &impl MatTraitConst,
    rune_region: Rect,
    calibrating: &mut ArrowsCalibrating,
) -> Result<()> {
    const SPIN_REGION_PAD: i32 = 16;
    const SPIN_ARROW_AREA_PIXEL_THRESHOLD: i32 = 200;
    const SPIN_ARROW_AREA_THRESHOLD: i32 = 520;

    // Extract the saturation channel and perform thresholding
    let mut rune_region_mat = to_hsv(&mat.roi(rune_region)?);
    unsafe {
        rune_region_mat.modify_inplace(|mat, mat_mut| {
            extract_channel(mat, mat_mut, 1).unwrap();
            #[cfg(debug_assertions)]
            if calibrating.is_spin_testing {
                debug_mat("Rune Region Before Thresh", mat, 0, &[]);
            }
            threshold(mat, mat_mut, 245.0, 255.0, THRESH_BINARY).unwrap();
        });
    }

    #[cfg(debug_assertions)]
    if calibrating.is_spin_testing {
        debug_mat("Rune Region", &rune_region_mat, 0, &[]);
    }

    let mut centroids = Mat::default();
    let mut stats = Mat::default();
    let labels_count = connected_components_with_stats(
        &rune_region_mat,
        &mut Mat::default(),
        &mut stats,
        &mut centroids,
        8,
        CV_32S,
    )
    .unwrap();
    // Maximum number of spinning arrows is 2
    let mut spin_arrows = Array::new();

    for i in 1..labels_count {
        let area = *stats.at_2d::<i32>(i, CC_STAT_AREA).unwrap();
        let w = *stats.at_2d::<i32>(i, CC_STAT_WIDTH).unwrap();
        let h = *stats.at_2d::<i32>(i, CC_STAT_HEIGHT).unwrap();
        // Spinning arrow has bigger area than normal rune
        if area < SPIN_ARROW_AREA_PIXEL_THRESHOLD && (w * h) < SPIN_ARROW_AREA_THRESHOLD {
            continue;
        }
        if spin_arrows.len() >= MAX_SPIN_ARROWS {
            debug!(target:"rune", "number of spin arrows exceeded limit, possibly false positives");
            return Ok(());
        }

        let centroid = centroids.row(i).unwrap();
        let centroid = centroid.data_typed::<f64>().unwrap();
        let centroid = Point::new(
            rune_region.x + centroid[0] as i32,
            rune_region.y + centroid[1] as i32,
        );

        let x = *stats.at_2d::<i32>(i, CC_STAT_LEFT).unwrap();
        let y = *stats.at_2d::<i32>(i, CC_STAT_TOP).unwrap();

        // Pad to ensure the region always contain the spin arrow even when it rotates
        // horitzontally or vertically
        let padded_x = (x - SPIN_REGION_PAD).max(0);
        let padded_y = (y - SPIN_REGION_PAD).max(0);
        let padded_w = (padded_x + w + SPIN_REGION_PAD * 2).min(rune_region.width) - padded_x;
        let padded_h = (padded_y + h + SPIN_REGION_PAD * 2).min(rune_region.height) - padded_y;

        let rect = Rect::new(
            rune_region.x + padded_x,
            rune_region.y + padded_y,
            padded_w,
            padded_h,
        );

        #[cfg(debug_assertions)]
        if calibrating.is_spin_testing {
            debug_mat(
                "Spin Arrow",
                &rune_region_mat,
                0,
                &[(rect - rune_region.tl(), "Region")],
            );
        }

        spin_arrows.push(SpinArrow {
            centroid,
            region: rect,
            last_arrow_head: None,
            final_arrow: None,
            #[cfg(debug_assertions)]
            is_spin_testing: calibrating.is_spin_testing,
        });
    }

    if spin_arrows.len() == MAX_SPIN_ARROWS {
        debug!(target: "rune", "{} spinning rune arrows detected, calibrating...", spin_arrows.len());
        calibrating.spin_arrows = Some(spin_arrows);
    }

    Ok(())
}

fn detect_spin_arrow(mat: &impl MatTraitConst, spin_arrow: &mut SpinArrow) -> Result<()> {
    const INTERPOLATE_FROM_CENTROID: f32 = 0.785;
    const SPIN_LAG_THRESHOLD: i32 = 25;
    const SPIN_ARROW_HUE_THRESHOLD: u8 = 30;

    // Extract spin arrow region
    let spin_arrow_mat = to_hsv(&mat.roi(spin_arrow.region)?);
    let kernel = get_structuring_element_def(MORPH_RECT, Size::new(3, 3)).unwrap();
    let mut spin_arrow_thresh = Mat::default();
    unsafe {
        spin_arrow_thresh.modify_inplace(|mat, mat_mut| {
            extract_channel(&spin_arrow_mat, mat_mut, 1).unwrap();
            threshold(mat, mat_mut, 245.0, 255.0, THRESH_BINARY).unwrap();
            dilate_def(mat, mat_mut, &kernel).unwrap();
        })
    }

    let mut contours = Vector::<Vector<Point>>::new();
    find_contours_def(
        &spin_arrow_thresh,
        &mut contours,
        RETR_EXTERNAL,
        CHAIN_APPROX_SIMPLE,
    )
    .unwrap();
    if contours.is_empty() {
        bail!("cannot find the spinning arrow contour")
    }

    let mut points = [Point2f::default(); 4];
    let rect = min_area_rect(&contours.get(0).unwrap()).unwrap();
    rect.points(&mut points).unwrap();

    // Determine the two short edges of a rectangle following the points order
    // returned by `[RotatedRect::points]`
    let mut first_short_edge_center = Point2f::default();
    let mut second_short_edge_center = Point2f::default();
    if (points[0] - points[1]).norm() < (points[0] - points[3]).norm() {
        first_short_edge_center.x = (points[0].x + points[1].x) / 2.0;
        first_short_edge_center.y = (points[0].y + points[1].y) / 2.0;

        second_short_edge_center.x = (points[3].x + points[2].x) / 2.0;
        second_short_edge_center.y = (points[3].y + points[2].y) / 2.0;
    } else {
        first_short_edge_center.x = (points[0].x + points[3].x) / 2.0;
        first_short_edge_center.y = (points[0].y + points[3].y) / 2.0;

        second_short_edge_center.x = (points[2].x + points[1].x) / 2.0;
        second_short_edge_center.y = (points[2].y + points[1].y) / 2.0;
    }

    // Determine which edge is the arrow head by first computing the collinear point
    // from the centroid to the center point of each edges
    let centroid = spin_arrow.centroid - spin_arrow.region.tl();
    let first_collinear = first_short_edge_center * INTERPOLATE_FROM_CENTROID
        + centroid.to::<f32>().unwrap() * (1.0 - INTERPOLATE_FROM_CENTROID);
    let first_collinear = first_collinear.to::<i32>().unwrap();

    let second_collinear = second_short_edge_center * INTERPOLATE_FROM_CENTROID
        + centroid.to::<f32>().unwrap() * (1.0 - INTERPOLATE_FROM_CENTROID);
    let second_collinear = second_collinear.to::<i32>().unwrap();

    let collinear = if let Some(last_collinear) = spin_arrow.last_arrow_head {
        if (last_collinear - centroid).dot(first_collinear - centroid) > 0 {
            first_collinear
        } else {
            second_collinear
        }
    } else {
        // Check the hue to determine the arrow head
        let first_hue = spin_arrow_mat
            .at_pt::<Vec3b>(first_collinear)
            .unwrap()
            .first()
            .copied()
            .unwrap();
        let second_hue = spin_arrow_mat
            .at_pt::<Vec3b>(second_collinear)
            .unwrap()
            .first()
            .copied()
            .unwrap();
        if first_hue <= SPIN_ARROW_HUE_THRESHOLD {
            first_collinear
        } else if second_hue <= SPIN_ARROW_HUE_THRESHOLD {
            second_collinear
        } else {
            bail!("failed to determine spinning arrow head")
        }
    };

    if spin_arrow.last_arrow_head.is_none() {
        spin_arrow.last_arrow_head = Some(collinear);
        return Ok(());
    }

    let prev_arrow_head = spin_arrow.last_arrow_head.unwrap() - centroid;
    let cur_arrow_head = collinear - centroid;
    // https://stackoverflow.com/a/13221874
    let dot = prev_arrow_head.x * -cur_arrow_head.y + prev_arrow_head.y * cur_arrow_head.x;
    if dot >= SPIN_LAG_THRESHOLD {
        debug!(target: "rune", "spinning arrow lag detected");
        let up = prev_arrow_head.dot(Point::new(0, -1));
        let down = prev_arrow_head.dot(Point::new(0, 1));
        let left = prev_arrow_head.dot(Point::new(-1, 0));
        let right = prev_arrow_head.dot(Point::new(1, 0));
        let results = [up, down, left, right];
        let (index, _) = results
            .iter()
            .enumerate()
            .max_by_key(|(_, dot)| **dot)
            .unwrap();
        let arrow = match index {
            0 => KeyKind::Up,
            1 => KeyKind::Down,
            2 => KeyKind::Left,
            3 => KeyKind::Right,
            _ => unreachable!(),
        };
        debug!(target: "rune", "spinning arrow result {arrow:?} {results:?}");
        spin_arrow.final_arrow = Some(arrow);
    }
    spin_arrow.last_arrow_head = Some(collinear);

    #[cfg(debug_assertions)]
    if spin_arrow.is_spin_testing {
        debug_spinning_arrows(
            mat,
            &contours,
            spin_arrow.region,
            prev_arrow_head,
            cur_arrow_head,
            spin_arrow.centroid,
        );
    }

    Ok(())
}

#[inline]
fn extract_rune_arrows_to_slice(vec: Vec<(Rect, KeyKind)>) -> [KeyKind; MAX_ARROWS] {
    debug_assert!(vec.len() == 4);
    let first = vec[0].1;
    let second = vec[1].1;
    let third = vec[2].1;
    let fourth = vec[3].1;
    info!( target: "player", "solving rune result {first:?} {second:?} {third:?} {fourth:?}");
    [first, second, third, fourth]
}

fn detect_erda_shower(mat: &impl MatTraitConst) -> Result<Rect> {
    /// TODO: Support default ratio
    static ERDA_SHOWER: LazyLock<Mat> = LazyLock::new(|| {
        imgcodecs::imdecode(
            include_bytes!(env!("ERDA_SHOWER_TEMPLATE")),
            IMREAD_GRAYSCALE,
        )
        .unwrap()
    });

    let size = mat.size().unwrap();
    // crop to bottom right of the image for skill bar
    let crop_x = size.width / 2;
    let crop_y = size.height / 5;
    let crop_bbox = Rect::new(size.width - crop_x, size.height - crop_y, crop_x, crop_y);
    let skill_bar = mat.roi(crop_bbox).unwrap();
    detect_template(&skill_bar, &*ERDA_SHOWER, crop_bbox.tl(), 0.96)
}

/// Detects a single match from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    offset: Point,
    threshold: f64,
) -> Result<Rect> {
    detect_template_single(mat, template, no_array(), offset, threshold).map(|(bbox, _)| bbox)
}

/// Detects a single match with `mask` from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template_single<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    mask: impl ToInputArray,
    offset: Point,
    threshold: f64,
) -> Result<(Rect, f64)> {
    detect_template_multiple(mat, template, mask, offset, 1, threshold)
        .into_iter()
        .next()
        .unwrap()
}

/// Detects multiple matches from `template` with the given BGR image `Mat`.
#[inline]
fn detect_template_multiple<T: ToInputArray + MatTraitConst>(
    mat: &impl ToInputArray,
    template: &T,
    mask: impl ToInputArray,
    offset: Point,
    max_matches: usize,
    threshold: f64,
) -> Vec<Result<(Rect, f64)>> {
    #[inline]
    fn clear_result(result: &mut Mat, rect: Rect, offset: Point) {
        let x = rect.x - offset.x;
        let y = rect.y - offset.y;
        let roi_rect = Rect::new(
            x,
            y,
            rect.width.min(result.cols() - x),
            rect.height.min(result.rows() - y),
        );
        result
            .roi_mut(roi_rect)
            .unwrap()
            .set_scalar(Scalar::default())
            .unwrap();
    }

    #[inline]
    fn match_one(
        result: &Mat,
        offset: Point,
        template_size: Size,
        threshold: f64,
    ) -> (Rect, Result<(Rect, f64)>) {
        let mut score = 0f64;
        let mut loc = Point::default();
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
        let br = tl + Point::from_size(template_size);
        let rect = Rect::from_points(tl, br);
        if score < threshold {
            (rect, Err(anyhow!("template not found").context(score)))
        } else {
            (rect, Ok((rect, score)))
        }
    }

    let mut result = Mat::default();
    if let Err(err) = match_template(mat, template, &mut result, TM_CCOEFF_NORMED, &mask) {
        error!(target: "detect", "template detection error {err}");
        return vec![];
    }

    let template_size = template.size().unwrap();
    let max_matches = max_matches.max(1);
    if max_matches == 1 {
        // Weird INFINITY values when match template with mask
        // https://github.com/opencv/opencv/issues/23257
        loop {
            let (rect, match_result) = match_one(&result, offset, template_size, threshold);
            if match_result
                .as_ref()
                .is_ok_and(|(_, score)| *score == f64::INFINITY)
            {
                clear_result(&mut result, rect, offset);
                continue;
            }
            return vec![match_result];
        }
    }

    let mut filter = Vec::new();
    for _ in 0..max_matches {
        loop {
            let (rect, match_result) = match_one(&result, offset, template_size, threshold);
            clear_result(&mut result, rect, offset);
            // Weird INFINITY values when match template with mask
            // https://github.com/opencv/opencv/issues/23257
            if match_result
                .as_ref()
                .is_ok_and(|(_, score)| *score == f64::INFINITY)
            {
                continue;
            }

            filter.push(match_result);
            break;
        }
    }
    filter
}

/// Extracts texts from the non-preprocessed `Mat` and detected text bounding boxes.
fn extract_texts(mat: &impl MatTraitConst, bboxes: &[Rect]) -> Vec<String> {
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

    let recognizier = TEXT_RECOGNITION_MODEL.lock().unwrap();
    bboxes
        .iter()
        .copied()
        .filter_map(|word| {
            let mut mat = mat.roi(word).unwrap().clone_pointee();
            unsafe {
                mat.modify_inplace(|mat, mat_mut| {
                    cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
                });
            }
            recognizier.recognize(&mat).ok()
        })
        .collect()
}

/// Extracts text bounding boxes from the preprocessed [`Mat`].
///
/// This function is adapted from
/// https://github.com/clovaai/CRAFT-pytorch/blob/master/craft_utils.py#L19 with minor changes
fn extract_text_bboxes(
    mat_in: &impl MatTraitConst,
    w_ratio: f32,
    h_ratio: f32,
    x_offset: i32,
    y_offset: i32,
) -> Vec<Rect> {
    const TEXT_SCORE_THRESHOLD: f64 = 0.7;
    const LINK_SCORE_THRESHOLD: f64 = 0.4;
    static TEXT_DETECTION_MODEL: LazyLock<Session> = LazyLock::new(|| {
        Session::builder()
            .and_then(|b| b.commit_from_memory(include_bytes!(env!("TEXT_DETECTION_MODEL"))))
            .expect("unable to build minimap name detection session")
    });

    let result = TEXT_DETECTION_MODEL
        .run([norm_rgb_to_input_value(mat_in)])
        .unwrap();
    let mat = from_output_value(&result);
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
        THRESH_BINARY,
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
            threshold(mat, mat_mut, LINK_SCORE_THRESHOLD, 1.0, THRESH_BINARY).unwrap();
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
        if area < 10 {
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
        let kernel =
            get_structuring_element_def(MORPH_RECT, Size::new(size + 1, size + 1)).unwrap();

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

#[inline]
fn remap_from_yolo(
    pred: &[f32],
    size: Size,
    w_ratio: f32,
    h_ratio: f32,
    left: i32,
    top: i32,
) -> Rect {
    let tl_x = ((pred[0] - left as f32) / w_ratio)
        .max(0.0)
        .min(size.width as f32);
    let tl_y = ((pred[1] - top as f32) / h_ratio)
        .max(0.0)
        .min(size.height as f32);
    let br_x = ((pred[2] - left as f32) / w_ratio)
        .max(0.0)
        .min(size.width as f32);
    let br_y = ((pred[3] - top as f32) / h_ratio)
        .max(0.0)
        .min(size.height as f32);
    Rect::from_points(
        Point::new(tl_x as i32, tl_y as i32),
        Point::new(br_x as i32, br_y as i32),
    )
}

/// Preprocesses a BGRA `Mat` image to a normalized and resized RGB `Mat` image with type `f32`
/// for YOLO detection.
///
/// Returns a triplet of `(Mat, width_ratio, height_ratio, left, top)`
#[inline]
fn preprocess_for_yolo(mat: &impl MatTraitConst) -> (Mat, f32, f32, i32, i32) {
    // https://github.com/ultralytics/ultralytics/blob/main/ultralytics/data/augment.py
    let mut mat = mat.try_clone().unwrap();

    let size = mat.size().unwrap();
    let (w_ratio, h_ratio) = (640.0 / size.width as f32, 640.0 / size.height as f32);
    let min_ratio = w_ratio.min(h_ratio);

    let w = (size.width as f32 * min_ratio).round();
    let h = (size.height as f32 * min_ratio).round();

    let pad_w = (640.0 - w) / 2.0;
    let pad_h = (640.0 - h) / 2.0;

    let top = (pad_h - 0.1).round() as i32;
    let bottom = (pad_h + 0.1).round() as i32;
    let left = (pad_w - 0.1).round() as i32;
    let right = (pad_w + 0.1).round() as i32;

    // SAFETY: all of the functions below can be called in place.
    unsafe {
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2RGB).unwrap();
            resize(
                mat,
                mat_mut,
                Size::new(w as i32, h as i32),
                0.0,
                0.0,
                INTER_LINEAR,
            )
            .unwrap();
            copy_make_border(
                mat,
                mat_mut,
                top,
                bottom,
                left,
                right,
                BORDER_CONSTANT,
                Scalar::all(114.0),
            )
            .unwrap();
            mat.convert_to(mat_mut, CV_32FC3, 1.0 / 255.0, 0.0).unwrap();
        });
    }
    (mat, min_ratio, min_ratio, left, top)
}

/// Preprocesses a BGRA `Mat` image to a normalized and resized RGB `Mat` image with type `f32`
/// for text bounding boxes detection.
///
/// The preprocess is adapted from: https://github.com/clovaai/CRAFT-pytorch/blob/master/imgproc.py
///
/// Returns a `(Mat, width_ratio, height_ratio)`.
#[inline]
fn preprocess_for_text_bboxes(mat: &impl MatTraitConst) -> (Mat, f32, f32) {
    let mut mat = mat.try_clone().unwrap();
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
    (mat, resize_w_ratio, resize_h_ratio)
}

/// Converts an BGRA `Mat` image to HSV.
#[inline]
fn to_hsv(mat: &impl MatTraitConst) -> Mat {
    let mut mat = mat.try_clone().unwrap();
    unsafe {
        // SAFETY: can be modified inplace
        mat.modify_inplace(|mat, mat_mut| {
            cvt_color_def(mat, mat_mut, COLOR_BGRA2BGR).unwrap();
            cvt_color_def(mat, mat_mut, COLOR_BGR2HSV_FULL).unwrap();
        });
    }
    mat
}

/// Converts an BGRA `Mat` image to BGR.
#[inline]
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
#[inline]
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
/// The returned `Mat` has shape `[..dims]` with batch size (1) removed.
#[inline]
fn from_output_value(result: &SessionOutputs) -> Mat {
    let (dims, outputs) = result["output0"].try_extract_raw_tensor::<f32>().unwrap();
    let dims = dims.iter().map(|&dim| dim as i32).collect::<Vec<i32>>();
    let mat = Mat::new_nd_with_data(dims.as_slice(), outputs).unwrap();
    let mat = mat.reshape_nd(1, &dims.as_slice()[1..]).unwrap();
    mat.clone_pointee()
}

/// Converts a continuous, normalized `f32` RGB `Mat` image to `SessionInputValue`.
///
/// The input `Mat` is assumed to be continuous, normalized RGB `f32` data type and
/// will panic if not. The `Mat` is reshaped to single channel, tranposed to `[1, 3, H, W]` and
/// converted to `SessionInputValue`.
#[inline]
fn norm_rgb_to_input_value(mat: &impl MatTraitConst) -> SessionInputValue {
    let mat = mat.reshape_nd(1, &[1, mat.rows(), mat.cols(), 3]).unwrap();
    let mut mat_t = Mat::default();
    transpose_nd(&mat, &Vector::from_slice(&[0, 3, 1, 2]), &mut mat_t).unwrap();
    let shape = mat_t.mat_size();
    let input = (shape.as_slice(), mat_t.data_typed::<f32>().unwrap());
    let tensor = Tensor::from_array(input).unwrap();
    SessionInputValue::Owned(tensor.into_dyn())
}
