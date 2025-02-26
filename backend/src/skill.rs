use log::debug;
use opencv::core::{Mat, MatTraitConst, Point, Rect, Vec4b};

use crate::{
    context::{Context, Contextual, ControlFlow},
    detect::Detector,
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
    Cooldown(u32),
}

#[derive(Clone, Copy, Debug)]
pub enum SkillKind {
    ErdaShower,
}

impl Contextual for Skill {
    type Persistent = SkillState;

    fn update(
        self,
        context: &Context,
        detector: &mut impl Detector,
        state: &mut SkillState,
    ) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, context, detector, state))
    }
}

fn update_context(
    contextual: Skill,
    context: &Context,
    detector: &mut impl Detector,
    state: &mut SkillState,
) -> Skill {
    match contextual {
        Skill::Detecting => {
            if !matches!(context.minimap, crate::minimap::Minimap::Idle(_)) {
                return Skill::Detecting;
            }
            match state.kind {
                SkillKind::ErdaShower => detector.detect_erda_shower(),
            }
            .map(|bbox| {
                state.anchor = get_anchor(detector.mat(), bbox);
                Skill::Idle
            })
            .unwrap_or(Skill::Detecting)
        }
        Skill::Idle => {
            let Ok(pixel) = detector.mat().at_pt::<Vec4b>(state.anchor.0) else {
                return Skill::Detecting;
            };
            if *pixel != state.anchor.1 {
                debug!(target: "skill", "assume skill to be on cooldown {:?} != {:?}, could be false positive", state.anchor, pixel);
                // assume it is on cooldown
                Skill::Cooldown(0)
            } else {
                Skill::Idle
            }
        }
        Skill::Cooldown(timeout) => {
            let timeout = timeout + 1;
            // rechecks after every amount of ticks
            // to see if it is still on cooldown
            // or if it is false positive
            if timeout % SKILL_OFF_COOLDOWN_DETECT_EVERY == 0 {
                let result = match state.kind {
                    SkillKind::ErdaShower => detector.detect_erda_shower(),
                };
                if let Ok(bbox) = result {
                    state.anchor = get_anchor(detector.mat(), bbox);
                    return Skill::Idle;
                }
            }
            if timeout >= SKILL_OFF_COOLDOWN_MAX_TIMEOUT {
                Skill::Detecting
            } else {
                Skill::Cooldown(timeout)
            }
        }
    }
}

#[inline(always)]
fn get_anchor(mat: &Mat, bbox: Rect) -> (Point, Vec4b) {
    let point = (bbox.tl() + bbox.br()) / 2;
    let pixel = mat.at_pt::<Vec4b>(point).unwrap();
    let anchor = (point, *pixel);
    debug!(target: "skill", "detected at {bbox:?} with anchor {:?}", anchor);
    anchor
}

#[cfg(test)]
mod tests {
    use crate::{
        detect::MockDetector,
        minimap::{Minimap, MinimapIdle},
    };

    use super::*;
    use anyhow::anyhow;
    use opencv::core::{CV_8UC4, Mat, MatExprTraitConst, MatTrait};

    fn create_test_mat_bbox(center_pixel: u8) -> (Mat, Rect) {
        let mut mat = Mat::zeros(100, 100, CV_8UC4).unwrap().to_mat().unwrap();
        let rect = Rect::new(0, 0, 100, 100);
        let center = (rect.tl() + rect.br()) / 2;
        *mat.at_pt_mut::<Vec4b>(center).unwrap() = Vec4b::all(center_pixel);
        (mat, rect)
    }

    #[test]
    fn skill_detecting_to_idle() {
        let context = Context {
            minimap: Minimap::Idle(MinimapIdle::default()),
            ..Context::default()
        };
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        let (mat, rect) = create_test_mat_bbox(255);
        detector.expect_mat().return_const(mat);
        detector
            .expect_detect_erda_shower()
            .returning(move || Ok(rect));

        let skill = update_context(Skill::Detecting, &context, &mut detector, &mut state);
        assert!(matches!(skill, Skill::Idle));
        assert_eq!(state.anchor, ((rect.tl() + rect.br()) / 2, Vec4b::all(255)));
    }

    #[test]
    fn skill_idle_to_cooldown() {
        let context = Context {
            minimap: Minimap::Idle(MinimapIdle::default()),
            ..Context::default()
        };
        let (mat, rect) = create_test_mat_bbox(254);
        let mut state = SkillState::new(SkillKind::ErdaShower);
        state.anchor = ((rect.tl() + rect.br()) / 2, Vec4b::all(255));
        let mut detector = MockDetector::new();
        detector.expect_mat().return_const(mat);

        let skill = update_context(Skill::Idle, &context, &mut detector, &mut state);
        assert!(matches!(skill, Skill::Cooldown(0)));
    }
    #[test]
    fn skill_cooldown_to_detecting() {
        let context = Context {
            minimap: Minimap::Idle(MinimapIdle::default()),
            ..Context::default()
        };
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();

        let skill = Skill::Cooldown(SKILL_OFF_COOLDOWN_MAX_TIMEOUT - 1);
        let skill = update_context(skill, &context, &mut detector, &mut state);
        assert!(matches!(skill, Skill::Detecting));
    }

    #[test]
    fn skill_cooldown_recheck_ok() {
        let context = Context {
            minimap: Minimap::Idle(MinimapIdle::default()),
            ..Context::default()
        };
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        let (mat, rect) = create_test_mat_bbox(255);
        detector.expect_mat().return_const(mat);
        detector
            .expect_detect_erda_shower()
            .returning(move || Ok(rect));

        let skill = Skill::Cooldown(SKILL_OFF_COOLDOWN_DETECT_EVERY - 1);
        let skill = update_context(skill, &context, &mut detector, &mut state);
        assert!(matches!(skill, Skill::Idle));
        assert_eq!(state.anchor, ((rect.tl() + rect.br()) / 2, Vec4b::all(255)));
    }

    #[test]
    fn skill_cooldown_recheck_err() {
        let context = Context {
            minimap: Minimap::Idle(MinimapIdle::default()),
            ..Context::default()
        };
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        detector
            .expect_detect_erda_shower()
            .returning(move || Err(anyhow!("error")));

        let skill = Skill::Cooldown(SKILL_OFF_COOLDOWN_DETECT_EVERY - 1);
        let skill = update_context(skill, &context, &mut detector, &mut state);
        assert!(matches!(
            skill,
            Skill::Cooldown(SKILL_OFF_COOLDOWN_DETECT_EVERY)
        ));
    }
}
