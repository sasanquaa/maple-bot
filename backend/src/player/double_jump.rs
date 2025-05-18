use std::{cell::RefCell, cmp::Ordering};

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
        moving::MOVE_TIMEOUT,
        state::LastMovement,
        timeout::{ChangeAxis, Timeout, update_moving_axis_context},
    },
};

/// Minimum x distance from the destination required to perform a double jump
pub const DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Minimum x distance from the destination required to perform a double jump in auto mobbing
pub const DOUBLE_JUMP_AUTO_MOB_THRESHOLD: i32 = 15;

/// Minimum x distance from the destination required to transition to [`Player::UseKey`]
const USE_KEY_X_THRESHOLD: i32 = DOUBLE_JUMP_THRESHOLD;

/// Minimum y distance from the destination required to transition to [`Player::UseKey`]
const USE_KEY_Y_THRESHOLD: i32 = 10;

// Note: even in auto mob, also use the non-auto mob threshold
const TIMEOUT: u32 = MOVE_TIMEOUT * 2;

/// Minimum x distance from the destination required to transition to [`Player::Grappling`]
const GRAPPLING_THRESHOLD: i32 = 4;

/// Minimum x distance changed to be considered as double jumped
const FORCE_THRESHOLD: i32 = 3;

/// Minimium y distance required to perform a fall and then double jump
const FALLING_THRESHOLD: i32 = 8;

#[derive(Copy, Clone, Debug)]
pub struct DoubleJumping {
    moving: Moving,
    /// Whether to force a double jump even when the player current position is already close to
    /// the destination
    pub forced: bool,
    /// Whether to wait for the player to become stationary before sending jump keys
    require_stationary: bool,
}

impl DoubleJumping {
    pub fn new(moving: Moving, forced: bool, require_stationary: bool) -> Self {
        Self {
            moving,
            forced,
            require_stationary,
        }
    }

    #[inline]
    fn moving(self, moving: Moving) -> DoubleJumping {
        DoubleJumping { moving, ..self }
    }
}

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
    double_jumping: DoubleJumping,
) -> Player {
    let moving = double_jumping.moving;
    let cur_pos = state.last_known_pos.unwrap();
    let ignore_grappling = double_jumping.forced || state.should_disable_grappling();
    let x_changed = (cur_pos.x - moving.pos.x).abs();
    let (x_distance, x_direction) = moving.x_distance_direction_from(true, cur_pos);
    let (y_distance, y_direction) = moving.y_distance_direction_from(true, cur_pos);
    let is_intermediate = moving.is_destination_intermediate();
    if !moving.timeout.started {
        // Checks to perform a fall and returns to double jump
        if !double_jumping.forced
            && !matches!(state.last_movement, Some(LastMovement::Falling))
            && y_direction < 0
            && y_distance >= FALLING_THRESHOLD
            && !is_intermediate
            && state.is_stationary
        {
            return Player::Falling(moving.pos(cur_pos), cur_pos, true);
        }
        if double_jumping.require_stationary && !state.is_stationary {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            return Player::DoubleJumping(double_jumping.moving(moving.pos(cur_pos)));
        }
        state.last_movement = Some(LastMovement::DoubleJumping);
    }

    let state = RefCell::new(state);
    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            let mut state = state.borrow_mut();
            // Mage teleportation requires a direction
            if !double_jumping.forced || state.config.teleport_key.is_some() {
                let key_direction = match x_direction.cmp(&0) {
                    Ordering::Greater => Some((KeyKind::Right, ActionKeyDirection::Right)),
                    Ordering::Less => Some((KeyKind::Left, ActionKeyDirection::Left)),
                    _ => None,
                };
                if let Some((key, direction)) = key_direction {
                    let _ = context.keys.send_down(key);
                    state.last_known_direction = direction;
                }
            }

            Player::DoubleJumping(double_jumping.moving(moving))
        },
        Some(|| {
            if let Some(key) = state.borrow().config.teleport_key {
                let _ = context.keys.send_up(key);
            }
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
        }),
        |mut moving| {
            let mut state = state.borrow_mut();
            let teleport_key = state.config.teleport_key;

            if !moving.completed {
                let can_continue = !double_jumping.forced
                    && x_distance >= state.double_jump_threshold(is_intermediate);
                let can_press = double_jumping.forced && x_changed <= FORCE_THRESHOLD;

                if can_continue || can_press {
                    if let Some(key) = teleport_key {
                        let _ = context.keys.send_down(key);
                    } else {
                        let _ = context.keys.send(state.config.jump_key);
                    }
                } else {
                    if let Some(key) = teleport_key {
                        let _ = context.keys.send_up(key);
                    }
                    let _ = context.keys.send_up(KeyKind::Right);
                    let _ = context.keys.send_up(KeyKind::Left);
                    moving = moving.completed(true);
                }
            }

            on_action(
                &mut state,
                |action| {
                    on_player_action(
                        context,
                        cur_pos,
                        double_jumping.forced,
                        action,
                        moving,
                        teleport_key,
                    )
                },
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
                        Player::DoubleJumping(double_jumping.moving(moving))
                    }
                },
            )
        },
        if double_jumping.forced {
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
    teleport_key: Option<KeyKind>,
) -> Option<(Player, bool)> {
    let (x_distance, _) = moving.x_distance_direction_from(false, cur_pos);
    let (y_distance, _) = moving.y_distance_direction_from(false, cur_pos);

    match action {
        // ignore proximity check when it is forced to double jumped
        // this indicates the player is already near the destination
        PlayerAction::AutoMob(_) => {
            let next =
                on_auto_mob_use_key_action(context, action, moving.pos, x_distance, y_distance);
            if next.is_some()
                && let Some(key) = teleport_key
            {
                let _ = context.keys.send_up(key);
            }
            next
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
