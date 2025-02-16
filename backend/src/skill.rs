use log::debug;
use opencv::core::{Mat, MatTraitConst, Point, Rect, Vec4b};

use super::{
    context::{Context, Contextual, ControlFlow},
    detect::detect_erda_shower,
};

const SKILL_OFF_COOLDOWN_MAX_TIMEOUT: u32 = 1800;
const SKILL_OFF_COOLDOWN_DETECT_EVERY: u32 = 300;

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

    fn update(self, _: &Context, mat: &Mat, state: &mut SkillState) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, mat, state))
    }
}

fn update_context(contextual: Skill, mat: &Mat, state: &mut SkillState) -> Skill {
    match contextual {
        Skill::Detecting => match state.kind {
            SkillKind::ErdaShower => detect_erda_shower(mat),
        }
        .map(|bbox| {
            state.anchor = get_anchor(mat, bbox);
            Skill::Idle
        })
        .unwrap_or(Skill::Detecting),
        Skill::Idle => {
            let pixel = mat.at_pt::<Vec4b>(state.anchor.0).unwrap();
            if *pixel != state.anchor.1 {
                debug!(target: "skill", "assume skill to be on cooldown {:?} != {:?}, could be false positive", state.anchor, pixel);
                // try to assume it is on cooldown
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
                    return if delayed {
                        state.anchor = get_anchor(mat, bbox);
                        Skill::Idle
                    } else {
                        Skill::Cooldown(timeout, true)
                    };
                } else {
                    debug!(target: "skill", "skill still in cooldown");
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
