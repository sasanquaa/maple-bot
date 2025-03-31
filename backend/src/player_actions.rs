use strum::Display;

use crate::{
    Action, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove, KeyBinding, Position,
    context::MS_PER_TICK,
};

/// Represents the fixed key action
#[derive(Clone, Copy, Debug)]
pub struct PlayerActionKey {
    pub key: KeyBinding,
    pub count: u32,
    pub position: Option<Position>,
    pub direction: ActionKeyDirection,
    pub with: ActionKeyWith,
    pub wait_before_use_ticks: u32,
    pub wait_after_use_ticks: u32,
}

impl From<ActionKey> for PlayerActionKey {
    fn from(
        ActionKey {
            key,
            count,
            position,
            direction,
            with,
            wait_before_use_millis,
            wait_after_use_millis,
            ..
        }: ActionKey,
    ) -> Self {
        Self {
            key,
            count,
            position,
            direction,
            with,
            wait_before_use_ticks: (wait_before_use_millis / MS_PER_TICK) as u32,
            wait_after_use_ticks: (wait_after_use_millis / MS_PER_TICK) as u32,
        }
    }
}

/// Represents the fixed move action
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
