use opencv::core::Point;
use platforms::windows::KeyKind;
use strum::Display;

use super::{Player, PlayerState, use_key::UseKey};
use crate::{
    Action, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove, KeyBinding, Position,
    context::{Context, MS_PER_TICK},
    database::LinkKeyBinding,
};

/// The minimum x distance required to transition to [`Player::UseKey`] in auto mob action
const AUTO_MOB_USE_KEY_X_THRESHOLD: i32 = 16;

/// The minimum y distance required to transition to [`Player::UseKey`] in auto mob action
const AUTO_MOB_USE_KEY_Y_THRESHOLD: i32 = 8;

/// Represents the fixed key action
///
/// Converted from [`ActionKey`] without fields used by [`Rotator`]
#[derive(Clone, Copy, Debug)]
pub struct PlayerActionKey {
    pub key: KeyBinding,
    pub link_key: Option<LinkKeyBinding>,
    pub count: u32,
    pub position: Option<Position>,
    pub direction: ActionKeyDirection,
    pub with: ActionKeyWith,
    pub wait_before_use_ticks: u32,
    pub wait_before_use_ticks_random_range: u32,
    pub wait_after_use_ticks: u32,
    pub wait_after_use_ticks_random_range: u32,
}

impl From<ActionKey> for PlayerActionKey {
    fn from(
        ActionKey {
            key,
            link_key,
            count,
            position,
            direction,
            with,
            wait_before_use_millis,
            wait_before_use_millis_random_range,
            wait_after_use_millis,
            wait_after_use_millis_random_range,
            ..
        }: ActionKey,
    ) -> Self {
        Self {
            key,
            link_key,
            count: count.max(1),
            position,
            direction,
            with,
            wait_before_use_ticks: (wait_before_use_millis / MS_PER_TICK) as u32,
            wait_before_use_ticks_random_range: (wait_before_use_millis_random_range / MS_PER_TICK)
                as u32,
            wait_after_use_ticks: (wait_after_use_millis / MS_PER_TICK) as u32,
            wait_after_use_ticks_random_range: (wait_after_use_millis_random_range / MS_PER_TICK)
                as u32,
        }
    }
}

/// Represents the fixed move action
///
/// Converted from [`ActionMove`] without fields used by [`Rotator`]
#[derive(Clone, Copy, Debug)]
pub struct PlayerActionMove {
    pub position: Position,
    pub wait_after_move_ticks: u32,
}

impl From<ActionMove> for PlayerActionMove {
    fn from(
        ActionMove {
            position,
            wait_after_move_millis,
            ..
        }: ActionMove,
    ) -> Self {
        Self {
            position,
            wait_after_move_ticks: (wait_after_move_millis / MS_PER_TICK) as u32,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(Default))]
pub struct PlayerActionAutoMob {
    pub key: KeyBinding,
    pub count: u32,
    pub wait_before_ticks: u32,
    pub wait_after_ticks: u32,
    pub position: Position,
}

impl std::fmt::Display for PlayerActionAutoMob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}, {}", self.position.x, self.position.y)
    }
}

/// Represents an action the `Rotator` can use
#[derive(Clone, Copy, Debug, Display)]
pub enum PlayerAction {
    /// Fixed key action provided by the user
    Key(PlayerActionKey),
    /// Fixed move action provided by the user
    Move(PlayerActionMove),
    /// Solve rune action
    SolveRune,
    #[strum(to_string = "AutoMob({0})")]
    AutoMob(PlayerActionAutoMob),
}

impl From<Action> for PlayerAction {
    fn from(action: Action) -> Self {
        match action {
            Action::Move(action) => PlayerAction::Move(action.into()),
            Action::Key(action) => PlayerAction::Key(action.into()),
        }
    }
}

/// Checks proximity in [`PlayerAction::AutoMob`] for transitioning to [`Player::UseKey`]
///
/// This is common logics shared with other contextual states when there is auto mob action
#[inline]
pub fn on_auto_mob_use_key_action(
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

/// Callbacks for when there is a normal or priority [`PlayerAction`]
///
/// This version does not require [`PlayerState`] in the callbacks arguments
#[inline]
pub fn on_action(
    state: &mut PlayerState,
    on_action_context: impl FnOnce(PlayerAction) -> Option<(Player, bool)>,
    on_default_context: impl FnOnce() -> Player,
) -> Player {
    on_action_state_mut(
        state,
        |_, action| on_action_context(action),
        on_default_context,
    )
}

/// Callbacks for when there is a normal or priority [`PlayerAction`]
///
/// This version requires a shared reference [`PlayerState`] in the callbacks arguments
#[inline]
pub fn on_action_state(
    state: &mut PlayerState,
    on_action_context: impl FnOnce(&PlayerState, PlayerAction) -> Option<(Player, bool)>,
    on_default_context: impl FnOnce() -> Player,
) -> Player {
    on_action_state_mut(
        state,
        |state, action| on_action_context(state, action),
        on_default_context,
    )
}

/// Callbacks for when there is a normal or priority [`PlayerAction`]
///
/// When there is a priority action, it takes precendece over the normal action. The callback
/// should return a tuple [`Option<(Player, bool)>`] with:
/// - `Some((Player, false))` indicating the callback is handled but `Player` is not terminal state
/// - `Some((Player, true))` indicating the callback is handled and `Player` is terminal state
/// - `None` indicating the callback is not handled and will be defaulted to `on_default_context`
///
/// When the returned tuple indicates a terminal state, the `PlayerAction` is considered complete.
/// Because this function passes a mutable reference of `PlayerState` to `on_action_context`,
/// caller should be aware not to clear the action but let this function handles it.
#[inline]
pub fn on_action_state_mut(
    state: &mut PlayerState,
    on_action_context: impl FnOnce(&mut PlayerState, PlayerAction) -> Option<(Player, bool)>,
    on_default_context: impl FnOnce() -> Player,
) -> Player {
    if let Some(action) = state.priority_action.or(state.normal_action)
        && let Some((next, is_terminal)) = on_action_context(state, action)
    {
        debug_assert!(state.has_normal_action() || state.has_priority_action());
        if is_terminal {
            match action {
                PlayerAction::AutoMob(_)
                | PlayerAction::SolveRune
                | PlayerAction::Move(_)
                | PlayerAction::Key(PlayerActionKey {
                    position: Some(Position { .. }),
                    ..
                }) => {
                    state.unstuck_counter = 0;
                }
                PlayerAction::Key(PlayerActionKey { position: None, .. }) => (),
            }
            // FIXME: clear only when has position?
            state.mark_action_completed();
        }
        return next;
    }
    on_default_context()
}
