mod clock;
mod detect;
mod mat;
mod minimap;
mod player;
mod skill;

use anyhow::Result;
use opencv::{
    core::{Mat, Scalar},
    imgproc::{LINE_8, rectangle_points},
};
use platforms::windows::{capture::DynamicCapture, handle::Handle, keys::Keys};

use clock::FpsClock;
use mat::OwnedMat;
use minimap::Minimap;
use player::Player;
use skill::Skill;

pub(crate) trait Contextual {
    fn update(&self, context: &Context, mat: &Mat) -> Self;
}

pub trait Callback {
    fn on_minimap();
}

pub struct Context {
    clock: FpsClock,
    pub(crate) keys: Keys,
    capture: DynamicCapture,
    pub minimap: Minimap,
    pub player: Player,
    pub skill: Skill,
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
            skill: Skill::Detecting,
        })
    }

    pub fn start(&mut self) {
        loop {
            let Ok(mat) = self.capture.grab().map(OwnedMat::new) else {
                continue;
            };
            self.minimap = self.minimap.update(self, &mat);
            self.player = self.player.update(self, &mat);
            // context.skill = context.skill.update(&context, &grayscale);
            draw_debug(self, &mat);
            self.clock.tick();
        }
    }
}

#[cfg(debug_assertions)]
fn draw_debug(context: &mut Context, mat: &Mat) {
    use opencv::core::Point;

    static mut COUNTER: usize = 0;

    let mut mat = mat.clone();

    if let Minimap::Idle(idle) = &context.minimap {
        let rect = idle.bbox;
        rectangle_points(
            &mut mat,
            rect.tl(),
            rect.br(),
            Scalar::from_array([255., 0., 0., 255.]),
            1,
            LINE_8,
            0,
        )
        .unwrap();
        rectangle_points(
            &mut mat,
            idle.bbox_name.tl(),
            idle.bbox_name.br(),
            Scalar::from_array([255., 0., 0., 255.]),
            1,
            LINE_8,
            0,
        )
        .unwrap();
    }

    // println!("state: {:?}", context.player);

    if let Player::Idle(idle) = &mut context.player {
        // 0.9098039, 0.54285717 -> 0.12109375, 0.60294116 -> 0.5234375, 0.27941176
        rectangle_points(
            &mut mat,
            idle.bbox.tl(),
            idle.bbox.br(),
            Scalar::from_array([255., 0., 0., 255.]),
            1,
            LINE_8,
            0,
        )
        .unwrap();
        match unsafe { COUNTER } {
            0 => {
                // idle.move_to(Point2f::new(0.9098039, 0.54285717));
                // idle.move_to(Point::new(193, 65));
                // idle.move_to(Point::new(223, 37));
                idle.move_to(Point::new(206, 44));
                // idle.move_to(Point::new(22, 36));
            }
            1 => {
                // idle.move_to(Point2f::new(0.12109375, 0.60294116));
                // idle.move_to(Point::new(183, 65));
                idle.move_to(Point::new(31, 24));
            }
            2 => {
                // idle.move_to(Point2f::new(0.5234375, 0.27941176));
                // idle.move_to(Point::new(193, 22));
                idle.move_to(Point::new(118, 14));
            }
            // c if c == 3 => {
            //     idle.move_to(Point::new(172, 22));
            // }
            // c if c == 4 => {
            //     idle.move_to(Point::new(50, 65));
            // }
            // c if c == 5 => {
            //     idle.move_to(Point::new(50, 36));
            // }
            // c if c == 6 => {
            //     idle.move_to(Point::new(65, 22));
            // }
            _ => (),
        }
        unsafe {
            COUNTER += 1;
            COUNTER %= 3;
        };
    }
}
