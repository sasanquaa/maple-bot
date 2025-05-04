use super::{Player, PlayerState, moving::Moving, state::LastMovement};
use crate::{
    context::Context,
    player::{
        MOVE_TIMEOUT,
        timeout::{ChangeAxis, update_moving_axis_context},
    },
};

/// Minimum y distance from the destination required to perform a grappling hook
pub const GRAPPLING_THRESHOLD: i32 = 26;

/// Maximum y distance from the destination required to perform a grappling hook
pub const GRAPPLING_MAX_THRESHOLD: i32 = 41;

const TIMEOUT: u32 = MOVE_TIMEOUT * 10;

const STOPPING_TIMEOUT: u32 = MOVE_TIMEOUT * 3;

const STOPPING_THRESHOLD: i32 = 5;

/// Updates the [`Player::Grappling`] contextual state
///
/// This state can only be transitioned via [`Player::Moving`] or [`Player::DoubleJumping`]
/// when the player has reached or close to the destination x-wise.
///
/// This state will use the Rope Lift skill.
pub fn update_grappling_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
) -> Player {
    if !moving.timeout.started {
        state.last_movement = Some(LastMovement::Grappling);
    }

    let cur_pos = state.last_known_pos.unwrap();
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
            let (distance, direction) = moving.y_distance_direction_from(true, moving.pos);
            if moving.timeout.current >= MOVE_TIMEOUT && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout_current(TIMEOUT).completed(true);
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
