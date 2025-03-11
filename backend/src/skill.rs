use log::debug;
use opencv::core::{Mat, MatTraitConst, Point, Rect, Vec4b};
use strum::{Display, EnumIter};

use crate::{
    context::{Context, Contextual, ControlFlow, Timeout, update_with_timeout},
    detect::Detector,
};

const SKILL_OFF_COOLDOWN_MAX_TIMEOUT: u32 = 1800;
const SKILL_OFF_COOLDOWN_DETECT_EVERY: u32 = 35;

#[derive(Debug)]
pub struct SkillState {
    kind: SkillKind,
}

impl SkillState {
    pub fn new(kind: SkillKind) -> Self {
        Self { kind }
    }
}

#[derive(Clone, Copy, Debug, Display)]
pub enum Skill {
    Detecting(Timeout),
    Idle(Point, Vec4b),
    #[strum(to_string = "Cooldown (Can be wrong)")]
    Cooldown(Timeout),
}

#[derive(Clone, Copy, Debug, EnumIter)]
pub enum SkillKind {
    ErdaShower,
}

impl Contextual for Skill {
    type Persistent = SkillState;

    fn update(
        self,
        _: &Context,
        detector: &mut impl Detector,
        state: &mut SkillState,
    ) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, detector, state))
    }
}

fn update_context(
    contextual: Skill,
    detector: &mut impl Detector,
    state: &mut SkillState,
) -> Skill {
    match contextual {
        Skill::Detecting(timeout) => update_with_timeout(
            timeout,
            SKILL_OFF_COOLDOWN_DETECT_EVERY * 2,
            (),
            |_, timeout| {
                match state.kind {
                    SkillKind::ErdaShower => detector.detect_erda_shower(),
                }
                .map(|bbox| {
                    let (point, pixel) = get_anchor(detector.mat(), bbox);
                    Skill::Idle(point, pixel)
                })
                .unwrap_or(Skill::Detecting(timeout))
            },
            |_| Skill::Detecting(Timeout::default()),
            |_, timeout| Skill::Detecting(timeout),
        ),
        Skill::Idle(anchor_point, anchor_pixel) => {
            let Ok(pixel) = detector.mat().at_pt::<Vec4b>(anchor_point) else {
                return Skill::Detecting(Timeout::default());
            };
            if *pixel != anchor_pixel {
                debug!(target: "skill", "assume skill to be on cooldown {:?} != {:?}, could be false positive", (anchor_point, anchor_pixel), pixel);
                // assume it is on cooldown
                Skill::Cooldown(Timeout::default())
            } else {
                Skill::Idle(anchor_point, anchor_pixel)
            }
        }
        Skill::Cooldown(timeout) => {
            fn on_next(
                state: &mut SkillState,
                detector: &mut impl Detector,
                timeout: Timeout,
            ) -> Skill {
                // rechecks after every amount of ticks
                // to see if it is still on cooldown
                // or if it is false positive
                if timeout.current % SKILL_OFF_COOLDOWN_DETECT_EVERY == 0 {
                    let result = match state.kind {
                        SkillKind::ErdaShower => detector.detect_erda_shower(),
                    };
                    if let Ok(bbox) = result {
                        let (point, pixel) = get_anchor(detector.mat(), bbox);
                        return Skill::Idle(point, pixel);
                    }
                }
                Skill::Cooldown(timeout)
            }
            update_with_timeout(
                timeout,
                SKILL_OFF_COOLDOWN_MAX_TIMEOUT,
                (state, detector),
                |(state, detector), timeout| on_next(state, detector, timeout),
                |_| Skill::Detecting(Timeout::default()),
                |(state, detector), timeout| on_next(state, detector, timeout),
            )
        }
    }
}

#[inline]
fn get_anchor(mat: &Mat, bbox: Rect) -> (Point, Vec4b) {
    let point = (bbox.tl() + bbox.br()) / 2;
    let pixel = mat.at_pt::<Vec4b>(point).unwrap();
    let anchor = (point, *pixel);
    debug!(target: "skill", "detected at {bbox:?} with anchor {:?}", anchor);
    anchor
}

#[cfg(test)]
mod tests {
    use crate::detect::MockDetector;

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
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        let (mat, rect) = create_test_mat_bbox(255);
        detector.expect_mat().return_const(mat);
        detector
            .expect_detect_erda_shower()
            .returning(move || Ok(rect));

        let skill = update_context(
            Skill::Detecting(Timeout::default()),
            &mut detector,
            &mut state,
        );
        assert!(matches!(skill, Skill::Idle(_, _)));
        match skill {
            Skill::Idle(point, pixel) => {
                assert_eq!(point, (rect.tl() + rect.br()) / 2);
                assert_eq!(pixel, Vec4b::all(255));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn skill_idle_to_cooldown() {
        let (mat, rect) = create_test_mat_bbox(254);
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        detector.expect_mat().return_const(mat);

        let skill = update_context(
            Skill::Idle((rect.tl() + rect.br()) / 2, Vec4b::all(255)),
            &mut detector,
            &mut state,
        );
        assert!(matches!(skill, Skill::Cooldown(_)));
    }
    #[test]
    fn skill_cooldown_to_detecting() {
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        let timeout = Timeout {
            started: true,
            current: SKILL_OFF_COOLDOWN_MAX_TIMEOUT,
        };
        let skill = Skill::Cooldown(timeout);
        let skill = update_context(skill, &mut detector, &mut state);
        assert!(matches!(skill, Skill::Detecting(_)));
    }

    #[test]
    fn skill_cooldown_recheck_ok() {
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        let (mat, rect) = create_test_mat_bbox(255);
        detector.expect_mat().return_const(mat);
        detector
            .expect_detect_erda_shower()
            .returning(move || Ok(rect));

        let timeout = Timeout {
            started: true,
            current: SKILL_OFF_COOLDOWN_DETECT_EVERY - 1,
        };
        let skill = Skill::Cooldown(timeout);
        let skill = update_context(skill, &mut detector, &mut state);
        assert!(matches!(skill, Skill::Idle(_, _)));
        match skill {
            Skill::Idle(point, pixel) => {
                assert_eq!(point, (rect.tl() + rect.br()) / 2);
                assert_eq!(pixel, Vec4b::all(255));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn skill_cooldown_recheck_err() {
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let mut detector = MockDetector::new();
        detector
            .expect_detect_erda_shower()
            .returning(move || Err(anyhow!("error")));

        let timeout = Timeout {
            started: true,
            current: SKILL_OFF_COOLDOWN_MAX_TIMEOUT - 1,
        };
        let skill = Skill::Cooldown(timeout);
        let skill = update_context(skill, &mut detector, &mut state);
        assert!(matches!(skill, Skill::Cooldown(_)));
    }
}
