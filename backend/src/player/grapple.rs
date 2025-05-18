use super::{
    Player, PlayerAction, PlayerState,
    actions::{on_action, on_auto_mob_use_key_action},
    moving::Moving,
    state::LastMovement,
};
use crate::{
    context::Context,
    player::{
        MOVE_TIMEOUT,
        timeout::{ChangeAxis, update_moving_axis_context},
    },
};

/// Minimum y distance from the destination required to perform a grappling hook
pub const GRAPPLING_THRESHOLD: i32 = 24;

/// Maximum y distance from the destination required to perform a grappling hook
pub const GRAPPLING_MAX_THRESHOLD: i32 = 41;

const TIMEOUT: u32 = MOVE_TIMEOUT * 10;

const STOPPING_TIMEOUT: u32 = MOVE_TIMEOUT * 2;

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
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, moving.pos);

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
            if moving.timeout.current >= MOVE_TIMEOUT && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout_current(TIMEOUT).completed(true);
            }
            if !moving.completed {
                if y_direction <= 0 || y_distance <= STOPPING_THRESHOLD {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                }
            } else if moving.timeout.current >= STOPPING_TIMEOUT {
                moving = moving.timeout_current(TIMEOUT);
            }

            on_action(
                state,
                |action| match action {
                    PlayerAction::AutoMob(_) => {
                        if moving.completed && moving.is_destination_intermediate() {
                            return Some((
                                Player::Moving(moving.dest, moving.exact, moving.intermediates),
                                false,
                            ));
                        }
                        let (x_distance, _) = moving.x_distance_direction_from(false, cur_pos);
                        let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);
                        on_auto_mob_use_key_action(context, action, cur_pos, x_distance, y_distance)
                    }
                    PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune => None,
                },
                || Player::Grappling(moving),
            )
        },
        ChangeAxis::Vertical,
    )
}
