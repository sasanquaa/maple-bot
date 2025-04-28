use actions::{on_action, on_action_state_mut};
use adjust::update_adjusting_context;
use cash_shop::{CashShop, update_cash_shop_context};
use double_jump::update_double_jumping_context;
use fall::update_falling_context;
use grapple::update_grappling_context;
use idle::update_idle_context;
use log::debug;
use moving::{Moving, MovingIntermediates, update_moving_context};
use opencv::core::Point;
use platforms::windows::KeyKind;
use solve_rune::{SolvingRune, update_solving_rune_context};
use stall::update_stalling_context;
use strum::Display;
use timeout::{ChangeAxis, Timeout, update_moving_axis_context, update_with_timeout};
use unstuck::update_unstucking_context;
use up_jump::update_up_jumping_context;
use use_key::{UseKey, update_use_key_context};

use crate::{
    array::Array,
    buff::{Buff, BuffKind},
    context::{Context, Contextual, ControlFlow},
    database::ActionKeyDirection,
    minimap::Minimap,
    network::NotificationKind,
    pathing::{PlatformWithNeighbors, find_points_with},
    task::{Update, update_detection_task},
};

mod actions;
mod adjust;
mod cash_shop;
mod double_jump;
mod fall;
mod grapple;
mod idle;
mod moving;
mod solve_rune;
mod stall;
mod state;
mod timeout;
mod unstuck;
mod up_jump;
mod use_key;

pub use {
    actions::PlayerAction, actions::PlayerActionAutoMob, actions::PlayerActionKey,
    actions::PlayerActionMove, state::PlayerState,
};

/// Maximum number of times [`Player::Moving`] state can be transitioned to
/// without changing position
const UNSTUCK_TRACKER_THRESHOLD: u32 = 7;

/// Minimium y distance required to perform a fall and double jump/adjusting
const ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD: i32 = 8;

/// Minimum x distance from the destination required to perform small movement
const ADJUSTING_SHORT_THRESHOLD: i32 = 1;

/// Minimum x distance from the destination required to walk
const ADJUSTING_MEDIUM_THRESHOLD: i32 = 3;

/// Minimum x distance from the destination required to perform a double jump
pub const DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Minimum x distance from the destination required to perform a double jump in auto mobbing
const DOUBLE_JUMP_AUTO_MOB_THRESHOLD: i32 = 15;

/// Maximum amount of ticks a change in x or y direction must be detected
const MOVE_TIMEOUT: u32 = 5;

/// Minimum y distance from the destination required to perform a fall
const FALLING_THRESHOLD: i32 = 4;

/// Minimum y distance from the destination required to perform a jump
pub const JUMP_THRESHOLD: i32 = 7;

/// Minimum y distance from the destination required to perform a grappling hook
pub const GRAPPLING_THRESHOLD: i32 = 26;

/// Maximum y distance from the destination required to perform a grappling hook
pub const GRAPPLING_MAX_THRESHOLD: i32 = 41;

/// The number of times a reachable y must successfuly ensures the player moves to that exact y
/// Once the count is reached, it is considered "solidified" and guaranteed the reachable y is
/// always a y that has platform(s)
const AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT: u32 = 4;

/// The maximum of number points for auto mobbing to periodically move to
const AUTO_MOB_MAX_PATHING_POINTS: usize = 5;

/// The acceptable y range above and below the detected mob position when matched with a reachable y
const AUTO_MOB_REACHABLE_Y_THRESHOLD: i32 = 10;

/// The minimum x distance required to transition to [`Player::UseKey`] in auto mob action
const AUTO_MOB_USE_KEY_X_THRESHOLD: i32 = 20;

/// The minimum y distance required to transition to [`Player::UseKey`] in auto mob action
const AUTO_MOB_USE_KEY_Y_THRESHOLD: i32 = 8;

/// The maximum number of times rune solving can fail before transition to
/// `Player::CashShopThenExit`
const MAX_RUNE_FAILED_COUNT: u32 = 2;

/// The player previous movement-related contextual state
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
enum LastMovement {
    Adjusting,
    DoubleJumping,
    Falling,
    Grappling,
    UpJumping,
    Jumping,
}

/// The player contextual states
#[derive(Clone, Copy, Debug, Display)]
pub enum Player {
    /// Detects player on the minimap
    Detecting,
    /// Does nothing state
    ///
    /// Acts as entry to other state when there is a [`PlayerAction`]
    Idle,
    UseKey(UseKey),
    /// Movement-related coordinator state
    Moving(Point, bool, Option<MovingIntermediates>),
    /// Performs walk or small adjustment x-wise action
    Adjusting(Moving),
    /// Performs double jump action
    DoubleJumping(Moving, bool, bool),
    /// Performs a grappling action
    Grappling(Moving),
    /// Performs a normal jump
    Jumping(Moving),
    /// Performs an up jump action
    UpJumping(Moving),
    /// Performs a falling action
    Falling(Moving, Point),
    /// Unstucks when inside non-detecting position or because of [`PlayerState::unstuck_counter`]
    Unstucking(Timeout, Option<bool>),
    /// Stalls for time and return to [`Player::Idle`] or [`PlayerState::stalling_timeout_state`]
    Stalling(Timeout, u32),
    /// Tries to solve a rune
    SolvingRune(SolvingRune),
    /// Enters the cash shop then exit after 10 seconds
    CashShopThenExit(Timeout, CashShop),
}

impl Contextual for Player {
    type Persistent = PlayerState;

    // 草草ｗｗ。。。
    // TODO: detect if a point is reachable after number of retries?
    // TODO: split into smaller files?
    fn update(self, context: &Context, state: &mut PlayerState) -> ControlFlow<Self> {
        if state.rune_cash_shop {
            let _ = context.keys.send_up(KeyKind::Up);
            let _ = context.keys.send_up(KeyKind::Down);
            let _ = context.keys.send_up(KeyKind::Left);
            let _ = context.keys.send_up(KeyKind::Right);
            state.rune_cash_shop = false;
            state.reset_to_idle_next_update = false;
            return ControlFlow::Next(Player::CashShopThenExit(
                Timeout::default(),
                CashShop::Entering,
            ));
        }
        let cur_pos = if state.ignore_pos_update {
            state.last_known_pos
        } else {
            update_state(context, state)
        };
        let Some(cur_pos) = cur_pos else {
            // When the player detection fails, the possible causes are:
            // - Player moved inside the edges of the minimap
            // - Other UIs overlapping the minimap
            //
            // `update_non_positional_context` is here to continue updating
            // `Player::Unstucking` returned from below when the player
            // is inside the edges of the minimap. And also `Player::CashShopThenExit`.
            if let Some(next) = update_non_positional_context(self, context, state, true) {
                return ControlFlow::Next(next);
            }
            let next = if !context.halting
                && let Minimap::Idle(idle) = context.minimap
                && !idle.partially_overlapping
            {
                Player::Unstucking(Timeout::default(), None)
            } else {
                Player::Detecting
            };
            if matches!(next, Player::Unstucking(_, _)) {
                state.last_known_direction = ActionKeyDirection::Any;
            }
            return ControlFlow::Next(next);
        };
        let contextual = if state.reset_to_idle_next_update {
            Player::Idle
        } else {
            self
        };
        let next = update_non_positional_context(contextual, context, state, false)
            .unwrap_or_else(|| update_positional_context(contextual, context, cur_pos, state));
        let control_flow = if state.use_immediate_control_flow {
            ControlFlow::Immediate(next)
        } else {
            ControlFlow::Next(next)
        };
        state.reset_to_idle_next_update = false;
        state.ignore_pos_update = state.use_immediate_control_flow;
        state.use_immediate_control_flow = false;
        control_flow
    }
}

/// Updates the contextual state that does not require the player current position
#[inline]
fn update_non_positional_context(
    contextual: Player,
    context: &Context,
    state: &mut PlayerState,
    failed_to_detect_player: bool,
) -> Option<Player> {
    match contextual {
        Player::UseKey(use_key) => {
            (!failed_to_detect_player).then(|| update_use_key_context(context, state, use_key))
        }
        Player::Unstucking(timeout, has_settings) => Some(update_unstucking_context(
            context,
            state,
            timeout,
            has_settings,
        )),
        Player::Stalling(timeout, max_timeout) => {
            (!failed_to_detect_player).then(|| update_stalling_context(state, timeout, max_timeout))
        }
        Player::SolvingRune(solving_rune) => (!failed_to_detect_player)
            .then(|| update_solving_rune_context(context, state, solving_rune)),
        // TODO: Improve this?
        Player::CashShopThenExit(timeout, cash_shop) => Some(update_cash_shop_context(
            context,
            state,
            timeout,
            cash_shop,
            failed_to_detect_player,
        )),
        Player::Detecting
        | Player::Idle
        | Player::Moving(_, _, _)
        | Player::Adjusting(_)
        | Player::DoubleJumping(_, _, _)
        | Player::Grappling(_)
        | Player::Jumping(_)
        | Player::UpJumping(_)
        | Player::Falling(_, _) => None,
    }
}

/// Updates the contextual state that requires the player current position
#[inline]
fn update_positional_context(
    contextual: Player,
    context: &Context,
    cur_pos: Point,
    state: &mut PlayerState,
) -> Player {
    match contextual {
        Player::Detecting => Player::Idle,
        Player::Idle => update_idle_context(context, state, cur_pos),
        Player::Moving(dest, exact, intermediates) => {
            update_moving_context(state, cur_pos, dest, exact, intermediates)
        }
        Player::Adjusting(moving) => update_adjusting_context(context, state, cur_pos, moving),
        Player::DoubleJumping(moving, forced, require_stationary) => update_double_jumping_context(
            context,
            state,
            cur_pos,
            moving,
            forced,
            require_stationary,
        ),
        Player::Grappling(moving) => update_grappling_context(context, state, cur_pos, moving),
        Player::UpJumping(moving) => update_up_jumping_context(context, state, cur_pos, moving),
        Player::Jumping(moving) => {
            if !moving.timeout.started {
                state.last_movement = Some(LastMovement::Jumping);
            }
            update_moving_axis_context(
                moving,
                cur_pos,
                MOVE_TIMEOUT,
                |moving| {
                    let _ = context.keys.send(state.config.jump_key);
                    Player::Jumping(moving)
                },
                None::<fn()>,
                Player::Jumping,
                ChangeAxis::Vertical,
            )
        }
        Player::Falling(moving, anchor) => {
            update_falling_context(context, state, cur_pos, moving, anchor)
        }
        Player::UseKey(_)
        | Player::Unstucking(_, _)
        | Player::Stalling(_, _)
        | Player::SolvingRune(_)
        | Player::CashShopThenExit(_, _) => unreachable!(),
    }
}

/// Checks proximity in [`PlayerAction::AutoMob`] for transitioning to [`Player::UseKey`]
///
/// This is common logics shared with other contextual states when there is auto mob action
#[inline]
fn on_auto_mob_use_key_action(
    context: &Context,
    action: PlayerAction,
    cur_pos: Point,
    x_distance: i32,
    y_distance: i32,
) -> Option<(Player, bool)> {
    if x_distance <= AUTO_MOB_USE_KEY_X_THRESHOLD && y_distance <= AUTO_MOB_USE_KEY_Y_THRESHOLD {
        let _ = context.keys.send_up(KeyKind::Down);
        let _ = context.keys.send_up(KeyKind::Up);
        let _ = context.keys.send_up(KeyKind::Left);
        let _ = context.keys.send_up(KeyKind::Right);
        Some((
            Player::UseKey(UseKey::from_action_pos(action, Some(cur_pos))),
            false,
        ))
    } else {
        None
    }
}

#[inline]
fn x_distance_direction(dest: Point, cur_pos: Point) -> (i32, i32) {
    let direction = dest.x - cur_pos.x;
    let distance = direction.abs();
    (distance, direction)
}

#[inline]
fn y_distance_direction(dest: Point, cur_pos: Point) -> (i32, i32) {
    let direction = dest.y - cur_pos.y;
    let distance = direction.abs();
    (distance, direction)
}

// TODO: ??????
// TODO: is 16 good?
#[inline]
fn find_points(
    platforms: &[PlatformWithNeighbors],
    cur_pos: Point,
    dest: Point,
    exact: bool,
    up_jump_only: bool,
) -> Option<MovingIntermediates> {
    let vertical_threshold = if up_jump_only {
        GRAPPLING_THRESHOLD
    } else {
        GRAPPLING_MAX_THRESHOLD
    };
    let vec = find_points_with(
        platforms,
        cur_pos,
        dest,
        DOUBLE_JUMP_THRESHOLD,
        JUMP_THRESHOLD,
        vertical_threshold,
    )?;
    let len = vec.len();
    let array = Array::from_iter(
        vec.into_iter()
            .enumerate()
            .map(|(i, point)| (point, if i == len - 1 { exact } else { false })),
    );
    Some(MovingIntermediates {
        current: 0,
        inner: array,
    })
}

// TODO: add more tests
#[cfg(test)]
mod tests {
    // use opencv::core::Rect;

    use std::assert_matches::assert_matches;

    use opencv::core::Point;
    use platforms::windows::KeyKind;

    use super::{Moving, PlayerState, update_falling_context, update_up_jumping_context};
    use crate::{
        bridge::MockKeySender,
        context::Context,
        player::{Player, Timeout},
    };

    // fn create_mock_detector() -> MockDetector {
    //     let rect = Rect::new(0, 0, 100, 100);
    //     let player = Rect::new(50, 50, 10, 10);
    //     let mut detector = MockDetector::new();
    //     detector.expect_clone().returning(|| create_mock_detector());
    //     detector.expect_detect_player().return_const(Ok(player));
    //     detector
    // }

    // #[tokio::test(start_paused = true)]
    // async fn update_health_state() {
    //     let rect = Rect::new(0, 0, 100, 100);
    //     let context = Context::default();
    //     let state = PlayerState::default();
    // update_health_state("");
    // }

    #[test]
    fn adjusting() {
        // TODO
    }

    #[test]
    fn double_jumping() {
        // TODO
    }

    #[test]
    fn grappling() {
        // TODO
    }

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
        update_up_jumping_context(&context, &mut state, pos, moving);
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
        update_up_jumping_context(&context, &mut state, pos, moving);
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
        update_up_jumping_context(&context, &mut state, pos, moving);
        let _ = context.keys; // drop mock for validation
    }

    #[test]
    fn up_jumping_update() {
        let cur_pos = Point::new(7, 7);
        let moving_pos = Point::new(7, 1);
        let moving = Moving {
            pos: moving_pos,
            timeout: Timeout {
                started: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut state = PlayerState::default();
        let context = Context::new(None, None);

        // up jumped because y changed > 5
        assert_matches!(
            update_up_jumping_context(&context, &mut state, cur_pos, moving),
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
        update_falling_context(&context, &mut state, pos, moving, Point::default());
        let _ = context.keys;

        // Don't send keys if not stationary
        let mut keys = MockKeySender::new();
        keys.expect_send_down().never();
        keys.expect_send().never();
        let context = Context::new(Some(keys), None);
        state.is_stationary = false;
        update_falling_context(&context, &mut state, pos, moving, Point::default());
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
            update_falling_context(&context, &mut state, pos, moving, anchor),
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
