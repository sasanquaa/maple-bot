use std::{
    any::{Any, TypeId},
    collections::HashMap,
};

use opencv::{
    core::{Mat, Scalar},
    highgui::{imshow, wait_key},
    imgcodecs::imwrite_def,
    imgproc::{LINE_8, rectangle_points},
};
use platforms::windows::{capture::DynamicCapture, handle::Handle, keys::Keys};

use crate::error::Error;

use super::{
    clock::FpsClock, detector::to_grayscale, mat::OwnedMat, minimap::MinimapState,
    player::PlayerState,
};

pub struct Context {
    pub fps: u32,
    pub keys: Keys,
    pub minimap: MinimapState,
    pub player: PlayerState,
}

pub trait UpdateState {
    fn update(&self, context: &Context, grayscale: &Mat) -> Self;
}

pub fn update_loop() -> Result<(), Error> {
    let handle = Handle::new(Some("MapleStoryClass"), None)?;
    let capture = DynamicCapture::new(handle.clone())?;
    let keys = Keys::new(handle);
    let fps = 30;
    let clock = FpsClock::new(fps);
    let mut context = Context {
        fps,
        keys,
        minimap: MinimapState::Detecting,
        player: PlayerState::Detecting,
    };
    loop {
        let Ok(mut mat) = capture.grab().map_err(Error::from).map(OwnedMat::new) else {
            continue;
        };
        let grayscale = to_grayscale(&mat, Some(1.5), Some(-80.0)).unwrap();
        context.minimap = context.minimap.update(&context, &grayscale);
        context.player = context.player.update(&context, &grayscale);
        if cfg!(debug_assertions) {
            println!("player state: {:?}", context.player);
        }
        draw_debug(&mut context, &mut mat);
        clock.tick();
    }
}

#[cfg(debug_assertions)]
fn draw_debug(context: &mut Context, mat: &mut Mat) {
    use opencv::core::Point;

    static mut COUNTER: usize = 0;

    let mut mat = to_grayscale(mat, None, None).unwrap();
    match &context.minimap {
        MinimapState::Idle(idle) => {
            let rect = idle.rect;
            let _ = rectangle_points(
                &mut mat,
                rect.tl(),
                rect.br(),
                Scalar::from_array([255., 0., 0., 255.]),
                1,
                LINE_8,
                0,
            )
            .unwrap();
        }
        _ => (),
    }
    match &mut context.player {
        PlayerState::Idle(idle) => {
            match unsafe { COUNTER } {
                c if c == 0 => {
                    idle.move_to(Point::new(193, 67));
                }
                c if c == 1 => {
                    idle.move_to(Point::new(183, 67));
                }
                c if c == 2 => {
                    idle.move_to(Point::new(193, 22));
                }
                c if c == 3 => {
                    idle.move_to(Point::new(172, 22));
                }
                c if c == 4 => {
                    idle.move_to(Point::new(50, 65));
                }
                c if c == 5 => {
                    idle.move_to(Point::new(50, 36));
                }
                c if c == 6 => {
                    idle.move_to(Point::new(65, 22));
                }
                _ => (),
            }
            unsafe {
                COUNTER += 1;
                COUNTER = COUNTER % 7;
            };
        }
        _ => (),
    }
    let _ = imshow("Debug", &mat);
    let _ = wait_key(1);
    let _ = imwrite_def((env!("OUT_DIR").to_owned() + "test.png").as_str(), &mat);
}
