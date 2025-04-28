use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{Player, PlayerState, moving::Moving};
use crate::{
    context::Context,
    player::{
        ADJUSTING_MEDIUM_THRESHOLD, LastMovement, MOVE_TIMEOUT, PlayerAction,
        actions::on_action,
        on_auto_mob_use_key_action,
        timeout::{ChangeAxis, update_moving_axis_context},
        x_distance_direction, y_distance_direction,
    },
};

/// Updates the [`Player::Falling`] contextual state
///
/// This state will perform a drop down `Down Key + Space`
pub fn update_falling_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: Moving,
    anchor: Point,
) -> Player {
    const STOP_DOWN_KEY_TICK: u32 = 3;
    const TIMEOUT: u32 = MOVE_TIMEOUT * 2;

    let y_changed = cur_pos.y - anchor.y;
    let (x_distance, _) = x_distance_direction(moving.dest, cur_pos);
    let is_stationary = state.is_stationary;
    let jump_key = state.config.jump_key;
    if !moving.timeout.started {
        state.last_movement = Some(LastMovement::Falling);
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            if is_stationary {
                let _ = context.keys.send_down(KeyKind::Down);
                let _ = context.keys.send(jump_key);
            }
            Player::Falling(moving, anchor)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Down);
        }),
        |mut moving| {
            if moving.timeout.total == STOP_DOWN_KEY_TICK {
                let _ = context.keys.send_up(KeyKind::Down);
            }
            if !moving.completed {
                if y_changed < 0 {
                    moving = moving.completed(true);
                }
            } else if x_distance >= ADJUSTING_MEDIUM_THRESHOLD {
                moving = moving.timeout_current(TIMEOUT);
            }
            on_action(
                state,
                |action| match action {
                    PlayerAction::AutoMob(_) => {
                        if moving.completed && moving.is_destination_intermediate() {
                            let _ = context.keys.send_up(KeyKind::Down);
                            return Some((
                                Player::Moving(moving.dest, moving.exact, moving.intermediates),
                                false,
                            ));
                        }
                        let dest = moving.last_destination();
                        let (x_distance, _) = x_distance_direction(dest, cur_pos);
                        let (y_distance, _) = y_distance_direction(dest, cur_pos);
                        on_auto_mob_use_key_action(context, action, cur_pos, x_distance, y_distance)
                    }
                    PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune => None,
                },
                || Player::Falling(moving, anchor),
            )
        },
        ChangeAxis::Vertical,
    )
}
