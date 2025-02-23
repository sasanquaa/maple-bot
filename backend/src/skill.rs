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
