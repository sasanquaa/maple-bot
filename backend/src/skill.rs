use log::debug;
use opencv::core::{Mat, MatTraitConst, Point, Rect, Vec4b};

use crate::{
    context::{Context, Contextual, ControlFlow},
    detect::detect_erda_shower,
};

const SKILL_OFF_COOLDOWN_MAX_TIMEOUT: u32 = 1800;
const SKILL_OFF_COOLDOWN_DETECT_EVERY: u32 = 35;

#[derive(Debug)]
pub struct SkillState {
    kind: SkillKind,
    anchor: (Point, Vec4b),
}

impl SkillState {
    pub fn new(kind: SkillKind) -> Self {
        Self {
            kind,
            anchor: (Point::default(), Vec4b::default()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Skill {
    Detecting,
    Idle,
    Cooldown(u32, bool),
}

#[derive(Clone, Copy, Debug)]
pub enum SkillKind {
    ErdaShower,
}

impl Contextual for Skill {
    type Persistent = SkillState;

    fn update(self, context: &Context, mat: &Mat, state: &mut SkillState) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, context, mat, state))
    }
}

fn update_context(
    contextual: Skill,
    context: &Context,
    mat: &Mat,
    state: &mut SkillState,
) -> Skill {
    match contextual {
        Skill::Detecting => {
            if !matches!(context.minimap, crate::minimap::Minimap::Idle(_)) {
                return Skill::Detecting;
            }
            match state.kind {
                SkillKind::ErdaShower => detect_erda_shower(mat),
            }
            .map(|bbox| {
                state.anchor = get_anchor(mat, bbox);
                Skill::Idle
            })
            .unwrap_or(Skill::Detecting)
        }
        Skill::Idle => {
            let Ok(pixel) = mat.at_pt::<Vec4b>(state.anchor.0) else {
                return Skill::Detecting;
            };
            if *pixel != state.anchor.1 {
                debug!(target: "skill", "assume skill to be on cooldown {:?} != {:?}, could be false positive", state.anchor, pixel);
                // assume it is on cooldown
                Skill::Cooldown(0, false)
            } else {
                Skill::Idle
            }
        }
        Skill::Cooldown(timeout, delayed) => {
            let timeout = timeout + 1;
            // rechecks after every amount of ticks
            // to see if it is still on cooldown
            // or if it is false positive
            if timeout % SKILL_OFF_COOLDOWN_DETECT_EVERY == 0 {
                let result = match state.kind {
                    SkillKind::ErdaShower => detect_erda_shower(mat),
                };
                if let Ok(bbox) = result {
                    state.anchor = get_anchor(mat, bbox);
                    return Skill::Idle;
                }
            }
            if timeout >= SKILL_OFF_COOLDOWN_MAX_TIMEOUT {
                Skill::Detecting
            } else {
                Skill::Cooldown(timeout, delayed)
            }
        }
    }
}

fn get_anchor(mat: &Mat, bbox: Rect) -> (Point, Vec4b) {
    let point = (bbox.tl() + bbox.br()) / 2;
    let pixel = mat.at_pt::<Vec4b>(point).unwrap();
    let anchor = (point, *pixel);
    debug!(target: "skill", "detected at {bbox:?} with anchor {:?}", anchor);
    anchor
}

#[cfg(test)]
mod tests {
    use crate::minimap::{Minimap, MinimapIdle};

    use super::*;
    use opencv::core::{CV_8UC4, Mat, MatTrait, Scalar};

    fn create_test_mat(color: Vec4b) -> Mat {
        let mut mat = Mat::new_rows_cols_with_default(100, 100, CV_8UC4, Scalar::all(0.0)).unwrap();
        let center = Point::new(50, 50);
        *mat.at_pt_mut::<Vec4b>(center).unwrap() = color;
        mat
    }

    #[test]
    fn skill_detecting_to_idle() {
        let context = Context {
            minimap: Minimap::Idle(MinimapIdle::default()),
            ..Context::default()
        };
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mat = create_test_mat(Vec4b::from([255, 0, 0, 255]));

        let skill = Skill::Detecting;
        let updated_skill = update_context(skill, &context, &mat, &mut state);

        assert!(matches!(updated_skill, Skill::Idle));
    }

    #[test]
    fn test_skill_idle_to_cooldown() {
        let context = Context::default();
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mat = create_test_mat(Vec4b::from([255, 0, 0, 255]));

        // First, transition to Idle state
        let skill = Skill::Detecting;
        let updated_skill = update_context(skill, &context, &mat, &mut state);
        assert!(matches!(updated_skill, Skill::Idle));

        // Change the pixel to simulate cooldown
        let mut mat = create_test_mat(Vec4b::from([0, 255, 0, 255]));
        let updated_skill = update_context(updated_skill, &context, &mat, &mut state);

        assert!(matches!(updated_skill, Skill::Cooldown(0, false)));
    }

    #[test]
    fn test_skill_cooldown_to_detecting() {
        let context = Context::default();
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mat = create_test_mat(Vec4b::from([255, 0, 0, 255]));

        // Transition to Cooldown state
        let skill = Skill::Cooldown(SKILL_OFF_COOLDOWN_MAX_TIMEOUT - 1, false);
        let updated_skill = update_context(skill, &context, &mat, &mut state);

        assert!(matches!(updated_skill, Skill::Detecting));
    }

    #[test]
    fn test_skill_cooldown_recheck() {
        let context = Context::default();
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mat = create_test_mat(Vec4b::from([255, 0, 0, 255]));

        // Transition to Cooldown state
        let skill = Skill::Cooldown(SKILL_OFF_COOLDOWN_DETECT_EVERY - 1, false);
        let updated_skill = update_context(skill, &context, &mat, &mut state);

        // After recheck, it should transition back to Idle if the skill is detected
        assert!(matches!(updated_skill, Skill::Idle));
    }
}
