use actions::{on_action, on_action_state_mut};
use adjusting::update_adjusting_context;
use double_jump::update_double_jumping_context;
use idle::update_idle_context;
use log::debug;
use moving::{Moving, MovingIntermediates, update_moving_context};
use opencv::core::Point;
use platforms::windows::KeyKind;
use solve_rune::{SolvingRune, update_solving_rune_context};
use stalling::update_stalling_context;
use strum::Display;
use timeout::{ChangeAxis, Timeout, update_moving_axis_context, update_with_timeout};
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
mod adjusting;
mod double_jump;
mod idle;
mod moving;
mod solve_rune;
mod stalling;
mod state;
mod timeout;
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

#[derive(Clone, Copy, Debug)]
pub enum CashShop {
    Entering,
    Entered,
    Exitting,
    Exitted,
    Stalling,
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
        Player::CashShopThenExit(timeout, cash_shop) => {
            let next = match cash_shop {
                CashShop::Entering => {
                    let _ = context.keys.send(state.config.cash_shop_key);
                    let next = if context.detector_unwrap().detect_player_in_cash_shop() {
                        CashShop::Entered
                    } else {
                        CashShop::Entering
                    };
                    Player::CashShopThenExit(timeout, next)
                }
                CashShop::Entered => {
                    update_with_timeout(
                        timeout,
                        305, // exits after 10 secs
                        |timeout| Player::CashShopThenExit(timeout, cash_shop),
                        || Player::CashShopThenExit(timeout, CashShop::Exitting),
                        |timeout| Player::CashShopThenExit(timeout, cash_shop),
                    )
                }
                CashShop::Exitting => {
                    let next = if context.detector_unwrap().detect_player_in_cash_shop() {
                        CashShop::Exitting
                    } else {
                        CashShop::Exitted
                    };
                    let _ = context.keys.send_click_to_focus();
                    let _ = context.keys.send(KeyKind::Esc);
                    let _ = context.keys.send(KeyKind::Enter);
                    Player::CashShopThenExit(timeout, next)
                }
                CashShop::Exitted => {
                    if failed_to_detect_player {
                        Player::CashShopThenExit(timeout, cash_shop)
                    } else {
                        Player::CashShopThenExit(Timeout::default(), CashShop::Stalling)
                    }
                }
                CashShop::Stalling => {
                    update_with_timeout(
                        timeout,
                        90, // returns after 3 secs
                        |timeout| Player::CashShopThenExit(timeout, cash_shop),
                        || Player::Idle,
                        |timeout| Player::CashShopThenExit(timeout, cash_shop),
                    )
                }
            };
            Some(next)
        }
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

/// Updates the [`Player::Grappling`] contextual state
///
/// This state can only be transitioned via [`Player::Moving`] or [`Player::DoubleJumping`]
/// when the player has reached or close to the destination x-wise.
///
/// This state will use the Rope Lift skill.
fn update_grappling_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: Moving,
) -> Player {
    const TIMEOUT: u32 = MOVE_TIMEOUT * 10;
    const STOPPING_TIMEOUT: u32 = MOVE_TIMEOUT * 3;
    const STOPPING_THRESHOLD: i32 = 3;

    if !moving.timeout.started {
        state.last_movement = Some(LastMovement::Grappling);
    }

    let key = state.config.grappling_key;
    let x_changed = cur_pos.x != moving.pos.x;

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            let _ = context.keys.send(key);
            Player::Grappling(moving)
        },
        None::<fn()>,
        |mut moving| {
            let (distance, direction) = y_distance_direction(moving.dest, moving.pos);
            if moving.timeout.current >= MOVE_TIMEOUT && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout_current(TIMEOUT);
            }
            if !moving.completed {
                if direction <= 0 || distance <= STOPPING_THRESHOLD {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                }
            } else if state.has_auto_mob_action_only() || moving.timeout.current >= STOPPING_TIMEOUT
            {
                moving = moving.timeout_current(TIMEOUT);
            }
            Player::Grappling(moving)
        },
        ChangeAxis::Vertical,
    )
}

/// Updates the [`Player::UpJumping`] contextual state
///
/// This state can only be transitioned via [`Player::Moving`] when the
/// player has reached the destination x-wise.
///
/// This state will:
/// - Abort the action if the player is near a portal
/// - Perform an up jump
fn update_up_jumping_context(
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

/// Updates the [`Player::Falling`] contextual state
///
/// This state will perform a drop down `Down Key + Space`
fn update_falling_context(
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

/// Updates the [`Player::Unstucking`] contextual state
///
/// This state can only be transitioned to when [`PlayerState::unstuck_counter`] reached the fixed
/// threshold or when the player moved into the edges of the minimap.
/// If [`PlayerState::unstuck_consecutive_counter`] has not reached the threshold and the player
/// moved into the left/right/top edges of the minimap, it will try to move
/// out as appropriate. It will also try to press ESC key to exit any dialog.
///
/// Each initial transition to [`Player::Unstucking`] increases
/// the [`PlayerState::unstuck_consecutive_counter`] by one. If the threshold is reached, this
/// state will enter GAMBA mode. And by definition, it means `random bullsh*t go`.
fn update_unstucking_context(
    context: &Context,
    state: &mut PlayerState,
    timeout: Timeout,
    has_settings: Option<bool>,
) -> Player {
    const Y_IGNORE_THRESHOLD: i32 = 18;
    // what is gamba mode? i am disappointed if you don't know
    const GAMBA_MODE_COUNT: u32 = 3;
    /// Random threshold to choose unstucking direction
    const X_TO_RIGHT_THRESHOLD: i32 = 10;

    let Minimap::Idle(idle) = context.minimap else {
        return Player::Detecting;
    };

    if !timeout.started {
        if state.unstuck_consecutive_counter + 1 < GAMBA_MODE_COUNT && has_settings.is_none() {
            let Update::Ok(has_settings) =
                update_detection_task(context, 0, &mut state.unstuck_task, move |detector| {
                    Ok(detector.detect_esc_settings())
                })
            else {
                return Player::Unstucking(timeout, has_settings);
            };
            return Player::Unstucking(timeout, Some(has_settings));
        }
        debug_assert!(
            state.unstuck_consecutive_counter + 1 >= GAMBA_MODE_COUNT || has_settings.is_some()
        );
        if state.unstuck_consecutive_counter < GAMBA_MODE_COUNT {
            state.unstuck_consecutive_counter += 1;
        }
    }

    let pos = state
        .last_known_pos
        .map(|pos| Point::new(pos.x, idle.bbox.height - pos.y));
    let is_gamba_mode = pos.is_none() || state.unstuck_consecutive_counter >= GAMBA_MODE_COUNT;

    update_with_timeout(
        timeout,
        MOVE_TIMEOUT,
        |timeout| {
            if has_settings.unwrap_or_default() || is_gamba_mode {
                let _ = context.keys.send(KeyKind::Esc);
            }
            let to_right = match (is_gamba_mode, pos) {
                (true, _) => rand::random_bool(0.5),
                (_, Some(Point { y, .. })) if y <= Y_IGNORE_THRESHOLD => {
                    return Player::Unstucking(timeout, has_settings);
                }
                (_, Some(Point { x, .. })) => x <= X_TO_RIGHT_THRESHOLD,
                (_, None) => unreachable!(),
            };
            if to_right {
                let _ = context.keys.send_down(KeyKind::Right);
            } else {
                let _ = context.keys.send_down(KeyKind::Left);
            }
            Player::Unstucking(timeout, has_settings)
        },
        || {
            let _ = context.keys.send_up(KeyKind::Down);
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            Player::Detecting
        },
        |timeout| {
            let send_space = match (is_gamba_mode, pos) {
                (true, _) => true,
                (_, Some(pos)) if pos.y > Y_IGNORE_THRESHOLD => true,
                _ => false,
            };
            if send_space {
                let _ = context.keys.send(state.config.jump_key);
            }
            Player::Unstucking(timeout, has_settings)
        },
    )
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

#[inline]
fn reset_health(state: &mut PlayerState) {
    state.health = None;
    state.health_task = None;
    state.health_bar = None;
    state.health_bar_task = None;
}

/// Increments the rune validation fail count and sets [`PlayerState::rune_cash_shop`] if needed
#[inline]
fn update_rune_fail_count_state(state: &mut PlayerState) {
    state.rune_failed_count += 1;
    if state.rune_failed_count >= MAX_RUNE_FAILED_COUNT {
        state.rune_failed_count = 0;
        state.rune_cash_shop = true;
    }
}

/// Updates the rune validation [`Timeout`]
///
/// [`PlayerState::rune_validate_timeout`] is [`Some`] only when [`Player::SolvingRune`]
/// successfully detects and sends all the keys. After about 12 seconds, it
/// will check if the player has the rune buff.
#[inline]
fn update_rune_validating_state(context: &Context, state: &mut PlayerState) {
    const VALIDATE_TIMEOUT: u32 = 375;

    debug_assert!(state.rune_failed_count < MAX_RUNE_FAILED_COUNT);
    debug_assert!(!state.rune_cash_shop);
    state.rune_validate_timeout = state.rune_validate_timeout.and_then(|timeout| {
        update_with_timeout(
            timeout,
            VALIDATE_TIMEOUT,
            Some,
            || {
                if matches!(context.buffs[BuffKind::Rune], Buff::NoBuff) {
                    update_rune_fail_count_state(state);
                } else {
                    state.rune_failed_count = 0;
                }
                None
            },
            Some,
        )
    });
}

// TODO: This should be a PlayerAction?
#[inline]
fn update_health_state(context: &Context, state: &mut PlayerState) {
    if let Player::SolvingRune(_) = context.player {
        return;
    }
    if state.config.use_potion_below_percent.is_none() {
        reset_health(state);
        return;
    }

    let Some(health_bar) = state.health_bar else {
        let update =
            update_detection_task(context, 1000, &mut state.health_bar_task, move |detector| {
                detector.detect_player_health_bar()
            });
        if let Update::Ok(health_bar) = update {
            state.health_bar = Some(health_bar);
        }
        return;
    };

    let Update::Ok(health) = update_detection_task(
        context,
        state.config.update_health_millis.unwrap_or(1000),
        &mut state.health_task,
        move |detector| {
            let (current_bar, max_bar) =
                detector.detect_player_current_max_health_bars(health_bar)?;
            let health = detector.detect_player_health(current_bar, max_bar)?;
            debug!(target: "player", "health updated {:?}", health);
            Ok(health)
        },
    ) else {
        return;
    };

    let percentage = state.config.use_potion_below_percent.unwrap();
    let (current, max) = health;
    let ratio = current as f32 / max as f32;

    state.health = Some(health);
    if ratio <= percentage {
        let _ = context.keys.send(state.config.potion_key);
    }
}

#[inline]
fn update_is_dead_state(context: &Context, state: &mut PlayerState) {
    let Update::Ok(is_dead) =
        update_detection_task(context, 5000, &mut state.is_dead_task, |detector| {
            Ok(detector.detect_player_is_dead())
        })
    else {
        return;
    };
    if is_dead && !state.is_dead {
        let _ = context
            .notification
            .schedule_notification(NotificationKind::PlayerIsDead);
    }
    state.is_dead = is_dead;
}

/// Updates the [`PlayerState`]
///
/// This function:
/// - Returns the player current position or `None` when the minimap or player cannot be detected
/// - Updates the stationary check via `state.is_stationary_timeout`
/// - Delegates to `update_health_state`, `update_rune_validating_state` and `update_is_dead_state`
/// - Resets `state.unstuck_counter` and `state.unstuck_consecutive_counter` when position changed
#[inline]
fn update_state(context: &Context, state: &mut PlayerState) -> Option<Point> {
    let Minimap::Idle(idle) = &context.minimap else {
        reset_health(state);
        return None;
    };
    let minimap_bbox = idle.bbox;
    let Ok(bbox) = context.detector_unwrap().detect_player(minimap_bbox) else {
        reset_health(state);
        return None;
    };
    let tl = bbox.tl();
    let br = bbox.br();
    let x = (tl.x + br.x) / 2;
    let y = minimap_bbox.height - br.y;
    let pos = Point::new(x, y);
    let last_known_pos = state.last_known_pos.unwrap_or(pos);
    if last_known_pos != pos {
        state.unstuck_counter = 0;
        state.unstuck_consecutive_counter = 0;
        state.is_stationary_timeout = Timeout::default();
    }

    let (is_stationary, is_stationary_timeout) = update_with_timeout(
        state.is_stationary_timeout,
        MOVE_TIMEOUT,
        |timeout| (false, timeout),
        || (true, state.is_stationary_timeout),
        |timeout| (false, timeout),
    );
    state.is_stationary = is_stationary;
    state.is_stationary_timeout = is_stationary_timeout;
    state.last_known_pos = Some(pos);

    update_health_state(context, state);
    update_rune_validating_state(context, state);
    update_is_dead_state(context, state);
    Some(pos)
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
