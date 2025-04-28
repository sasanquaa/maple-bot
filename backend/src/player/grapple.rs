use opencv::core::Point;

use super::{Player, PlayerState, moving::Moving};
use crate::{
    context::Context,
    player::{
        LastMovement, MOVE_TIMEOUT,
        timeout::{ChangeAxis, update_moving_axis_context},
        y_distance_direction,
    },
};

/// Updates the [`Player::Grappling`] contextual state
///
/// This state can only be transitioned via [`Player::Moving`] or [`Player::DoubleJumping`]
/// when the player has reached or close to the destination x-wise.
///
/// This state will use the Rope Lift skill.
pub fn update_grappling_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: Moving,
) -> Player {
    const TIMEOUT: u32 = MOVE_TIMEOUT * 10;
    const STOPPING_TIMEOUT: u32 = MOVE_TIMEOUT * 3;
    const STOPPING_THRESHOLD: i32 = 3;

    if !moving.timeout.started {
        state.last_movement = Some(LastMovement::Grappling);
    }

    let key = state.config.grappling_key;
    let x_changed = cur_pos.x != moving.pos.x;

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            let _ = context.keys.send(key);
            Player::Grappling(moving)
        },
        None::<fn()>,
        |mut moving| {
            let (distance, direction) = y_distance_direction(moving.dest, moving.pos);
            if moving.timeout.current >= MOVE_TIMEOUT && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout_current(TIMEOUT);
            }
            if !moving.completed {
                if direction <= 0 || distance <= STOPPING_THRESHOLD {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                }
            } else if state.has_auto_mob_action_only() || moving.timeout.current >= STOPPING_TIMEOUT
            {
                moving = moving.timeout_current(TIMEOUT);
            }
            Player::Grappling(moving)
        },
        ChangeAxis::Vertical,
    )
}
