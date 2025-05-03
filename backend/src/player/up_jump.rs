use log::debug;
use platforms::windows::KeyKind;

use super::{Player, PlayerState, moving::Moving};
use crate::{
    context::Context,
    minimap::Minimap,
    player::{
        MOVE_TIMEOUT, PlayerAction,
        actions::{on_action, on_auto_mob_use_key_action},
        adjust::ADJUSTING_MEDIUM_THRESHOLD,
        state::LastMovement,
        timeout::{ChangeAxis, update_moving_axis_context},
    },
};

/// Updates the [`Player::UpJumping`] contextual state
///
/// This state can only be transitioned via [`Player::Moving`] when the
/// player has reached the destination x-wise.
///
/// This state will:
/// - Abort the action if the player is near a portal
/// - Perform an up jump
pub fn update_up_jumping_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
) -> Player {
    const SPAM_DELAY: u32 = 7;
    const STOP_UP_KEY_TICK: u32 = 3;
    const TIMEOUT: u32 = MOVE_TIMEOUT * 2;
    const UP_JUMPED_THRESHOLD: i32 = 5;

    let cur_pos = state.last_known_pos.unwrap();
    if !moving.timeout.started {
        if let Minimap::Idle(idle) = context.minimap {
            for portal in idle.portals {
                if portal.x <= cur_pos.x
                    && cur_pos.x < portal.x + portal.width
                    && portal.y >= cur_pos.y
                    && portal.y - portal.height < cur_pos.y
                {
                    debug!(target: "player", "abort action due to potential map moving");
                    state.mark_action_completed();
                    return Player::Idle;
                }
            }
        }
        state.last_movement = Some(LastMovement::UpJumping);
    }

    let y_changed = (cur_pos.y - moving.pos.y).abs();
    let (x_distance, _) = moving.x_distance_direction_from(true, cur_pos);
    let up_jump_key = state.config.upjump_key;
    let jump_key = state.config.jump_key;
    let has_teleport_key = state.config.teleport_key.is_some();

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            // Only send Up key when the key is not of a Demon Slayer
            if !matches!(up_jump_key, Some(KeyKind::Up)) {
                let _ = context.keys.send_down(KeyKind::Up);
            }
            match (up_jump_key, has_teleport_key) {
                // This is a generic class, a mage or a Demon Slayer
                (None, _) | (Some(_), true) | (Some(KeyKind::Up), false) => {
                    let _ = context.keys.send(jump_key);
                }
                _ => (),
            }
            Player::UpJumping(moving)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Up);
        }),
        |mut moving| {
            match (moving.completed, up_jump_key, has_teleport_key) {
                (false, None, true) | (false, Some(KeyKind::Up), false) | (false, None, false) => {
                    if y_changed <= UP_JUMPED_THRESHOLD {
                        // Spam jump key until the player y changes
                        // above a threshold as sending jump key twice
                        // doesn't work
                        if moving.timeout.total >= SPAM_DELAY {
                            // This up jump key is Up for Demon Slayer
                            if let Some(key) = up_jump_key {
                                let _ = context.keys.send(key);
                            } else {
                                let _ = context.keys.send(jump_key);
                            }
                        }
                    } else {
                        moving = moving.completed(true);
                    }
                }
                (false, Some(key), _) => {
                    if !has_teleport_key || moving.timeout.total >= SPAM_DELAY {
                        let _ = context.keys.send(key);
                        moving = moving.completed(true);
                    }
                }
                (true, _, _) => {
                    // This is when up jump like Blaster or mage still requires up key
                    // cancel early to avoid stucking to a rope
                    if up_jump_key.is_some() && moving.timeout.total == STOP_UP_KEY_TICK {
                        let _ = context.keys.send_up(KeyKind::Up);
                    }
                    if x_distance >= ADJUSTING_MEDIUM_THRESHOLD
                        && moving.timeout.current >= MOVE_TIMEOUT
                    {
                        moving = moving.timeout_current(TIMEOUT);
                    }
                }
            }
            on_action(
                state,
                |action| match action {
                    PlayerAction::AutoMob(_) => {
                        if moving.completed
                            && moving.is_destination_intermediate()
                            && cur_pos.y >= moving.dest.y
                        {
                            let _ = context.keys.send_up(KeyKind::Up);
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
                || Player::UpJumping(moving),
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

    use super::{Moving, PlayerState, update_up_jumping_context};
    use crate::{
        bridge::MockKeySender,
        context::Context,
        player::{Player, Timeout},
    };

    #[test]
    fn up_jumping_start() {
        let pos = Point::new(5, 5);
        let moving = Moving {
            pos,
            dest: pos,
            ..Default::default()
        };
        let mut state = PlayerState::default();
        let mut context = Context::new(None, None);
        state.config.jump_key = KeyKind::Space;
        state.last_known_pos = Some(pos);

        let mut keys = MockKeySender::new();
        keys.expect_send_down()
            .withf(|key| matches!(key, KeyKind::Up))
            .returning(|_| Ok(()))
            .once();
        keys.expect_send()
            .withf(|key| matches!(key, KeyKind::Space))
            .returning(|_| Ok(()))
            .once();
        context.keys = Box::new(keys);
        // Space + Up only
        update_up_jumping_context(&context, &mut state, moving);
        let _ = context.keys; // drop mock for validation

        state.config.upjump_key = Some(KeyKind::C);
        let mut keys = MockKeySender::new();
        keys.expect_send_down()
            .withf(|key| matches!(key, KeyKind::Up))
            .once()
            .returning(|_| Ok(()));
        keys.expect_send()
            .withf(|key| matches!(key, KeyKind::Space))
            .never()
            .returning(|_| Ok(()));
        context.keys = Box::new(keys);
        // Up only
        update_up_jumping_context(&context, &mut state, moving);
        let _ = context.keys; // drop mock for validation

        state.config.teleport_key = Some(KeyKind::Shift);
        let mut keys = MockKeySender::new();
        keys.expect_send_down()
            .withf(|key| matches!(key, KeyKind::Up))
            .once()
            .returning(|_| Ok(()));
        keys.expect_send()
            .withf(|key| matches!(key, KeyKind::Space))
            .once()
            .returning(|_| Ok(()));
        context.keys = Box::new(keys);
        // Space + Up
        update_up_jumping_context(&context, &mut state, moving);
        let _ = context.keys; // drop mock for validation
    }

    #[test]
    fn up_jumping_update() {
        let moving_pos = Point::new(7, 1);
        let moving = Moving {
            pos: moving_pos,
            timeout: Timeout {
                started: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut state = PlayerState {
            last_known_pos: Some(Point::new(7, 7)),
            ..Default::default()
        };
        let context = Context::new(None, None);

        // up jumped because y changed > 5
        assert_matches!(
            update_up_jumping_context(&context, &mut state, moving),
            Player::UpJumping(Moving {
                timeout: Timeout {
                    current: 1,
                    total: 1,
                    ..
                },
                completed: true,
                ..
            })
        );

        // TODO
        // more tests
    }
}
