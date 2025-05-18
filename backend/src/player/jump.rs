use super::{
    Player, PlayerState,
    moving::{MOVE_TIMEOUT, Moving},
    state::LastMovement,
    timeout::{ChangeAxis, update_moving_axis_context},
};
use crate::context::Context;

const TIMEOUT: u32 = MOVE_TIMEOUT + 3;

pub fn update_jumping_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
) -> Player {
    if !moving.timeout.started {
        state.last_movement = Some(LastMovement::Jumping);
    }

    update_moving_axis_context(
        moving,
        state.last_known_pos.unwrap(),
        TIMEOUT,
        |moving| {
            let _ = context.keys.send(state.config.jump_key);
            Player::Jumping(moving)
        },
        None::<fn()>,
        Player::Jumping,
        ChangeAxis::Vertical,
    )
}
