use platforms::windows::KeyKind;

use super::{PlayerAction, PlayerActionKey, PlayerState, moving::Moving, use_key::UseKey};
use crate::{
    ActionKeyDirection, ActionKeyWith,
    context::Context,
    player::{
        Player,
        actions::{on_action_state, on_auto_mob_use_key_action},
        moving::{ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD, MOVE_TIMEOUT},
        state::LastMovement,
        timeout::{ChangeAxis, Timeout, update_moving_axis_context},
    },
};

/// Minimum x distance from the destination required to perform small movement
pub const ADJUSTING_SHORT_THRESHOLD: i32 = 1;

/// Minimum x distance from the destination required to walk
pub const ADJUSTING_MEDIUM_THRESHOLD: i32 = 3;

const ADJUSTING_SHORT_TIMEOUT: u32 = 3;

/// Updates the [`Player::Adjusting`] contextual state
///
/// This state just walks towards the destination. If [`Moving::exact`] is true,
/// then it will perform small movement to ensure the `x` is as close as possible.
pub fn update_adjusting_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
) -> Player {
    debug_assert!(moving.timeout.started || !moving.completed);
    let cur_pos = state.last_known_pos.unwrap();
    let (x_distance, x_direction) = moving.x_distance_direction_from(true, cur_pos);
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
    let is_intermediate = moving.is_destination_intermediate();
    if x_distance >= state.double_jump_threshold(is_intermediate) {
        state.use_immediate_control_flow = true;
        return Player::Moving(moving.dest, moving.exact, moving.intermediates);
    }
    if !moving.timeout.started {
        // Checks to perform a fall and returns to walk
        if !matches!(state.last_movement, Some(LastMovement::Falling))
            && x_distance >= ADJUSTING_MEDIUM_THRESHOLD
            && y_direction < 0
            && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
            && !is_intermediate
            && state.is_stationary
        {
            return Player::Falling(moving.pos(cur_pos), cur_pos, false);
        }
        state.last_movement = Some(LastMovement::Adjusting);
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        MOVE_TIMEOUT,
        Player::Adjusting,
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
        }),
        |mut moving| {
            if !moving.completed {
                match (x_distance, x_direction) {
                    (x, d) if x >= ADJUSTING_MEDIUM_THRESHOLD && d > 0 => {
                        let _ = context.keys.send_up(KeyKind::Left);
                        let _ = context.keys.send_down(KeyKind::Right);
                        state.last_known_direction = ActionKeyDirection::Right;
                    }
                    (x, d) if x >= ADJUSTING_MEDIUM_THRESHOLD && d < 0 => {
                        let _ = context.keys.send_up(KeyKind::Right);
                        let _ = context.keys.send_down(KeyKind::Left);
                        state.last_known_direction = ActionKeyDirection::Left;
                    }
                    (x, d) if moving.exact && x >= ADJUSTING_SHORT_THRESHOLD && d > 0 => {
                        let _ = context.keys.send_up(KeyKind::Left);
                        let _ = context.keys.send_down(KeyKind::Right);
                        if moving.timeout.current >= ADJUSTING_SHORT_TIMEOUT {
                            let _ = context.keys.send_up(KeyKind::Right);
                        }
                        state.last_known_direction = ActionKeyDirection::Right;
                    }
                    (x, d) if moving.exact && x >= ADJUSTING_SHORT_THRESHOLD && d < 0 => {
                        let _ = context.keys.send_up(KeyKind::Right);
                        let _ = context.keys.send_down(KeyKind::Left);
                        if moving.timeout.current >= ADJUSTING_SHORT_TIMEOUT {
                            let _ = context.keys.send_up(KeyKind::Left);
                        }
                        state.last_known_direction = ActionKeyDirection::Left;
                    }
                    _ => {
                        let _ = context.keys.send_up(KeyKind::Right);
                        let _ = context.keys.send_up(KeyKind::Left);
                        moving = moving.completed(true);
                    }
                }
            }

            on_action_state(
                state,
                |state, action| on_player_action(context, state, action, moving),
                || {
                    if !moving.completed {
                        Player::Adjusting(moving)
                    } else {
                        Player::Adjusting(moving.timeout_current(MOVE_TIMEOUT))
                    }
                },
            )
        },
        ChangeAxis::Both,
    )
}

fn on_player_action(
    context: &Context,
    state: &PlayerState,
    action: PlayerAction,
    moving: Moving,
) -> Option<(Player, bool)> {
    const USE_KEY_Y_THRESHOLD: i32 = 2;

    let cur_pos = state.last_known_pos.unwrap();
    let (x_distance, _) = moving.x_distance_direction_from(false, cur_pos);
    let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);

    match action {
        PlayerAction::Key(PlayerActionKey {
            with: ActionKeyWith::DoubleJump,
            direction,
            ..
        }) => {
            if !moving.completed || y_distance > 0 {
                return None;
            }
            if matches!(direction, ActionKeyDirection::Any)
                || direction == state.last_known_direction
            {
                Some((
                    Player::DoubleJumping(
                        moving.timeout(Timeout::default()).completed(false),
                        true,
                        false,
                    ),
                    false,
                ))
            } else {
                Some((Player::UseKey(UseKey::from_action(action)), false))
            }
        }
        PlayerAction::Key(PlayerActionKey {
            with: ActionKeyWith::Any,
            ..
        }) => {
            if moving.completed && y_distance <= USE_KEY_Y_THRESHOLD {
                Some((Player::UseKey(UseKey::from_action(action)), false))
            } else {
                None
            }
        }
        PlayerAction::AutoMob(_) => {
            on_auto_mob_use_key_action(context, action, moving.pos, x_distance, y_distance)
        }
        PlayerAction::Key(PlayerActionKey {
            with: ActionKeyWith::Stationary,
            ..
        })
        | PlayerAction::SolveRune
        | PlayerAction::Move(_) => None,
    }
}
