use opencv::{
    core::{Mat, Point, Scalar},
    highgui::{imshow, wait_key},
    imgcodecs::imwrite_def,
    imgproc::{LINE_8, rectangle_points},
};
use platforms::windows::{capture::DynamicCapture, handle::Handle, keys::Keys};

use crate::error::Error;

use super::{detector::to_grayscale, mat::OwnedMat, minimap::MinimapState, player::PlayerState};

pub struct Context {
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
    let mut context = Context {
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
        draw_debug(&mut context, &mut mat);
    }
}

#[cfg(debug_assertions)]
fn draw_debug(context: &mut Context, mat: &mut Mat) {
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
            idle.move_to(Point::new(215, 62));
            let _ = rectangle_points(
                &mut mat,
                idle.rect.tl(),
                idle.rect.br(),
                Scalar::from_array([255., 0., 0., 255.]),
                1,
                LINE_8,
                0,
            )
            .unwrap();
        }
        _ => (),
    }
    let _ = imshow("Debug", &mat);
    let _ = wait_key(1);
    let _ = imwrite_def((env!("OUT_DIR").to_owned() + "test.png").as_str(), &mat);
}
