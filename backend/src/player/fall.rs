use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{Player, PlayerState, moving::Moving};
use crate::{
    context::Context,
    player::{
        MOVE_TIMEOUT, PlayerAction,
        actions::{on_action, on_auto_mob_use_key_action},
        state::LastMovement,
        timeout::{ChangeAxis, update_moving_axis_context},
    },
};

/// Minimum y distance from the destination required to perform a fall
pub const FALLING_THRESHOLD: i32 = 4;

/// The tick to stop helding down [`KeyKind::Down`] at
const STOP_DOWN_KEY_TICK: u32 = 3;

const TIMEOUT: u32 = MOVE_TIMEOUT * 2;

const TELEPORT_FALL_THRESHOLD: i32 = 14;

/// Updates the [`Player::Falling`] contextual state
///
/// This state will perform a drop down action. It is completed as soon as the player current `y`
/// position is below `anchor`. If `timeout_on_complete` is provided, it will timeout when the
/// action is complete and return to [`Player::Moving`]. Timing out early is currently used by
/// [`Player::DoubleJumping`] to perform a composite action `drop down and then double jump`.
pub fn update_falling_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
    anchor: Point,
    timeout_on_complete: bool,
) -> Player {
    let cur_pos = state.last_known_pos.unwrap();
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
    if !moving.timeout.started {
        // Wait until stationary before doing a fall
        if !state.is_stationary {
            return Player::Falling(moving.pos(cur_pos), cur_pos, timeout_on_complete);
        }
        if y_direction >= 0 {
            return Player::Moving(moving.dest, moving.exact, moving.intermediates);
        }
        state.last_movement = Some(LastMovement::Falling);
    }

    let y_changed = cur_pos.y - anchor.y;
    let jump_key = state.config.jump_key;
    let teleport_key = state.config.teleport_key;

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            let _ = context.keys.send_down(KeyKind::Down);
            if let Some(key) = teleport_key
                && y_distance <= TELEPORT_FALL_THRESHOLD
            {
                let _ = context.keys.send(key);
            } else {
                let _ = context.keys.send(jump_key);
            }
            Player::Falling(moving, anchor, timeout_on_complete)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Down);
        }),
        |mut moving| {
            if moving.timeout.total == STOP_DOWN_KEY_TICK {
                let _ = context.keys.send_up(KeyKind::Down);
            }
            if !moving.completed && y_changed < 0 {
                moving = moving.completed(true);
            } else if moving.completed && timeout_on_complete {
                moving = moving.timeout_current(TIMEOUT);
            }

            on_action(
                state,
                |action| match action {
                    PlayerAction::AutoMob(_) => {
                        // Ignore `timeout_on_complete` for auto-mobbing intermediate destination
                        if moving.completed
                            && moving.is_destination_intermediate()
                            && y_direction >= 0
                        {
                            let _ = context.keys.send_up(KeyKind::Down);
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
                || Player::Falling(moving, anchor, timeout_on_complete),
            )
        },
        ChangeAxis::Vertical,
    )
}

#[cfg(test)]
mod tests {
    use std::assert_matches::assert_matches;

    use opencv::core::Point;
    use platforms::windows::KeyKind;

    use super::update_falling_context;
    use crate::{
        bridge::MockKeySender,
        context::Context,
        player::{Player, PlayerState, moving::Moving, timeout::Timeout},
    };

    #[test]
    fn falling_start() {
        let mut state = PlayerState::default();
        state.config.jump_key = KeyKind::Space;
        let mut keys = MockKeySender::new();
        keys.expect_send_down()
            .withf(|key| matches!(key, KeyKind::Down))
            .returning(|_| Ok(()));
        keys.expect_send()
            .withf(|key| matches!(key, KeyKind::Space))
            .returning(|_| Ok(()));
        let context = Context::new(Some(keys), None);
        let pos = Point::new(5, 5);
        let moving = Moving {
            pos,
            dest: pos,
            ..Default::default()
        };
        // Send keys if stationary
        state.is_stationary = true;
        state.last_known_pos = Some(pos);
        update_falling_context(&context, &mut state, moving, Point::default(), false);
        let _ = context.keys;

        // Don't send keys if not stationary
        let mut keys = MockKeySender::new();
        keys.expect_send_down().never();
        keys.expect_send().never();
        let context = Context::new(Some(keys), None);
        state.is_stationary = false;
        update_falling_context(&context, &mut state, moving, Point::default(), false);
    }

    #[test]
    fn falling_update() {
        let mut keys = MockKeySender::new();
        keys.expect_send_up()
            .withf(|key| matches!(key, KeyKind::Down))
            .once()
            .returning(|_| Ok(()));
        let context = Context::new(Some(keys), None);
        let pos = Point::new(5, 5);
        let anchor = Point::new(6, 6);
        let dest = Point::new(2, 2);
        let mut state = PlayerState::default();
        state.last_known_pos = Some(pos);
        state.is_stationary = true;
        let moving = Moving {
            pos,
            dest,
            timeout: Timeout {
                started: true,
                total: 2,
                ..Default::default()
            },
            ..Default::default()
        };

        // Send up key because total = 2 and timeout early
        assert_matches!(
            update_falling_context(&context, &mut state, moving, anchor, false),
            Player::Falling(
                Moving {
                    completed: true,
                    timeout: Timeout {
                        current: 1,
                        total: 3,
                        ..
                    },
                    ..
                },
                _,
                _
            )
        );
    }
}
