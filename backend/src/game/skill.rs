use anyhow::Result;
use log::debug;
use opencv::core::{Mat, MatTraitConst, Rect};

use crate::game::detect::compute_mats_similarity_score;

use super::{Context, Contextual, detect::detect_erda_shower};

const SKILL_OFF_COODLOWN_THRESHOLD: f64 = 15.0;

#[derive(Debug)]
pub struct SkillState {
    kind: SkillKind,
    mat: Mat,
    bbox: Rect,
}

impl SkillState {
    pub fn new(kind: SkillKind) -> Self {
        Self {
            kind,
            mat: Mat::default(),
            bbox: Rect::default(),
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

    fn update(&self, _: &Context, mat: &Mat, state: &mut SkillState) -> Self {
        match self {
            Skill::Detecting => match state.kind {
                SkillKind::ErdaShower => detect_erda_shower(mat, 0.90),
            }
            .map(|bbox| {
                state.mat = mat.roi(bbox).unwrap().clone_pointee();
                state.bbox = bbox;
                debug!(target: "skill", "detected at {bbox:?}");
                Skill::Idle
            })
            .unwrap_or(Skill::Detecting),
            Skill::Idle => {
                let Ok(score) = compute_similarity_score(mat, state) else {
                    return Skill::Detecting;
                };
                if score <= SKILL_OFF_COODLOWN_THRESHOLD {
                    Skill::Idle
                } else {
                    state.mat = mat.roi(state.bbox).unwrap().clone_pointee();
                    debug!(target: "skill", "assume skill to be on cooldown with score {score}, could be false positive");
                    // try to assume it is on cooldown
                    // but it could be due to overlapping UI, animations, effects, ...
                    Skill::Cooldown(0)
                }
            }
            Skill::Cooldown(timeout) => {
                let timeout = timeout + 1;
                // rechecks every 150 ticks (5 secs)
                // to see if it is still on cooldown
                // or if it is false positive
                if timeout >= 100 {
                    let Ok(score) = compute_similarity_score(mat, state) else {
                        return Skill::Detecting;
                    };
                    state.mat = mat.roi(state.bbox).unwrap().clone_pointee();
                    debug!(target: "skill", "in cooldown score: {score}");
                    match score {
                        s if s <= SKILL_OFF_COODLOWN_THRESHOLD => Skill::Idle,
                        // 180 is chosen through some testing because
                        // if the two images are totally different skills
                        // the score would go up to and beyond 400
                        // could be due to mentioned effects or really on cooldown
                        s if s <= 180.0 => Skill::Cooldown(0),
                        _ => Skill::Detecting,
                    }
                } else {
                    Skill::Cooldown(timeout)
                }
            }
        }
    }
}

#[inline(always)]
fn compute_similarity_score(mat: &Mat, state: &SkillState) -> Result<f64> {
    let mat = mat.roi(state.bbox)?;
    compute_mats_similarity_score(&mat, &state.mat)
}
