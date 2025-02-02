use anyhow::Result;
use opencv::{
    core::{Mat, Scalar},
    highgui::{imshow, wait_key},
    imgproc::{LINE_8, rectangle_points},
};
use platforms::windows::{capture::DynamicCapture, handle::Handle, keys::Keys};

use super::{
    clock::FpsClock, mat::OwnedMat, minimap::MinimapState, player::PlayerState, skill::SkillState,
};

pub struct Context {
    pub keys: Keys,
    pub minimap: MinimapState,
    pub player: PlayerState,
    pub skill: SkillState,
}

pub trait UpdateState {
    fn update(&self, context: &Context, mat: &Mat) -> Self;
}

pub fn update_loop() -> Result<()> {
    let handle = Handle::new(Some("MapleStoryClass"), None)?;
    let capture = DynamicCapture::new(handle.clone())?;
    let keys = Keys::new(handle);
    let clock = FpsClock::new(30);
    let mut context = Context {
        keys,
        minimap: MinimapState::Detecting,
        player: PlayerState::Detecting,
        skill: SkillState::Detecting,
    };
    loop {
        let Ok(mat) = capture.grab().map(OwnedMat::new) else {
            continue;
        };
        context.minimap = context.minimap.update(&context, &mat);
        context.player = context.player.update(&context, &mat);
        // context.skill = context.skill.update(&context, &grayscale);
        draw_debug(&mut context, &mat);
        clock.tick();
    }
}

#[cfg(debug_assertions)]
fn draw_debug(context: &mut Context, mat: &Mat) {
    use opencv::core::Point;

    static mut COUNTER: usize = 0;

    let mut mat = mat.clone();

    if let MinimapState::Idle(idle) = &context.minimap {
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

    if let PlayerState::Idle(idle) = &mut context.player {
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
            c if c == 0 => {
                // idle.move_to(Point2f::new(0.9098039, 0.54285717));
                // idle.move_to(Point::new(193, 65));
                idle.move_to(Point::new(223, 37));
            }
            c if c == 1 => {
                // idle.move_to(Point2f::new(0.12109375, 0.60294116));
                // idle.move_to(Point::new(183, 65));
                idle.move_to(Point::new(113, 32));
            }
            c if c == 2 => {
                // idle.move_to(Point2f::new(0.5234375, 0.27941176));
                // idle.move_to(Point::new(193, 22));
                idle.move_to(Point::new(22, 19));
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
    let _ = imshow("Debug", &mat);
    let _ = wait_key(1);
}
