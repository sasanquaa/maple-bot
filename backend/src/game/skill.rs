use opencv::core::{Mat, Rect};

use super::{Context, Contextual, detect::detect_erda_shower};

#[derive(Clone, Copy, Debug)]
pub enum Skill {
    Detecting,
    Idle(Rect),
}

#[derive(Clone, Copy, Debug)]
pub enum SkillKind {
    ErdaShower,
}

impl Contextual for Skill {
    type Extra = SkillKind;

    fn update(&self, _: &Context, mat: &Mat, extra: SkillKind) -> Self {
        match self {
            Skill::Detecting => match extra {
                SkillKind::ErdaShower => detect_erda_shower(mat, 0.65),
            }
            .map(Skill::Idle)
            .unwrap_or(Skill::Detecting),
            Skill::Idle(rect) => Skill::Idle(*rect),
        }
    }
}
