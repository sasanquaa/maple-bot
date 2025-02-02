use opencv::core::Rect;


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
