use actions::{on_action, on_action_state_mut};
use adjust::update_adjusting_context;
use cash_shop::{CashShop, update_cash_shop_context};
use double_jump::update_double_jumping_context;
use fall::update_falling_context;
use grapple::update_grappling_context;
use idle::update_idle_context;
use jump::update_jumping_context;
use moving::{MOVE_TIMEOUT, Moving, MovingIntermediates, update_moving_context};
use opencv::core::Point;
use platforms::windows::KeyKind;
use solve_rune::{SolvingRune, update_solving_rune_context};
use stall::update_stalling_context;
use state::LastMovement;
use strum::Display;
use timeout::{Timeout, update_with_timeout};
use unstuck::update_unstucking_context;
use up_jump::update_up_jumping_context;
use use_key::{UseKey, update_use_key_context};

use crate::{
    context::{Context, Contextual, ControlFlow},
    database::ActionKeyDirection,
    minimap::Minimap,
};

mod actions;
mod adjust;
mod cash_shop;
mod double_jump;
mod fall;
mod grapple;
mod idle;
mod jump;
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
    actions::PlayerActionMove, double_jump::DOUBLE_JUMP_THRESHOLD,
    grapple::GRAPPLING_MAX_THRESHOLD, grapple::GRAPPLING_THRESHOLD, state::PlayerState,
};

/// Minimum y distance from the destination required to perform a jump
pub const JUMP_THRESHOLD: i32 = 7;

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

impl Player {
    #[inline]
    pub fn can_action_override_current_state(&self) -> bool {
        match self {
            Player::Detecting
            | Player::Idle
            | Player::Moving(_, _, _)
            | Player::DoubleJumping(_, false, _)
            | Player::Adjusting(_) => true,
            Player::Grappling(moving)
            | Player::Jumping(moving)
            | Player::UpJumping(moving)
            | Player::Falling(moving, _) => moving.completed,
            Player::SolvingRune(_)
            | Player::CashShopThenExit(_, _)
            | Player::Unstucking(_, _)
            | Player::DoubleJumping(_, true, _)
            | Player::UseKey(_)
            | Player::Stalling(_, _) => false,
        }
    }
}

impl Contextual for Player {
    type Persistent = PlayerState;

    // TODO: Detect if a point is reachable after number of retries?
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

        let has_position = if state.ignore_pos_update {
            state.last_known_pos.is_some()
        } else {
            state
                .update_state(context)
                .then(|| state.last_known_pos.unwrap())
                .is_some()
        };
        if !has_position {
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
            .unwrap_or_else(|| update_positional_context(contextual, context, state));
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
    state: &mut PlayerState,
) -> Player {
    match contextual {
        Player::Detecting => Player::Idle,
        Player::Idle => update_idle_context(context, state),
        Player::Moving(dest, exact, intermediates) => {
            update_moving_context(state, dest, exact, intermediates)
        }
        Player::Adjusting(moving) => update_adjusting_context(context, state, moving),
        Player::DoubleJumping(moving, forced, require_stationary) => {
            update_double_jumping_context(context, state, moving, forced, require_stationary)
        }
        Player::Grappling(moving) => update_grappling_context(context, state, moving),
        Player::UpJumping(moving) => update_up_jumping_context(context, state, moving),
        Player::Jumping(moving) => update_jumping_context(context, state, moving),
        Player::Falling(moving, anchor) => update_falling_context(context, state, moving, anchor),
        Player::UseKey(_)
        | Player::Unstucking(_, _)
        | Player::Stalling(_, _)
        | Player::SolvingRune(_)
        | Player::CashShopThenExit(_, _) => unreachable!(),
    }
}
