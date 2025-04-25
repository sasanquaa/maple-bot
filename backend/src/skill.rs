use std::{
    mem,
    ops::{Index, IndexMut},
};

use anyhow::Result;
use log::debug;
use opencv::core::{MatTraitConst, Point, Rect, Vec4b};
use strum::{Display, EnumIter};

use crate::{
    context::{Context, Contextual, ControlFlow},
    player::Player,
    task::{Task, Update, update_detection_task},
};

#[derive(Debug)]
pub struct SkillState {
    kind: SkillKind,
    task: Option<Task<Result<(Point, Vec4b)>>>,
}

impl SkillState {
    pub fn new(kind: SkillKind) -> Self {
        Self { kind, task: None }
    }
}

#[derive(Clone, Copy, Debug, Display)]
pub enum Skill {
    Detecting,
    Idle(Point, Vec4b),
    Cooldown,
}

#[derive(Clone, Copy, Debug, EnumIter)]
pub enum SkillKind {
    ErdaShower,
    // TODO: Sol Janus?
}

impl SkillKind {
    pub const COUNT: usize = mem::variant_count::<SkillKind>();
}

impl Index<SkillKind> for [Skill; SkillKind::COUNT] {
    type Output = Skill;

    fn index(&self, index: SkillKind) -> &Self::Output {
        self.get(index as usize).unwrap()
    }
}

impl IndexMut<SkillKind> for [Skill; SkillKind::COUNT] {
    fn index_mut(&mut self, index: SkillKind) -> &mut Self::Output {
        self.get_mut(index as usize).unwrap()
    }
}

impl Contextual for Skill {
    type Persistent = SkillState;

    fn update(self, context: &Context, state: &mut SkillState) -> ControlFlow<Self> {
        let next = if matches!(context.player, Player::CashShopThenExit(_, _)) {
            self
        } else {
            update_context(self, context, state)
        };
        ControlFlow::Next(next)
    }
}

fn update_context(contextual: Skill, context: &Context, state: &mut SkillState) -> Skill {
    match contextual {
        Skill::Detecting => update_detection(contextual, context, state, Skill::Idle),
        Skill::Idle(anchor_point, anchor_pixel) => {
            let Ok(pixel) = context.detector_unwrap().mat().at_pt::<Vec4b>(anchor_point) else {
                return Skill::Detecting;
            };
            if !anchor_match(*pixel, anchor_pixel) {
                debug!(target: "skill", "assume skill to be on cooldown {:?} != {:?}, could be false positive", (anchor_point, anchor_pixel), pixel);
                // assume it is on cooldown
                Skill::Cooldown
            } else {
                Skill::Idle(anchor_point, anchor_pixel)
            }
        }
        Skill::Cooldown => update_detection(contextual, context, state, Skill::Idle),
    }
}

#[inline]
fn update_detection(
    contextual: Skill,
    context: &Context,
    state: &mut SkillState,
    on_next: impl FnOnce(Point, Vec4b) -> Skill,
) -> Skill {
    let kind = state.kind;
    let update = update_detection_task(context, 1000, &mut state.task, move |detector| {
        let bbox = match kind {
            SkillKind::ErdaShower => detector.detect_erda_shower()?,
        };
        Ok(get_anchor(detector.mat(), bbox))
    });
    match update {
        Update::Ok((point, pixel)) => on_next(point, pixel),
        Update::Err(err) => {
            if err.downcast::<f64>().unwrap() < 0.52 {
                Skill::Detecting
            } else {
                contextual
            }
        }
        Update::Pending => contextual,
    }
}

#[inline]
fn anchor_match(anchor: Vec4b, pixel: Vec4b) -> bool {
    const ANCHOR_ACCEPTABLE_ERROR_RANGE: u32 = 45;

    let b = anchor[0].abs_diff(pixel[0]) as u32;
    let g = anchor[1].abs_diff(pixel[1]) as u32;
    let r = anchor[2].abs_diff(pixel[2]) as u32;
    let avg = (b + g + r) / 3; // Average for grayscale
    avg <= ANCHOR_ACCEPTABLE_ERROR_RANGE
}

#[inline]
fn get_anchor(mat: &impl MatTraitConst, bbox: Rect) -> (Point, Vec4b) {
    let point = (bbox.tl() + bbox.br()) / 2;
    let pixel = mat.at_pt::<Vec4b>(point).unwrap();
    let anchor = (point, *pixel);
    debug!(target: "skill", "detected at {bbox:?} with anchor {:?}", anchor);
    anchor
}

#[cfg(test)]
mod tests {
    use std::{assert_matches::assert_matches, time::Duration};

    use anyhow::{Context as AnyhowContext, anyhow};
    use opencv::core::{CV_8UC4, Mat, MatExprTraitConst, MatTrait};
    use tokio::time::advance;

    use super::*;
    use crate::detect::MockDetector;

    fn create_test_mat_bbox(center_pixel: u8) -> (Mat, Rect) {
        let mut mat = Mat::zeros(100, 100, CV_8UC4).unwrap().to_mat().unwrap();
        let rect = Rect::new(0, 0, 100, 100);
        let center = (rect.tl() + rect.br()) / 2;
        *mat.at_pt_mut::<Vec4b>(center).unwrap() = Vec4b::all(center_pixel);
        (mat, rect)
    }

    fn create_mock_detector(center_pixel: u8, error: Option<f64>) -> (MockDetector, Rect) {
        let mut detector = MockDetector::new();
        let (mat, rect) = create_test_mat_bbox(center_pixel);
        detector
            .expect_clone()
            .returning(move || create_mock_detector(center_pixel, error).0);
        detector.expect_mat().return_const(mat.into());
        if let Some(error) = error {
            detector
                .expect_detect_erda_shower()
                .returning(move || Err(anyhow!("")).context(error));
        } else {
            detector
                .expect_detect_erda_shower()
                .returning(move || Ok(rect));
        }
        (detector, rect)
    }

    async fn advance_task(contextual: Skill, context: &Context, state: &mut SkillState) -> Skill {
        let mut skill = update_context(contextual, context, state);
        while !state.task.as_ref().unwrap().completed() {
            skill = update_context(skill, context, state);
            advance(Duration::from_millis(1000)).await;
        }
        skill
    }

    #[tokio::test(start_paused = true)]
    async fn skill_detecting_to_idle() {
        let (detector, rect) = create_mock_detector(255, None);
        let context = Context::new(None, Some(detector));
        let mut state = SkillState::new(SkillKind::ErdaShower);

        let skill = advance_task(Skill::Detecting, &context, &mut state).await;
        assert_matches!(skill, Skill::Idle(_, _));
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
        let (detector, rect) = create_mock_detector(200, None);
        let context = Context::new(None, Some(detector));
        let mut state = SkillState::new(SkillKind::ErdaShower);

        let skill = update_context(
            Skill::Idle((rect.tl() + rect.br()) / 2, Vec4b::all(255)),
            &context,
            &mut state,
        );
        assert_matches!(skill, Skill::Cooldown);
    }

    #[tokio::test(start_paused = true)]
    async fn skill_cooldown_to_detecting() {
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let (detector, _) = create_mock_detector(255, Some(0.51));
        let context = Context::new(None, Some(detector));

        let skill = advance_task(Skill::Cooldown, &context, &mut state).await;
        assert_matches!(skill, Skill::Detecting);
    }

    #[tokio::test(start_paused = true)]
    async fn skill_cooldown_recheck_ok() {
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let (detector, rect) = create_mock_detector(255, None);
        let context = Context::new(None, Some(detector));

        let skill = advance_task(Skill::Cooldown, &context, &mut state).await;
        assert_matches!(skill, Skill::Idle(_, _));
        match skill {
            Skill::Idle(point, pixel) => {
                assert_eq!(point, (rect.tl() + rect.br()) / 2);
                assert_eq!(pixel, Vec4b::all(255));
            }
            _ => unreachable!(),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn skill_cooldown_recheck_err() {
        let mut state = SkillState::new(SkillKind::ErdaShower);
        let (detector, _) = create_mock_detector(255, Some(0.52));
        let context = Context::new(None, Some(detector));

        let skill = advance_task(Skill::Cooldown, &context, &mut state).await;
        assert_matches!(skill, Skill::Cooldown);
    }
}
