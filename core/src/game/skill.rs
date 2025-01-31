use opencv::{core::Rect, prelude::Mat};

use super::{
    detect::detect_erda_shower,
    state::{Context, UpdateState},
};

#[derive(Debug)]
pub enum SkillState {
    Detecting,
    Idle(Rect),
}

// impl UpdateState for SkillState {
//     fn update(&self, context: &Context, grayscale: &Mat) -> Self {
//         let Ok(rect) = detect_erda_shower(grayscale, 0.65) else {
//             return Self::Detecting;
//         };
//         Self::Idle(rect)
//     }
// }
