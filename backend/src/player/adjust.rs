use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{
    PlayerAction, PlayerActionKey, PlayerState, moving::Moving, on_auto_mob_use_key_action,
    use_key::UseKey,
};
use crate::{
    ActionKeyDirection, ActionKeyWith,
    context::Context,
    player::{
        ADJUSTING_MEDIUM_THRESHOLD, ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD,
        ADJUSTING_SHORT_THRESHOLD, LastMovement, MOVE_TIMEOUT, Player,
        actions::on_action_state,
        timeout::{ChangeAxis, Timeout, update_moving_axis_context},
        x_distance_direction, y_distance_direction,
    },
};

/// Updates the [`Player::Adjusting`] contextual state
///
/// This state just walks towards the destination. If [`Moving::exact`] is true,
/// then it will perform small movement to ensure the `x` is as close as possible.
pub fn update_adjusting_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: Moving,
) -> Player {
    const ADJUSTING_SHORT_TIMEOUT: u32 = 3;

    debug_assert!(moving.timeout.started || !moving.completed);
    let (x_distance, x_direction) = x_distance_direction(moving.dest, cur_pos);
    let (y_distance, y_direction) = y_distance_direction(moving.dest, cur_pos);
    let is_intermediate = moving.is_destination_intermediate();
    if x_distance >= state.double_jump_threshold(is_intermediate) {
        state.use_immediate_control_flow = true;
        return Player::Moving(moving.dest, moving.exact, moving.intermediates);
    }
    if !moving.timeout.started {
        if !matches!(state.last_movement, Some(LastMovement::Falling))
            && x_distance >= ADJUSTING_MEDIUM_THRESHOLD
            && y_direction < 0
            && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
            && !is_intermediate
        {
            return Player::Falling(moving.pos(cur_pos), cur_pos);
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
                |state, action| {
                    let dest = moving.last_destination();
                    let (x_distance, _) = x_distance_direction(dest, cur_pos);
                    let (y_distance, _) = y_distance_direction(dest, cur_pos);
                    on_player_action(context, state, action, x_distance, y_distance, moving)
                },
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
    x_distance: i32,
    y_distance: i32,
    moving: Moving,
) -> Option<(Player, bool)> {
    const USE_KEY_Y_THRESHOLD: i32 = 2;

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
