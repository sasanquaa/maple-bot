use log::debug;
use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{
    Player, PlayerAction, PlayerActionKey, PlayerState, actions::on_auto_mob_use_key_action,
    moving::Moving, use_key::UseKey,
};
use crate::{
    ActionKeyDirection, ActionKeyWith,
    context::Context,
    player::{
        actions::on_action,
        moving::{ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD, MOVE_TIMEOUT},
        state::LastMovement,
        timeout::{ChangeAxis, Timeout, update_moving_axis_context},
    },
};

/// Minimum x distance from the destination required to perform a double jump
pub const DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Minimum x distance from the destination required to perform a double jump in auto mobbing
pub const DOUBLE_JUMP_AUTO_MOB_THRESHOLD: i32 = 15;

const USE_KEY_X_THRESHOLD: i32 = DOUBLE_JUMP_THRESHOLD;

const USE_KEY_Y_THRESHOLD: i32 = 10;
// Note: even in auto mob, also use the non-auto mob threshold
const TIMEOUT: u32 = MOVE_TIMEOUT * 2;

const GRAPPLING_THRESHOLD: i32 = 4;

const FORCE_THRESHOLD: i32 = 3;

/// Updates the [`Player::DoubleJumping`] contextual state
///
/// This state continues to double jump as long as the distance x-wise is still
/// `>= DOUBLE_JUMP_THRESHOLD`. Or when `forced`, this state will attempt a single double jump.
/// When `require_stationary`, this state will wait for the player to be stationary before
/// double jumping.
///
/// `forced` is currently true when it is transitioned from [`Player::Idle`], [`Player::Moving`],
/// [`Player::Adjusting`], and [`Player::UseKey`] with [`PlayerState::last_known_direction`]
/// matches the [`PlayerAction::Key`] direction.
///
/// `require_stationary` is currently true when it is transitioned from [`Player::Idle`] and
/// [`Player::UseKey`] with [`PlayerState::last_known_direction`] matches the
/// [`PlayerAction::Key`] direction.
pub fn update_double_jumping_context(
    context: &Context,
    state: &mut PlayerState,
    moving: Moving,
    forced: bool,
    require_stationary: bool,
) -> Player {
    debug_assert!(moving.timeout.started || !moving.completed);
    let cur_pos = state.last_known_pos.unwrap();
    let ignore_grappling = forced || state.should_disable_grappling();
    let x_changed = (cur_pos.x - moving.pos.x).abs();
    let (x_distance, x_direction) = moving.x_distance_direction_from(true, cur_pos);
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
    let is_intermediate = moving.is_destination_intermediate();
    if !moving.timeout.started {
        // Checks to perform a fall and returns to double jump
        if !forced
            && !matches!(state.last_movement, Some(LastMovement::Falling))
            && y_direction < 0
            && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
            && !is_intermediate
        {
            return Player::Falling(moving.pos(cur_pos), cur_pos);
        }
        if require_stationary && !state.is_stationary {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            return Player::DoubleJumping(moving.pos(cur_pos), forced, require_stationary);
        }
        state.last_movement = Some(LastMovement::DoubleJumping);
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| Player::DoubleJumping(moving, forced, require_stationary),
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
        }),
        |mut moving| {
            if !moving.completed {
                // mage teleportation requires a direction
                if !forced || state.config.teleport_key.is_some() {
                    match x_direction {
                        d if d > 0 => {
                            let _ = context.keys.send_up(KeyKind::Left);
                            let _ = context.keys.send_down(KeyKind::Right);
                            state.last_known_direction = ActionKeyDirection::Right;
                        }
                        d if d < 0 => {
                            let _ = context.keys.send_up(KeyKind::Right);
                            let _ = context.keys.send_down(KeyKind::Left);
                            state.last_known_direction = ActionKeyDirection::Left;
                        }
                        _ => {
                            if state.config.teleport_key.is_some() {
                                match state.last_known_direction {
                                    ActionKeyDirection::Any => (),
                                    ActionKeyDirection::Left => {
                                        let _ = context.keys.send_up(KeyKind::Right);
                                        let _ = context.keys.send_down(KeyKind::Left);
                                    }
                                    ActionKeyDirection::Right => {
                                        let _ = context.keys.send_up(KeyKind::Left);
                                        let _ = context.keys.send_down(KeyKind::Right);
                                    }
                                }
                            }
                        }
                    }
                }
                if (!forced && x_distance >= state.double_jump_threshold(is_intermediate))
                    || (forced && x_changed <= FORCE_THRESHOLD)
                {
                    let _ = context
                        .keys
                        .send(state.config.teleport_key.unwrap_or(state.config.jump_key));
                } else {
                    let _ = context.keys.send_up(KeyKind::Right);
                    let _ = context.keys.send_up(KeyKind::Left);
                    moving = moving.completed(true);
                }
            }
            on_action(
                state,
                |action| on_player_action(context, cur_pos, forced, action, moving),
                || {
                    if !ignore_grappling
                        && moving.completed
                        && x_distance <= GRAPPLING_THRESHOLD
                        && y_direction > 0
                    {
                        debug!(target: "player", "performs grappling on double jump");
                        Player::Grappling(moving.completed(false).timeout(Timeout::default()))
                    } else if moving.completed && moving.timeout.current >= MOVE_TIMEOUT {
                        Player::Moving(moving.dest, moving.exact, moving.intermediates)
                    } else {
                        Player::DoubleJumping(moving, forced, require_stationary)
                    }
                },
            )
        },
        if forced {
            // this ensures it won't double jump forever when
            // jumping towards either edge of the map
            ChangeAxis::Horizontal
        } else {
            ChangeAxis::Both
        },
    )
}

/// Handles [`PlayerAction`] during double jump
///
/// It currently handles action for auto mob and a key action with [`ActionKeyWith::Any`] or
/// [`ActionKeyWith::DoubleJump`]. For auto mob, the same handling logics is reused. For the other,
/// it will try to transition to [`Player::UseKey`] when the player is close enough.
fn on_player_action(
    context: &Context,
    cur_pos: Point,
    forced: bool,
    action: PlayerAction,
    moving: Moving,
) -> Option<(Player, bool)> {
    let (x_distance, _) = moving.x_distance_direction_from(false, cur_pos);
    let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);

    match action {
        // ignore proximity check when it is forced to double jumped
        // this indicates the player is already near the destination
        PlayerAction::AutoMob(_) => {
            on_auto_mob_use_key_action(context, action, moving.pos, x_distance, y_distance)
        }
        PlayerAction::Key(PlayerActionKey {
            with: ActionKeyWith::DoubleJump | ActionKeyWith::Any,
            ..
        }) => {
            if !moving.completed {
                return None;
            }
            if forced
                || (!moving.exact
                    && x_distance <= USE_KEY_X_THRESHOLD
                    && y_distance <= USE_KEY_Y_THRESHOLD)
            {
                Some((Player::UseKey(UseKey::from_action(action)), false))
            } else {
                None
            }
        }
        PlayerAction::Key(PlayerActionKey {
            with: ActionKeyWith::Stationary,
            ..
        })
        | PlayerAction::SolveRune
        | PlayerAction::Move { .. } => None,
    }
}
