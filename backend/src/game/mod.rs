mod clock;
mod detect;
mod mat;
pub mod minimap;
pub mod player;
pub mod skill;

use anyhow::{Result, anyhow};
use opencv::core::{Mat, MatTraitConst, MatTraitConstManual, Vec4b};
use platforms::windows::{capture::DynamicCapture, handle::Handle, keys::Keys};

use clock::FpsClock;
use mat::OwnedMat;
use minimap::Minimap;
use player::Player;
use skill::{Skill, SkillKind};

pub(crate) trait Contextual {
    type Extra = ();

    fn update(&self, context: &Context, mat: &Mat, extra: Self::Extra) -> Self;
}

pub struct Context {
    clock: FpsClock,
    pub(crate) keys: Keys,
    capture: DynamicCapture,
    pub(crate) minimap: Minimap,
    pub(crate) player: Player,
    pub(crate) skills: Vec<Skill>,
    frame: OwnedMat,
}

impl Context {
    pub fn new() -> Result<Self> {
        let clock = FpsClock::new(30);
        let handle = Handle::new(Some("MapleStoryClass"), None)?;
        let keys = Keys::new(handle.clone());
        let capture = DynamicCapture::new(handle.clone())?;
        Ok(Context {
            clock,
            keys,
            capture,
            minimap: Minimap::Detecting,
            player: Player::Detecting,
            skills: vec![Skill::Detecting],
            frame: OwnedMat::empty(),
        })
    }

    pub fn update_loop(mut self, mut on_updated: impl FnMut(&Context)) {
        loop {
            let Ok(mat) = self.capture.grab().map(OwnedMat::new) else {
                continue;
            };
            self.minimap = self.minimap.update(&self, &mat, ());
            self.player = self.player.update(&self, &mat, ());
            self.skills = self
                .skills
                .iter()
                .map(|skill| skill.update(&self, &mat, SkillKind::ErdaShower))
                .collect();
            self.frame = mat;
            on_updated(&self);
            self.clock.tick();
        }
    }

    pub fn minimap(&self) -> Result<(Vec<u8>, usize, usize)> {
        if let Minimap::Idle(idle) = self.minimap {
            let minimap = self
                .frame
                .roi(idle.bbox)?
                .iter::<Vec4b>()?
                .flat_map(|bgra| {
                    let bgra = bgra.1;
                    [bgra[2], bgra[1], bgra[0], 255]
                })
                .collect::<Vec<u8>>();
            return Ok((minimap, idle.bbox.width as usize, idle.bbox.height as usize));
        }
        Err(anyhow!("minimap not found"))
    }

    pub fn minimap_name(&self) -> Result<Vec<u8>> {
        if let Minimap::Idle(idle) = self.minimap {
            let name = self
                .frame
                .roi(idle.bbox_name)?
                .iter::<Vec4b>()?
                .flat_map(|rgba| rgba.1)
                .collect::<Vec<u8>>();
            return Ok(name);
        }
        Err(anyhow!("minimap name not found"))
    }
}
