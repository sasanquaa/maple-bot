use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{Player, PlayerState, moving::Moving};
use crate::{
    context::Context,
    player::{
        MOVE_TIMEOUT, PlayerAction,
        actions::{on_action, on_auto_mob_use_key_action},
        adjust::ADJUSTING_MEDIUM_THRESHOLD,
        state::LastMovement,
        timeout::{ChangeAxis, update_moving_axis_context},
    },
};

/// Minimum y distance from the destination required to perform a fall
pub const FALLING_THRESHOLD: i32 = 4;

/// Updates the [`Player::Falling`] contextual state
///
/// This state will perform a drop down `Down Key + Space`
pub fn update_falling_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
    anchor: Point,
) -> Player {
    const STOP_DOWN_KEY_TICK: u32 = 3;
    const TIMEOUT: u32 = MOVE_TIMEOUT * 2;

    let cur_pos = state.last_known_pos.unwrap();
    let y_changed = cur_pos.y - anchor.y;
    let (x_distance, _) = moving.x_distance_direction_from(true, cur_pos);
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
                        let (x_distance, _) = moving.x_distance_direction_from(false, cur_pos);
                        let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);
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
        update_falling_context(&context, &mut state, moving, Point::default());
        let _ = context.keys;

        // Don't send keys if not stationary
        let mut keys = MockKeySender::new();
        keys.expect_send_down().never();
        keys.expect_send().never();
        let context = Context::new(Some(keys), None);
        state.is_stationary = false;
        update_falling_context(&context, &mut state, moving, Point::default());
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
        let mut state = PlayerState {
            is_stationary: true,
            last_known_pos: Some(pos),
            ..Default::default()
        };
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
            update_falling_context(&context, &mut state, moving, anchor),
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
                _
            )
        );
    }
}
