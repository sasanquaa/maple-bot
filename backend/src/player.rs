use std::ops::Range;

use log::debug;
use opencv::{core::Point, prelude::Mat};
use platforms::windows::keys::KeyKind;

use crate::{
    detect::{detect_player_rune_buff, detect_rune_arrows},
    models::{ActionKeyDirection, ActionKeyWith},
};

use super::{
    context::{Context, Contextual, ControlFlow},
    detect::detect_player,
    minimap::Minimap,
    models::{Action, KeyBinding},
};

const ADJUSTING_SHORT_THRESHOLD: i32 = 1;
const ADJUSTING_MEDIUM_THRESHOLD: i32 = 3;
const DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Maximum amount of ticks a change in x or y direction must be detected
const PLAYER_MOVE_TIMEOUT: u32 = 4;

#[derive(Debug, Default)]
pub struct PlayerState {
    /// A normal action requested by the `Rotator`
    normal_action: Option<PlayerAction>,
    /// A priority action requested by the `Rotator`, this action will override
    /// the normal action if it is in the middle of executing.
    priority_action: Option<PlayerAction>,
    /// The RopeLift key, must be set first before use
    pub grappling_key: Option<KeyKind>,
    /// The up jump key with `None` indicating composite jump (Up arrow + Double Space)
    pub upjump_key: Option<KeyKind>,
    /// Tracks if the player moved within a specified ticks to determine if the player is on ground
    is_on_ground_timeout: Timeout,
    /// Whether the player is on ground
    is_on_ground: bool,
    /// Approximates the player direction for using key
    last_known_direction: ActionKeyDirection,
    /// Last known position after each detection used for unstucking
    last_known_pos: Option<Point>,
    /// Indicates whether to use `ControlFlow::Immediate` on this update
    use_immediate_control_flow: bool,
    /// Indicates whether to reset the contextual state back to `Player::Idle` on next update
    reset_to_idle_next_update: bool,
    /// Indicates whether the contextual state was `Player::Falling`
    /// Helps for coordinating between double jump / adjusting + falling
    /// Code that uses this variable must clear it after use
    was_falling: bool,
    /// Tracks whether the player has a rune buff
    pub(crate) has_rune_buff: bool,
    /// The interval between rune buff checks
    has_rune_buff_check_interval: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum PlayerAction {
    Fixed(Action),
    SolveRune(Point),
}

impl PlayerState {
    pub fn has_normal_action(&self) -> bool {
        self.normal_action.is_some()
    }

    pub fn set_normal_action(&mut self, action: PlayerAction) {
        self.normal_action = Some(action);
        self.reset_to_idle_next_update = true;
    }

    pub fn has_priority_action(&self) -> bool {
        self.priority_action.is_some()
    }

    pub fn set_priority_action(&mut self, action: PlayerAction) {
        self.priority_action = Some(action);
        self.reset_to_idle_next_update = true;
    }
}

/// The axis to which the change in position should be detected.
#[derive(Clone, Copy)]
enum ChangeAxis {
    /// Detects a change in y direction
    Vertical,
    /// Detects a change in both directions
    Both,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Timeout {
    current: u32,
    started: bool,
}

/// A contextual state that stores moving-related data.
#[derive(Clone, Copy, Debug)]
pub struct PlayerMoving {
    /// The player's previous position
    pos: Point,
    /// The destination the player is moving to
    dest: Point,
    /// Whether to allow adjusting to precise destination
    exact: bool,
    /// Whether the movement has completed
    completed: bool,
    /// Current timeout ticks for checking if the player position's changed
    timeout: Timeout,
}

impl PlayerMoving {
    fn new(pos: Point, dest: Point, exact: bool) -> Self {
        Self {
            pos,
            dest,
            exact,
            completed: false,
            timeout: Timeout::default(),
        }
    }

    fn pos(self, pos: Point) -> PlayerMoving {
        PlayerMoving { pos, ..self }
    }

    fn completed(self, completed: bool) -> PlayerMoving {
        PlayerMoving { completed, ..self }
    }

    fn timeout(self, timeout: Timeout) -> PlayerMoving {
        PlayerMoving { timeout, ..self }
    }

    fn timeout_current(self, current: u32) -> PlayerMoving {
        PlayerMoving {
            timeout: Timeout {
                current,
                ..self.timeout
            },
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerUseKey {
    key: KeyBinding,
    direction: ActionKeyDirection,
    with: ActionKeyWith,
    wait_before_use_ticks: u32,
    wait_after_use_ticks: u32,
    timeout: Timeout,
}

impl PlayerUseKey {
    fn new_from_action(action: Action) -> Self {
        match action {
            Action::Key {
                key,
                direction,
                with,
                wait_before_use_ticks,
                wait_after_use_ticks,
                ..
            } => Self {
                key,
                direction,
                with,
                wait_before_use_ticks,
                wait_after_use_ticks,
                timeout: Timeout::default(),
            },
            Action::Move { .. } => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Player {
    Detecting,
    Idle,
    UseKey(PlayerUseKey),
    Moving(Point, bool),
    Adjusting(PlayerMoving),
    DoubleJumping(PlayerMoving, bool),
    Grappling(PlayerMoving),
    Jumping(PlayerMoving),
    UpJumping(PlayerMoving),
    Falling(PlayerMoving),
    Unstucking(Timeout),
    Stalling(Timeout, u32),
    SolvingRune(Timeout, usize, Option<[KeyKind; 4]>),
}

impl Contextual for Player {
    type Persistent = PlayerState;

    // 草草ｗｗ。。。
    // TODO: detect if a point is reachable after number of retries?
    // TODO: add unit tests
    fn update(self, context: &Context, mat: &Mat, state: &mut PlayerState) -> ControlFlow<Self> {
        if let Player::Unstucking(_) = self {
            return ControlFlow::Next(update_context(
                self,
                context,
                mat,
                state.last_known_pos.unwrap(),
                state,
            ));
        }
        let Some(cur_pos) = update_pos(context, mat, state) else {
            let next = match (state.last_known_pos, context.minimap) {
                (Some(_), Minimap::Idle(idle)) => {
                    if idle.partially_overlapping {
                        Player::Detecting
                    } else {
                        Player::Unstucking(Timeout::default())
                    }
                }
                _ => Player::Detecting,
            };
            if matches!(next, Player::Unstucking(_)) {
                state.last_known_direction = ActionKeyDirection::Any;
            }
            return ControlFlow::Next(next);
        };
        let contextual = if state.reset_to_idle_next_update {
            let _ = context.keys.send_up(KeyKind::Up);
            let _ = context.keys.send_up(KeyKind::Down);
            let _ = context.keys.send_up(KeyKind::Left);
            let _ = context.keys.send_up(KeyKind::Right);
            Player::Idle
        } else {
            self
        };
        let next = update_context(contextual, context, mat, cur_pos, state);
        let cf = if state.use_immediate_control_flow {
            ControlFlow::Immediate(next)
        } else {
            ControlFlow::Next(next)
        };
        state.reset_to_idle_next_update = false;
        state.use_immediate_control_flow = false;
        cf
    }
}

fn update_context(
    contextual: Player,
    context: &Context,
    mat: &Mat,
    cur_pos: Point,
    state: &mut PlayerState,
) -> Player {
    match contextual {
        Player::Detecting => Player::Idle,
        Player::Idle => update_idle_context(state, cur_pos),
        Player::UseKey(use_key) => update_use_key_context(context, state, use_key),
        Player::Moving(dest, exact) => update_moving_context(state, cur_pos, dest, exact),
        Player::Adjusting(moving) => update_adjusting_context(context, state, cur_pos, moving),
        Player::DoubleJumping(moving, ignore_grappling) => {
            update_double_jumping_context(context, state, cur_pos, moving, ignore_grappling)
        }
        Player::Grappling(moving) => update_grappling_context(context, state, cur_pos, moving),
        Player::UpJumping(moving) => update_up_jumping_context(context, state, cur_pos, moving),
        Player::Jumping(moving) => update_moving_axis_context(
            moving,
            cur_pos,
            PLAYER_MOVE_TIMEOUT,
            |moving| {
                let _ = context.keys.send(KeyKind::Space);
                Player::Jumping(moving)
            },
            None::<fn()>,
            Player::Jumping,
            ChangeAxis::Vertical,
        ),
        Player::Falling(moving) => update_falling_context(context, state, cur_pos, moving),
        Player::Unstucking(timeout) => update_unstucking_context(context, cur_pos, timeout),
        Player::Stalling(timeout, max_timeout) => {
            let update = |timeout| Player::Stalling(timeout, max_timeout);
            let next = update_timeout(timeout, max_timeout, update, || Player::Idle, update);
            on_action(
                state,
                |_, _| Some((next, matches!(next, Player::Idle))),
                || next,
            )
        }
        Player::SolvingRune(timeout, index, result) => {
            update_solving_rune_context(context, mat, state, timeout, index, result)
        }
    }
}

fn update_idle_context(state: &mut PlayerState, cur_pos: Point) -> Player {
    fn on_fixed_action(action: Action, cur_pos: Point) -> Option<(Player, bool)> {
        match action {
            Action::Move { position, .. } => {
                debug!(target: "player", "handling move: {} {}", position.x, position.y);
                Some((
                    Player::Moving(Point::new(position.x, position.y), position.allow_adjusting),
                    false,
                ))
            }
            Action::Key {
                position: Some(position),
                ..
            } => Some((
                Player::Moving(Point::new(position.x, position.y), position.allow_adjusting),
                false,
            )),
            Action::Key {
                position: None,
                with: ActionKeyWith::DoubleJump,
                ..
            } => Some((
                Player::DoubleJumping(PlayerMoving::new(cur_pos, cur_pos, false), true),
                false,
            )),
            Action::Key {
                position: None,
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            } => Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
        }
    }

    on_action(
        state,
        |action, _| match action {
            PlayerAction::Fixed(action) => on_fixed_action(action, cur_pos),
            PlayerAction::SolveRune(dest) => Some((Player::Moving(dest, true), false)),
        },
        || Player::Idle,
    )
}

fn update_use_key_context(
    context: &Context,
    state: &mut PlayerState,
    use_key: PlayerUseKey,
) -> Player {
    const USE_KEY_TIMEOUT: u32 = 10;
    const CHANGE_DIRECTION_TIMEOUT: u32 = 2;

    fn update_direction(
        context: &Context,
        state: &mut PlayerState,
        timeout: Timeout,
        direction: ActionKeyDirection,
    ) -> bool {
        match direction {
            ActionKeyDirection::Left => {
                if !matches!(state.last_known_direction, ActionKeyDirection::Left) {
                    let _ = context.keys.send_down(KeyKind::Left);
                    if timeout.current >= CHANGE_DIRECTION_TIMEOUT {
                        state.last_known_direction = ActionKeyDirection::Left;
                    }
                    false
                } else {
                    let _ = context.keys.send_up(KeyKind::Left);
                    true
                }
            }
            ActionKeyDirection::Right => {
                if !matches!(state.last_known_direction, ActionKeyDirection::Right) {
                    let _ = context.keys.send_down(KeyKind::Right);
                    if timeout.current >= CHANGE_DIRECTION_TIMEOUT {
                        state.last_known_direction = ActionKeyDirection::Right;
                    }
                    false
                } else {
                    let _ = context.keys.send_up(KeyKind::Right);
                    true
                }
            }
            ActionKeyDirection::Any => true,
        }
    }

    if !use_key.timeout.started {
        state.use_immediate_control_flow = true;
    }

    let update = |timeout| Player::UseKey(PlayerUseKey { timeout, ..use_key });
    let next = update_timeout(
        use_key.timeout,
        USE_KEY_TIMEOUT + use_key.wait_before_use_ticks,
        update,
        || Player::Idle,
        |timeout| {
            if matches!(use_key.with, ActionKeyWith::Stationary) && !state.is_on_ground {
                return update(timeout);
            }
            if !update_direction(context, state, timeout, use_key.direction) {
                return update(timeout);
            }
            if timeout.current < use_key.wait_before_use_ticks {
                return update(timeout);
            }
            let key = match use_key.key {
                KeyBinding::Y => KeyKind::Y,
                KeyBinding::F => KeyKind::F,
                KeyBinding::C => KeyKind::C,
                KeyBinding::A => KeyKind::A,
                KeyBinding::W => KeyKind::W,
                KeyBinding::R => KeyKind::R,
                KeyBinding::F2 => KeyKind::F2,
                KeyBinding::F4 => KeyKind::F4,
                KeyBinding::Delete => KeyKind::Delete,
                KeyBinding::Up => KeyKind::Up,
                KeyBinding::One => KeyKind::One,
                KeyBinding::Four => KeyKind::Four,
            };
            let _ = context.keys.send(key);
            if use_key.wait_after_use_ticks > 0 {
                Player::Stalling(Timeout::default(), use_key.wait_after_use_ticks)
            } else {
                Player::Idle
            }
        },
    );

    on_action(
        state,
        |action, _| match action {
            PlayerAction::Fixed(Action::Key { .. }) => Some((next, matches!(next, Player::Idle))),
            PlayerAction::Fixed(Action::Move { .. }) | PlayerAction::SolveRune(_) => None,
        },
        || next,
    )
}

fn update_moving_context(
    state: &mut PlayerState,
    cur_pos: Point,
    dest: Point,
    exact: bool,
) -> Player {
    const PLAYER_VERTICAL_MOVE_THRESHOLD: i32 = 2;
    const PLAYER_GRAPPLING_THRESHOLD: i32 = 25;
    const PLAYER_UP_JUMP_THRESHOLD: i32 = 10;
    const PLAYER_JUMP_THRESHOLD: i32 = 7;
    const PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD: Range<i32> = const {
        debug_assert!(PLAYER_JUMP_THRESHOLD < PLAYER_UP_JUMP_THRESHOLD);
        PLAYER_JUMP_THRESHOLD..PLAYER_UP_JUMP_THRESHOLD
    };

    let (x_distance, _) = x_distance_direction(&dest, &cur_pos);
    let (y_distance, y_direction) = y_distance_direction(&dest, &cur_pos);
    let moving = PlayerMoving::new(cur_pos, dest, exact);
    state.use_immediate_control_flow = true;

    fn on_fixed_action(action: Action, moving: PlayerMoving) -> Option<(Player, bool)> {
        match action {
            Action::Move {
                wait_after_move_ticks,
                ..
            } => {
                if wait_after_move_ticks > 0 {
                    Some((
                        Player::Stalling(Timeout::default(), wait_after_move_ticks),
                        false,
                    ))
                } else {
                    Some((Player::Idle, true))
                }
            }
            Action::Key {
                with: ActionKeyWith::DoubleJump,
                ..
            } => Some((Player::DoubleJumping(moving, true), false)),
            Action::Key {
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            } => Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
        }
    }

    match (x_distance, y_direction, y_distance) {
        (d, _, _) if d >= DOUBLE_JUMP_THRESHOLD => Player::DoubleJumping(moving, false),
        (d, _, _)
            if (exact && d >= ADJUSTING_SHORT_THRESHOLD)
                || (!exact && d >= ADJUSTING_MEDIUM_THRESHOLD) =>
        {
            Player::Adjusting(moving)
        }
        // y > 0: cur_pos is below dest
        // y < 0: cur_pos is above of dest
        (_, y, d)
            if y > 0
                && (d >= PLAYER_GRAPPLING_THRESHOLD
                    || PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD.contains(&d)) =>
        {
            Player::Grappling(moving)
        }
        (_, y, d) if y > 0 && d >= PLAYER_UP_JUMP_THRESHOLD => Player::UpJumping(moving),
        (_, y, d) if y > 0 && d < PLAYER_JUMP_THRESHOLD => Player::Jumping(moving),
        // this probably won't work if the platforms are far apart,
        // which is weird to begin with and only happen in very rare place (e.g. Haven)
        (_, y, d) if y < 0 && d >= PLAYER_VERTICAL_MOVE_THRESHOLD => Player::Falling(moving),
        _ => {
            debug!(
                target: "player",
                "reached {:?} with actual position {:?}",
                dest, cur_pos
            );
            on_action(
                state,
                |action, _| match action {
                    PlayerAction::Fixed(action) => on_fixed_action(action, moving),
                    PlayerAction::SolveRune(_) => {
                        Some((Player::SolvingRune(Timeout::default(), 0, None), false))
                    }
                },
                || Player::Idle,
            )
        }
    }
}

fn update_adjusting_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: PlayerMoving,
) -> Player {
    const USE_KEY_AT_Y_DOUBLE_JUMP_THRESHOLD: i32 = 1;
    const USE_KEY_AT_Y_PROXIMITY_THRESHOLD: i32 = 2;
    const ADJUSTING_SHORT_TIMEOUT: u32 = 2;

    fn on_fixed_action(
        action: Action,
        y_distance: i32,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            Action::Key {
                with: ActionKeyWith::DoubleJump,
                ..
            } => (moving.completed && y_distance <= USE_KEY_AT_Y_DOUBLE_JUMP_THRESHOLD)
                .then_some((Player::DoubleJumping(moving, true), false)),
            Action::Key {
                with: ActionKeyWith::Any,
                ..
            } => (moving.completed && y_distance <= USE_KEY_AT_Y_PROXIMITY_THRESHOLD)
                .then_some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
            Action::Key {
                with: ActionKeyWith::Stationary,
                ..
            }
            | Action::Move { .. } => None,
        }
    }

    let (x_distance, x_direction) = x_distance_direction(&moving.dest, &cur_pos);
    let (y_distance, y_direction) = y_distance_direction(&moving.dest, &cur_pos);
    if x_distance >= DOUBLE_JUMP_THRESHOLD {
        state.use_immediate_control_flow = true;
        return Player::Moving(moving.dest, moving.exact);
    }
    if y_direction < 0 && !state.was_falling && !moving.timeout.started {
        return Player::Falling(moving.pos(cur_pos));
    } else {
        state.was_falling = false;
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT,
        Player::Adjusting,
        None::<fn()>,
        |mut moving| {
            if !moving.completed {
                match (x_distance, x_direction) {
                    (x, d) if x >= ADJUSTING_MEDIUM_THRESHOLD && d > 0 => {
                        let _ = context.keys.send_up(KeyKind::Left);
                        let _ = context.keys.send_down(KeyKind::Right);
                        state.last_known_direction = ActionKeyDirection::Right;
                    }
                    (x, d) if x >= ADJUSTING_MEDIUM_THRESHOLD && d < 0 => {
                        let _ = context.keys.send_up(KeyKind::Right);
                        let _ = context.keys.send_down(KeyKind::Left);
                        state.last_known_direction = ActionKeyDirection::Left;
                    }
                    (x, d) if moving.exact && x >= ADJUSTING_SHORT_THRESHOLD && d > 0 => {
                        let _ = context.keys.send_up(KeyKind::Left);
                        let _ = context.keys.send_down(KeyKind::Right);
                        if moving.timeout.current >= ADJUSTING_SHORT_TIMEOUT {
                            let _ = context.keys.send_up(KeyKind::Right);
                        }
                        state.last_known_direction = ActionKeyDirection::Right;
                    }
                    (x, d) if moving.exact && x >= ADJUSTING_SHORT_THRESHOLD && d < 0 => {
                        let _ = context.keys.send_up(KeyKind::Right);
                        let _ = context.keys.send_down(KeyKind::Left);
                        if moving.timeout.current >= ADJUSTING_SHORT_TIMEOUT {
                            let _ = context.keys.send_up(KeyKind::Left);
                        }
                        state.last_known_direction = ActionKeyDirection::Left;
                    }
                    _ => {
                        let _ = context.keys.send_up(KeyKind::Right);
                        let _ = context.keys.send_up(KeyKind::Left);
                        moving = moving.completed(true);
                    }
                }
            }
            on_action(
                state,
                |action, _| match action {
                    PlayerAction::Fixed(action) => on_fixed_action(action, y_distance, moving),
                    PlayerAction::SolveRune(_) => None,
                },
                || {
                    if !moving.completed {
                        Player::Adjusting(moving)
                    } else {
                        Player::Moving(moving.dest, moving.exact)
                    }
                },
            )
        },
        ChangeAxis::Both,
    )
}

fn update_double_jumping_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: PlayerMoving,
    ignore_grappling: bool,
) -> Player {
    const DOUBLE_JUMP_USE_KEY_X_PROXIMITY_THRESHOLD: i32 = DOUBLE_JUMP_THRESHOLD;
    const DOUBLE_JUMP_USE_KEY_Y_PROXIMITY_THRESHOLD: i32 = 10;
    const DOUBLE_JUMP_GRAPPLING_THRESHOLD: i32 = 4;
    const DOUBLE_JUMPED_FORCE_THRESHOLD: i32 = 3;

    fn on_fixed_action(
        state: &mut PlayerState,
        action: Action,
        x_distance: i32,
        y_distance: i32,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            Action::Key {
                with: ActionKeyWith::DoubleJump | ActionKeyWith::Any,
                ..
            } => {
                if moving.completed
                    && x_distance <= DOUBLE_JUMP_USE_KEY_X_PROXIMITY_THRESHOLD
                    && y_distance <= DOUBLE_JUMP_USE_KEY_Y_PROXIMITY_THRESHOLD
                {
                    state.use_immediate_control_flow = true;
                    Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false))
                } else {
                    None
                }
            }
            Action::Key {
                with: ActionKeyWith::Stationary,
                ..
            }
            | Action::Move { .. } => None,
        }
    }

    let x_changed = (cur_pos.x - moving.pos.x).abs();
    let (x_distance, x_direction) = x_distance_direction(&moving.dest, &cur_pos);
    let (y_distance, y_direction) = y_distance_direction(&moving.dest, &cur_pos);
    if y_direction < 0 && !state.was_falling && !moving.timeout.started {
        return Player::Falling(moving.pos(cur_pos));
    } else {
        state.was_falling = false;
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT * 2,
        |moving| Player::DoubleJumping(moving, ignore_grappling),
        None::<fn()>,
        |mut moving| {
            if !moving.completed {
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
                    _ => (),
                }
                if x_distance >= DOUBLE_JUMP_THRESHOLD || x_changed <= DOUBLE_JUMPED_FORCE_THRESHOLD
                {
                    let _ = context.keys.send(KeyKind::Space);
                } else {
                    let _ = context.keys.send_up(KeyKind::Right);
                    let _ = context.keys.send_up(KeyKind::Left);
                    moving = moving.completed(true);
                }
            }
            on_action(
                state,
                |action, state| match action {
                    PlayerAction::Fixed(action) => {
                        on_fixed_action(state, action, x_distance, y_distance, moving)
                    }
                    PlayerAction::SolveRune(_) => None,
                },
                || {
                    if moving.completed
                        && !ignore_grappling
                        && x_distance <= DOUBLE_JUMP_GRAPPLING_THRESHOLD
                        && y_direction > 0
                    {
                        debug!(target: "player", "performs grappling on double jump");
                        Player::Grappling(moving.completed(false).timeout(Timeout::default()))
                    } else if moving.completed && moving.timeout.current >= PLAYER_MOVE_TIMEOUT {
                        Player::Moving(moving.dest, moving.exact)
                    } else {
                        Player::DoubleJumping(moving, ignore_grappling)
                    }
                },
            )
        },
        ChangeAxis::Both,
    )
}

fn update_grappling_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: PlayerMoving,
) -> Player {
    const GRAPPLING_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 15;
    const GRAPPLING_STOPPING_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 2;
    const GRAPPLING_STOPPING_THRESHOLD: i32 = 2;

    if state.grappling_key.is_none() {
        debug!(target: "player", "failed to use grappling as key is not set");
        return Player::Idle;
    }

    let key = state.grappling_key.unwrap();
    let x_changed = cur_pos.x != moving.pos.x;
    update_moving_axis_context(
        moving,
        cur_pos,
        GRAPPLING_TIMEOUT,
        |moving| {
            let _ = context.keys.send(key);
            Player::Grappling(moving)
        },
        None::<fn()>,
        |mut moving| {
            let (distance, _) = y_distance_direction(&moving.dest, &moving.pos);
            if moving.timeout.current >= PLAYER_MOVE_TIMEOUT && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout_current(GRAPPLING_TIMEOUT);
            } else if !moving.completed {
                if distance <= GRAPPLING_STOPPING_THRESHOLD {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                }
            } else if moving.timeout.current >= GRAPPLING_STOPPING_TIMEOUT {
                moving = moving.timeout_current(GRAPPLING_TIMEOUT);
            }
            Player::Grappling(moving)
        },
        ChangeAxis::Vertical,
    )
}

fn update_up_jumping_context(
    context: &Context,
    state: &PlayerState,
    cur_pos: Point,
    moving: PlayerMoving,
) -> Player {
    const UP_JUMP_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 2;
    const UP_JUMPED_THRESHOLD: i32 = 4;

    let y_changed = (cur_pos.y - moving.pos.y).abs();
    update_moving_axis_context(
        moving,
        cur_pos,
        UP_JUMP_TIMEOUT,
        |moving| {
            let _ = context.keys.send_down(KeyKind::Up);
            Player::UpJumping(moving)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Up);
        }),
        |mut moving| {
            if !moving.completed {
                if state.upjump_key.is_some() {
                    let _ = context.keys.send(state.upjump_key.unwrap());
                    moving = moving.completed(true);
                } else if y_changed <= UP_JUMPED_THRESHOLD {
                    // spamming space until the player y changes
                    // above a threshold as sending space twice
                    // doesn't work
                    let _ = context.keys.send(KeyKind::Space);
                } else {
                    moving = moving.completed(true);
                }
            }
            Player::UpJumping(moving)
        },
        ChangeAxis::Vertical,
    )
}

fn update_falling_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: PlayerMoving,
) -> Player {
    const FALLING_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 2;

    let y_changed = cur_pos.y - moving.pos.y;
    let (x_distance, _) = x_distance_direction(&moving.dest, &cur_pos);

    update_moving_axis_context(
        moving,
        cur_pos,
        FALLING_TIMEOUT,
        |moving| {
            state.was_falling = true;
            let _ = context.keys.send_down(KeyKind::Down);
            let _ = context.keys.send(KeyKind::Space);
            Player::Falling(moving)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Down);
        }),
        |mut moving| {
            // only timeout early when the player is not super close to
            // the destination, useful for falling + double jumping
            if x_distance >= ADJUSTING_MEDIUM_THRESHOLD && y_changed <= -2 {
                moving = moving.timeout_current(FALLING_TIMEOUT);
            }
            Player::Falling(moving)
        },
        ChangeAxis::Vertical,
    )
}

fn update_unstucking_context(context: &Context, cur_pos: Point, timeout: Timeout) -> Player {
    update_timeout(
        timeout,
        PLAYER_MOVE_TIMEOUT,
        |timeout| {
            if cur_pos.x <= 10 {
                let _ = context.keys.send_down(KeyKind::Right);
            } else {
                let _ = context.keys.send_down(KeyKind::Left);
            }
            Player::Unstucking(timeout)
        },
        || {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            Player::Detecting
        },
        Player::Unstucking,
    )
}

fn update_solving_rune_context(
    context: &Context,
    mat: &Mat,
    state: &mut PlayerState,
    timeout: Timeout,
    index: usize,
    result: Option<[KeyKind; 4]>,
) -> Player {
    const PRESS_KEY_INTERVAL: u32 = 10;

    let next = update_timeout(
        timeout,
        50,
        |timeout| {
            let _ = context.keys.send(KeyKind::Ctrl);
            let result = detect_rune_arrows(mat).ok();
            if result.is_none() {
                Player::Idle
            } else {
                Player::SolvingRune(timeout, index, result)
            }
        },
        // there are only 4 keys with each pressed at % PRESS_KEY_INTERVAL
        // so timing out can never be reached
        || unreachable!(),
        |timeout| {
            if timeout.current % PRESS_KEY_INTERVAL != 0 {
                return Player::SolvingRune(timeout, index, result);
            }
            let keys = result.unwrap();
            let _ = context.keys.send(keys[index]);
            if index >= keys.len() - 1 {
                Player::Idle
            } else {
                Player::SolvingRune(timeout, index + 1, result)
            }
        },
    );
    on_action(
        state,
        |action, _| match action {
            PlayerAction::SolveRune(_) => Some((next, matches!(next, Player::Idle))),
            PlayerAction::Fixed(_) => unreachable!(),
        },
        || next,
    )
}

#[inline(always)]
fn on_action(
    state: &mut PlayerState,
    on_action_context: impl FnOnce(PlayerAction, &mut PlayerState) -> Option<(Player, bool)>,
    on_default_context: impl FnOnce() -> Player,
) -> Player {
    if let Some(action) = state.priority_action.or(state.normal_action) {
        let Some((next, is_terminal)) = on_action_context(action, state) else {
            return on_default_context();
        };
        if is_terminal {
            if state.priority_action.is_some() {
                state.priority_action = None;
            } else {
                state.normal_action = None;
            }
        }
        next
    } else {
        on_default_context()
    }
}

#[inline(always)]
fn x_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.x - cur_pos.x;
    let distance = direction.abs();
    (distance, direction)
}

#[inline(always)]
fn y_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.y - cur_pos.y;
    let distance = direction.abs();
    (distance, direction)
}

#[inline(always)]
fn update_moving_axis_timeout(
    prev_pos: Point,
    cur_pos: Point,
    timeout: Timeout,
    max_timeout: u32,
    axis: ChangeAxis,
) -> Timeout {
    if timeout.current >= max_timeout {
        return timeout;
    }
    let moved = match axis {
        ChangeAxis::Vertical => cur_pos.y != prev_pos.y,
        ChangeAxis::Both { .. } => cur_pos.x != prev_pos.x || cur_pos.y != prev_pos.y,
    };
    Timeout {
        current: if moved { 0 } else { timeout.current + 1 },
        ..timeout
    }
}

#[inline(always)]
fn update_moving_axis_context(
    moving: PlayerMoving,
    cur_pos: Point,
    max_timeout: u32,
    on_started: impl FnOnce(PlayerMoving) -> Player,
    on_timeout: Option<impl FnOnce()>,
    on_update: impl FnOnce(PlayerMoving) -> Player,
    axis: ChangeAxis,
) -> Player {
    update_timeout(
        update_moving_axis_timeout(moving.pos, cur_pos, moving.timeout, max_timeout, axis),
        max_timeout,
        |timeout| on_started(moving.timeout(timeout)),
        || {
            if let Some(callback) = on_timeout {
                callback();
            }
            Player::Moving(moving.dest, moving.exact)
        },
        |timeout| on_update(moving.timeout(timeout)),
    )
}

#[inline(always)]
fn update_timeout(
    timeout: Timeout,
    max_timeout: u32,
    on_started: impl FnOnce(Timeout) -> Player,
    on_timeout: impl FnOnce() -> Player,
    on_update: impl FnOnce(Timeout) -> Player,
) -> Player {
    match timeout {
        t if !t.started => on_started(Timeout {
            started: true,
            current: 0,
        }),
        t if t.current >= max_timeout => on_timeout(),
        t => on_update(Timeout {
            current: t.current + 1,
            ..timeout
        }),
    }
}

#[inline(always)]
fn update_pos(context: &Context, mat: &Mat, state: &mut PlayerState) -> Option<Point> {
    const RUNE_BUFF_CHECK_INTERVAL_TICKS: u32 = 305;

    let Minimap::Idle(idle) = &context.minimap else {
        return None;
    };
    let minimap_bbox = idle.bbox;
    let Ok(bbox) = detect_player(mat, &minimap_bbox) else {
        return None;
    };
    let tl = bbox.tl() - minimap_bbox.tl();
    let br = bbox.br() - minimap_bbox.tl();
    let x = ((tl.x + br.x) / 2) as f32 / idle.scale_w;
    let y = (minimap_bbox.height - br.y) as f32 / idle.scale_h;
    let pos = Point::new(x as i32, y as i32);
    let last_known_pos = state.last_known_pos.unwrap_or(pos);
    if cfg!(debug_assertions)
        && (state.last_known_pos.is_none() || state.last_known_pos.unwrap() != pos)
    {
        debug!(target: "player", "position updated in minimap: {:?} in {:?}", pos, minimap_bbox);
    }
    if state.has_rune_buff_check_interval % RUNE_BUFF_CHECK_INTERVAL_TICKS == 0 {
        state.has_rune_buff = detect_player_rune_buff(mat);
    }
    state.has_rune_buff_check_interval =
        (state.has_rune_buff_check_interval + 1) % RUNE_BUFF_CHECK_INTERVAL_TICKS;
    state.is_on_ground_timeout = update_moving_axis_timeout(
        last_known_pos,
        pos,
        state.is_on_ground_timeout,
        PLAYER_MOVE_TIMEOUT * 2,
        ChangeAxis::Both,
    );
    state.is_on_ground = state.is_on_ground_timeout.current > PLAYER_MOVE_TIMEOUT;
    state.last_known_pos = Some(pos);
    Some(pos)
}
