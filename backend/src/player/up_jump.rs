use log::debug;
use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{Player, PlayerState, moving::Moving};
use crate::{
    context::Context,
    minimap::Minimap,
    player::{
        ADJUSTING_MEDIUM_THRESHOLD, LastMovement, MOVE_TIMEOUT, PlayerAction,
        actions::on_action,
        on_auto_mob_use_key_action,
        timeout::{ChangeAxis, update_moving_axis_context},
        x_distance_direction, y_distance_direction,
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
    cur_pos: Point,
    moving: Moving,
) -> Player {
    const SPAM_DELAY: u32 = 7;
    const STOP_UP_KEY_TICK: u32 = 3;
    const TIMEOUT: u32 = MOVE_TIMEOUT * 2;
    const UP_JUMPED_THRESHOLD: i32 = 5;

    if !moving.timeout.started {
        if let Minimap::Idle(idle) = context.minimap {
            for portal in idle.portals {
                if portal.x <= cur_pos.x
                    && cur_pos.x < portal.x + portal.width
                    && portal.y >= cur_pos.y
                    && portal.y - portal.height < cur_pos.y
                {
                    debug!(target: "player", "abort action due to potential map moving");
                    state.clear_action_and_movement();
                    return Player::Idle;
                }
            }
        }
        state.last_movement = Some(LastMovement::UpJumping);
    }

    let y_changed = (cur_pos.y - moving.pos.y).abs();
    let (x_distance, _) = x_distance_direction(moving.dest, cur_pos);
    let up_jump_key = state.config.upjump_key;
    let jump_key = state.config.jump_key;
    let has_teleport_key = state.config.teleport_key.is_some();
    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            let _ = context.keys.send_down(KeyKind::Up);
            if up_jump_key.is_none() || (up_jump_key.is_some() && has_teleport_key) {
                let _ = context.keys.send(jump_key);
            }
            Player::UpJumping(moving)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Up);
        }),
        |mut moving| {
            match (moving.completed, up_jump_key) {
                (false, Some(key)) => {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                }
                (false, None) => {
                    if y_changed <= UP_JUMPED_THRESHOLD {
                        // spamming space until the player y changes
                        // above a threshold as sending space twice
                        // doesn't work
                        if moving.timeout.total >= SPAM_DELAY {
                            let _ = context.keys.send(jump_key);
                        }
                    } else {
                        moving = moving.completed(true);
                    }
                }
                (true, _) => {
                    // this is when up jump like blaster or mage still requires up key
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
                        let dest = moving.last_destination();
                        let (x_distance, _) = x_distance_direction(dest, cur_pos);
                        let (y_distance, _) = y_distance_direction(dest, cur_pos);
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
