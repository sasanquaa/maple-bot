use std::ops::Range;

use log::debug;
use opencv::core::Point;
use platforms::windows::KeyKind;
use strum::Display;

use crate::{
    buff::Buff,
    context::{Context, Contextual, ControlFlow, RUNE_BUFF_POSITION, map_key},
    database::{Action, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove, KeyBinding},
    detect::Detector,
    minimap::Minimap,
};

/// Maximum number of times adjusting or double jump states can be transitioned to without changing position
const UNSTUCK_TRACKER_THRESHOLD: u32 = 10;

/// Minimium y distance required to perform a fall and double jump/adjusting
const ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD: i32 = 8;

/// Minimum x distance from the destination required to spam small movement
const ADJUSTING_SHORT_THRESHOLD: i32 = 1;

/// Minimum x distance from the destination required to walk
const ADJUSTING_MEDIUM_THRESHOLD: i32 = 3;

/// Minimum x distance from the destination required to perform a double jump
const DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Maximum amount of ticks a change in x or y direction must be detected
const PLAYER_MOVE_TIMEOUT: u32 = 5;

#[derive(Debug, Default)]
pub struct PlayerState {
    /// A normal action requested by the `Rotator`
    normal_action: Option<PlayerAction>,
    priority_action_id: i32,
    /// A priority action requested by the `Rotator`, this action will override
    /// the normal action if it is in the middle of executing.
    priority_action: Option<PlayerAction>,
    /// The interact key
    pub interact_key: KeyKind,
    /// The RopeLift key
    pub grappling_key: KeyKind,
    /// The up jump key with `None` indicating composite jump (Up arrow + Double Space)
    pub upjump_key: Option<KeyKind>,
    /// The cash shop key
    pub cash_shop_key: KeyKind,
    /// Tracks if the player moved within a specified ticks to determine if the player is stationary
    is_stationary_timeout: Timeout,
    /// Whether the player is stationary
    is_stationary: bool,
    /// Approximates the player direction for using key
    last_known_direction: ActionKeyDirection,
    /// Last known position after each detection used for unstucking
    pub last_known_pos: Option<Point>,
    /// Indicates whether to use `ControlFlow::Immediate` on this update
    use_immediate_control_flow: bool,
    /// Indicates whether to ignore update_pos and use last_known_pos on next update
    ignore_pos_update: bool,
    /// Indicates whether to reset the contextual state back to `Player::Idle` on next update
    reset_to_idle_next_update: bool,
    /// Indicates whether the contextual state was `Player::DoubleJumping` or `Player::Falling`
    /// Helps for coordinating: use key with direction + double jumping and falling + double jumping
    /// Resets to `None` when the destination is reached or in `Player::Idle`
    last_moving_state: Option<PlayerLastMovingState>,
    /// Tracks whether movement-related actions do not change the player position after a while.
    /// Resets when a limit is reached (for unstucking), in `Player::Idle` or position did change.
    unstuck_counter: u32,
}

#[derive(Clone, Copy, Debug)]
enum PlayerLastMovingState {
    DoubleJumping,
    Falling,
}

/// Represents an action the `Rotator` can use
#[derive(Clone, Copy, Debug)]
pub enum PlayerAction {
    /// Fixed action provided by the user
    Fixed(Action),
    /// Solve rune action
    SolveRune,
}

impl std::fmt::Display for PlayerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayerAction::Fixed(action) => action.fmt(f),
            PlayerAction::SolveRune => write!(f, "SolveRune"),
        }
    }
}

impl PlayerState {
    #[inline]
    pub fn normal_action_name(&self) -> Option<String> {
        self.normal_action.map(|action| action.to_string())
    }

    #[inline]
    pub fn has_normal_action(&self) -> bool {
        self.normal_action.is_some()
    }

    #[inline]
    pub fn set_normal_action(&mut self, action: PlayerAction) {
        self.reset_to_idle_next_update = true;
        self.normal_action = Some(action);
    }

    #[inline]
    pub fn priority_action_name(&self) -> Option<String> {
        self.priority_action.map(|action| action.to_string())
    }

    #[inline]
    pub fn has_priority_action(&self) -> bool {
        self.priority_action.is_some()
    }

    #[inline]
    pub fn set_priority_action(&mut self, id: i32, action: PlayerAction) {
        self.reset_to_idle_next_update = true;
        self.priority_action_id = id;
        self.priority_action = Some(action);
    }

    #[inline]
    pub fn has_solve_rune_or_queue_front_action(&self) -> bool {
        self.priority_action.is_some_and(|action| {
            matches!(
                action,
                PlayerAction::SolveRune
                    | PlayerAction::Fixed(Action::Key(ActionKey {
                        queue_to_front: Some(true),
                        ..
                    }))
            )
        })
    }

    #[inline]
    pub fn replace_priority_action(
        &mut self,
        id: i32,
        action: PlayerAction,
    ) -> Option<(i32, PlayerAction)> {
        debug_assert!(matches!(action, PlayerAction::Fixed(Action::Key(_))));
        let prev_id = self.priority_action_id;
        self.reset_to_idle_next_update = true;
        self.priority_action_id = id;
        Some(prev_id).zip(self.priority_action.replace(action))
    }

    #[inline]
    pub fn abort_actions(&mut self) {
        self.reset_to_idle_next_update = true;
        self.priority_action = None;
        self.normal_action = None;
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

/// A struct that stores the current tick before timing out.
///
/// Most contextual state can be timed out as there is no guaranteed
/// an action will be performed. So timeout is used to retry
/// such action and to avoid looping in a single state forever. Or
/// for some contextual states to perform an action only after timing out.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timeout {
    current: u32,
    started: bool,
}

/// A contextual state that stores moving-related data.
#[derive(Clone, Copy, Debug)]
pub struct PlayerMoving {
    /// The player's previous position and will be updated to current position
    /// after calling `update_moving_axis_timeout`
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

/// Convenient implementations
impl PlayerMoving {
    #[inline]
    fn new(pos: Point, dest: Point, exact: bool) -> Self {
        Self {
            pos,
            dest,
            exact,
            completed: false,
            timeout: Timeout::default(),
        }
    }

    #[inline]
    fn pos(self, pos: Point) -> PlayerMoving {
        PlayerMoving { pos, ..self }
    }

    #[inline]
    fn completed(self, completed: bool) -> PlayerMoving {
        PlayerMoving { completed, ..self }
    }

    #[inline]
    fn timeout(self, timeout: Timeout) -> PlayerMoving {
        PlayerMoving { timeout, ..self }
    }

    #[inline]
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
    #[inline]
    fn new_from_action(action: Action) -> Self {
        match action {
            Action::Key(ActionKey {
                key,
                direction,
                with,
                wait_before_use_ticks,
                wait_after_use_ticks,
                ..
            }) => Self {
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

#[derive(Clone, Copy, Default, Debug)]
pub struct PlayerSolvingRune {
    timeout: Timeout,
    key_index: usize,
    keys: Option<[KeyKind; 4]>,
    validating: bool,
    failed_count: usize,
}

#[derive(Clone, Copy, Debug, Display)]
pub enum Player {
    Detecting,
    Idle,
    UseKey(PlayerUseKey),
    Moving(Point, bool),
    Adjusting(PlayerMoving),
    DoubleJumping(PlayerMoving, bool, bool),
    Grappling(PlayerMoving),
    Jumping(PlayerMoving),
    UpJumping(PlayerMoving),
    Falling(PlayerMoving),
    Unstucking(Timeout),
    Stalling(Timeout, u32),
    SolvingRune(PlayerSolvingRune),
    CashShopThenExit(Timeout, bool, bool),
}

impl Contextual for Player {
    type Persistent = PlayerState;

    // 草草ｗｗ。。。
    // TODO: detect if a point is reachable after number of retries?
    // TODO: add unit tests
    // TODO: support mages
    fn update(
        self,
        context: &Context,
        detector: &mut impl Detector,
        state: &mut PlayerState,
    ) -> ControlFlow<Self> {
        let cur_pos = if state.ignore_pos_update {
            state.last_known_pos
        } else {
            update_state(context, detector, state)
        };
        let Some(cur_pos) = cur_pos else {
            if let Some(next) = update_non_positional_context(self, context, detector, state) {
                return ControlFlow::Next(next);
            }
            let next = if !context.halting
                && let Minimap::Idle(idle) = context.minimap
                && state.last_known_pos.is_some()
            {
                if idle.partially_overlapping {
                    Player::Detecting
                } else {
                    Player::Unstucking(Timeout::default())
                }
            } else {
                Player::Detecting
            };
            if matches!(next, Player::Unstucking(_)) {
                state.last_known_direction = ActionKeyDirection::Any;
            }
            return ControlFlow::Next(next);
        };
        let contextual = if state.reset_to_idle_next_update {
            Player::Idle
        } else {
            self
        };
        let next = update_non_positional_context(contextual, context, detector, state)
            .unwrap_or_else(|| update_positional_context(contextual, context, cur_pos, state));
        if let Player::UseKey(use_key) = next
            && !use_key.timeout.started
        {
            state.use_immediate_control_flow = true;
        }
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

fn update_non_positional_context(
    contextual: Player,
    context: &Context,
    detector: &mut impl Detector,
    state: &mut PlayerState,
) -> Option<Player> {
    match contextual {
        Player::UseKey(use_key) => Some(update_use_key_context(context, state, use_key)),
        Player::Unstucking(timeout) => Some(update_unstucking_context(
            context,
            state.last_known_pos.unwrap(),
            timeout,
            rand::random_bool(0.5), // gamba
        )),
        Player::Stalling(timeout, max_timeout) => {
            let update = |timeout| Player::Stalling(timeout, max_timeout);
            let next = update_timeout(timeout, max_timeout, update, || Player::Idle, update);
            Some(on_action(
                state,
                |_| Some((next, matches!(next, Player::Idle))),
                || next,
            ))
        }
        Player::SolvingRune(solving_rune) => Some(update_solving_rune_context(
            context,
            detector,
            state,
            solving_rune,
        )),
        Player::CashShopThenExit(timeout, in_cash_shop, exitting) => {
            let next = match (in_cash_shop, exitting) {
                (false, _) => {
                    let _ = context.keys.send(state.cash_shop_key);
                    Player::CashShopThenExit(
                        timeout,
                        detector.detect_player_in_cash_shop(),
                        exitting,
                    )
                }
                (true, false) => {
                    update_timeout(
                        timeout,
                        305, // exits after 10 secs
                        |timeout| Player::CashShopThenExit(timeout, in_cash_shop, exitting),
                        || Player::CashShopThenExit(timeout, in_cash_shop, true),
                        |timeout| Player::CashShopThenExit(timeout, in_cash_shop, exitting),
                    )
                }
                (true, true) => {
                    if detector.detect_player_in_cash_shop() {
                        let _ = context.keys.send_click_to_focus();
                        let _ = context.keys.send(KeyKind::Esc);
                        let _ = context.keys.send(KeyKind::Enter);
                        Player::CashShopThenExit(timeout, in_cash_shop, exitting)
                    } else {
                        Player::Idle
                    }
                }
            };
            Some(on_action(
                state,
                |action| match action {
                    PlayerAction::Fixed(_) => None,
                    PlayerAction::SolveRune => Some((next, false)),
                },
                || next,
            ))
        }
        Player::Detecting
        | Player::Idle
        | Player::Moving(_, _)
        | Player::Adjusting(_)
        | Player::DoubleJumping(_, _, _)
        | Player::Grappling(_)
        | Player::Jumping(_)
        | Player::UpJumping(_)
        | Player::Falling(_) => None,
    }
}

fn update_positional_context(
    contextual: Player,
    context: &Context,
    cur_pos: Point,
    state: &mut PlayerState,
) -> Player {
    match contextual {
        Player::Detecting => Player::Idle,
        Player::Idle => update_idle_context(context, state, cur_pos),
        Player::Moving(dest, exact) => update_moving_context(state, cur_pos, dest, exact),
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
        Player::UseKey(_)
        | Player::Unstucking(_)
        | Player::Stalling(_, _)
        | Player::SolvingRune(_)
        | Player::CashShopThenExit(_, _, _) => unreachable!(),
    }
}

fn update_idle_context(context: &Context, state: &mut PlayerState, cur_pos: Point) -> Player {
    fn on_fixed_action(
        last_known_direction: ActionKeyDirection,
        action: Action,
        cur_pos: Point,
    ) -> Option<(Player, bool)> {
        match action {
            Action::Move(ActionMove { position, .. }) => {
                debug!(target: "player", "handling move: {} {}", position.x, position.y);
                Some((
                    Player::Moving(Point::new(position.x, position.y), position.allow_adjusting),
                    false,
                ))
            }
            Action::Key(ActionKey {
                position: Some(position),
                ..
            }) => {
                debug!(target: "player", "handling move: {} {}", position.x, position.y);
                Some((
                    Player::Moving(Point::new(position.x, position.y), position.allow_adjusting),
                    false,
                ))
            }
            Action::Key(ActionKey {
                position: None,
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            }) => {
                if matches!(direction, ActionKeyDirection::Any) || direction == last_known_direction
                {
                    Some((
                        Player::DoubleJumping(
                            PlayerMoving::new(cur_pos, cur_pos, false),
                            true,
                            true,
                        ),
                        false,
                    ))
                } else {
                    Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false))
                }
            }
            Action::Key(ActionKey {
                position: None,
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            }) => Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
        }
    }

    state.unstuck_counter = 0;
    state.last_moving_state = None;
    let last_known_direction = state.last_known_direction;
    let _ = context.keys.send_up(KeyKind::Up);
    let _ = context.keys.send_up(KeyKind::Down);
    let _ = context.keys.send_up(KeyKind::Left);
    let _ = context.keys.send_up(KeyKind::Right);

    on_action(
        state,
        |action| match action {
            PlayerAction::Fixed(action) => on_fixed_action(last_known_direction, action, cur_pos),
            PlayerAction::SolveRune => {
                if let Minimap::Idle(idle) = context.minimap {
                    if let Some(rune) = idle.rune {
                        return Some((Player::Moving(rune, true), false));
                    }
                }
                Some((Player::Idle, true))
            }
        },
        || Player::Idle,
    )
}

fn update_use_key_context(
    context: &Context,
    state: &mut PlayerState,
    use_key: PlayerUseKey,
) -> Player {
    const USE_KEY_TIMEOUT: u32 = 12;
    const CHANGE_DIRECTION_TIMEOUT: u32 = 3;

    fn update_direction(
        context: &Context,
        state: &mut PlayerState,
        timeout: Timeout,
        direction: ActionKeyDirection,
    ) -> bool {
        if matches!(direction, ActionKeyDirection::Any) {
            return true;
        }
        let key = match direction {
            ActionKeyDirection::Left => KeyKind::Left,
            ActionKeyDirection::Right => KeyKind::Right,
            ActionKeyDirection::Any => unreachable!(),
        };
        if state.last_known_direction != direction {
            let _ = context.keys.send_down(key);
            if timeout.current >= CHANGE_DIRECTION_TIMEOUT {
                let _ = context.keys.send_up(key);
                state.last_known_direction = direction;
            }
            false
        } else {
            true
        }
    }

    let update = |timeout| Player::UseKey(PlayerUseKey { timeout, ..use_key });
    let next = update_timeout(
        use_key.timeout,
        USE_KEY_TIMEOUT + use_key.wait_before_use_ticks,
        update,
        || Player::Idle,
        |timeout| {
            if !update_direction(context, state, timeout, use_key.direction) {
                return update(timeout);
            }
            match use_key.with {
                ActionKeyWith::Any => (),
                ActionKeyWith::Stationary => {
                    if !state.is_stationary {
                        return update(timeout);
                    }
                }
                ActionKeyWith::DoubleJump => {
                    if !matches!(
                        state.last_moving_state,
                        Some(PlayerLastMovingState::DoubleJumping)
                    ) {
                        let pos = state.last_known_pos.unwrap();
                        return Player::DoubleJumping(
                            PlayerMoving::new(pos, pos, false),
                            true,
                            true,
                        );
                    }
                }
            }
            if timeout.current < use_key.wait_before_use_ticks {
                return update(timeout);
            }
            let key = map_key(use_key.key);
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
        |action| match action {
            PlayerAction::Fixed(Action::Key(ActionKey { .. })) => {
                Some((next, matches!(next, Player::Idle)))
            }
            PlayerAction::Fixed(Action::Move { .. }) | PlayerAction::SolveRune => None,
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
    const PLAYER_VERTICAL_MOVE_THRESHOLD: i32 = 4;
    const PLAYER_GRAPPLING_THRESHOLD: i32 = 25;
    const PLAYER_UP_JUMP_THRESHOLD: i32 = 10;
    const PLAYER_JUMP_THRESHOLD: i32 = 7;
    const PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD: Range<i32> = const {
        debug_assert!(PLAYER_JUMP_THRESHOLD < PLAYER_UP_JUMP_THRESHOLD);
        PLAYER_JUMP_THRESHOLD..PLAYER_UP_JUMP_THRESHOLD
    };

    state.unstuck_counter += 1;
    if state.unstuck_counter >= UNSTUCK_TRACKER_THRESHOLD {
        state.unstuck_counter = 0;
        return Player::Unstucking(Timeout::default());
    }
    state.use_immediate_control_flow = true;

    let (x_distance, _) = x_distance_direction(&dest, &cur_pos);
    let (y_distance, y_direction) = y_distance_direction(&dest, &cur_pos);
    let moving = PlayerMoving::new(cur_pos, dest, exact);

    fn on_fixed_action(
        last_known_direction: ActionKeyDirection,
        action: Action,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            Action::Move(ActionMove {
                wait_after_move_ticks,
                ..
            }) => {
                if wait_after_move_ticks > 0 {
                    Some((
                        Player::Stalling(Timeout::default(), wait_after_move_ticks),
                        false,
                    ))
                } else {
                    Some((Player::Idle, true))
                }
            }
            Action::Key(ActionKey {
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            }) => {
                if matches!(direction, ActionKeyDirection::Any) || direction == last_known_direction
                {
                    Some((Player::DoubleJumping(moving, true, false), false))
                } else {
                    Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false))
                }
            }
            Action::Key(ActionKey {
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            }) => Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
        }
    }

    match (x_distance, y_direction, y_distance) {
        (d, _, _) if d >= DOUBLE_JUMP_THRESHOLD => Player::DoubleJumping(moving, false, false),
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
        (_, y, d) if y > 0 && d >= PLAYER_JUMP_THRESHOLD => Player::Jumping(moving),
        // this probably won't work if the platforms are far apart,
        // which is weird to begin with and only happen in very rare place (e.g. Haven)
        (_, y, d) if y < 0 && d >= PLAYER_VERTICAL_MOVE_THRESHOLD => Player::Falling(moving),
        _ => {
            debug!(
                target: "player",
                "reached {:?} with actual position {:?}",
                dest, cur_pos
            );
            state.last_moving_state = None;
            let last_known_direction = state.last_known_direction;
            on_action(
                state,
                |action| match action {
                    PlayerAction::Fixed(action) => {
                        on_fixed_action(last_known_direction, action, moving)
                    }
                    PlayerAction::SolveRune => {
                        Some((Player::SolvingRune(PlayerSolvingRune::default()), false))
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
    const USE_KEY_AT_Y_DOUBLE_JUMP_THRESHOLD: i32 = 0;
    const USE_KEY_AT_Y_PROXIMITY_THRESHOLD: i32 = 2;
    const ADJUSTING_SHORT_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT - 2;

    fn on_fixed_action(
        last_known_direction: ActionKeyDirection,
        action: Action,
        y_distance: i32,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            Action::Key(ActionKey {
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            }) => {
                if !moving.completed || y_distance > USE_KEY_AT_Y_DOUBLE_JUMP_THRESHOLD {
                    return None;
                }
                if matches!(direction, ActionKeyDirection::Any) || direction == last_known_direction
                {
                    Some((
                        Player::DoubleJumping(
                            moving.timeout(Timeout::default()).completed(false),
                            true,
                            false,
                        ),
                        false,
                    ))
                } else {
                    Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false))
                }
            }
            Action::Key(ActionKey {
                with: ActionKeyWith::Any,
                ..
            }) => (moving.completed && y_distance <= USE_KEY_AT_Y_PROXIMITY_THRESHOLD)
                .then_some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
            Action::Key(ActionKey {
                with: ActionKeyWith::Stationary,
                ..
            })
            | Action::Move { .. } => None,
        }
    }

    let last_known_direction = state.last_known_direction;
    let (x_distance, x_direction) = x_distance_direction(&moving.dest, &cur_pos);
    let (y_distance, y_direction) = y_distance_direction(&moving.dest, &cur_pos);
    if x_distance >= DOUBLE_JUMP_THRESHOLD {
        state.use_immediate_control_flow = true;
        return Player::Moving(moving.dest, moving.exact);
    }
    if y_direction < 0
        && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
        && state.is_stationary
        && !matches!(
            state.last_moving_state,
            Some(PlayerLastMovingState::Falling)
        )
        && !moving.timeout.started
    {
        return Player::Falling(moving.pos(cur_pos));
    }

    if !moving.timeout.started {
        state.use_immediate_control_flow = true;
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT,
        Player::Adjusting,
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
        }),
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
                |action| match action {
                    PlayerAction::Fixed(action) => {
                        on_fixed_action(last_known_direction, action, y_distance, moving)
                    }
                    PlayerAction::SolveRune => None,
                },
                || {
                    if !moving.completed {
                        Player::Adjusting(moving)
                    } else {
                        Player::Adjusting(moving.timeout_current(PLAYER_MOVE_TIMEOUT))
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
    forced: bool,
    require_stationary: bool,
) -> Player {
    const DOUBLE_JUMP_USE_KEY_X_PROXIMITY_THRESHOLD: i32 = DOUBLE_JUMP_THRESHOLD;
    const DOUBLE_JUMP_USE_KEY_Y_PROXIMITY_THRESHOLD: i32 = 10;
    const DOUBLE_JUMP_GRAPPLING_THRESHOLD: i32 = 4;
    const DOUBLE_JUMPED_FORCE_THRESHOLD: i32 = 3;

    fn on_fixed_action(
        forced: bool,
        action: Action,
        x_distance: i32,
        y_distance: i32,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            // ignore proximity check when it is forced to double jumped
            // this indicates the player is already near the destination
            Action::Key(ActionKey {
                with: ActionKeyWith::DoubleJump | ActionKeyWith::Any,
                ..
            }) => (moving.completed
                && ((!moving.exact
                    && x_distance <= DOUBLE_JUMP_USE_KEY_X_PROXIMITY_THRESHOLD
                    && y_distance <= DOUBLE_JUMP_USE_KEY_Y_PROXIMITY_THRESHOLD)
                    || forced))
                .then_some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
            Action::Key(ActionKey {
                with: ActionKeyWith::Stationary,
                ..
            })
            | Action::Move { .. } => None,
        }
    }

    let x_changed = (cur_pos.x - moving.pos.x).abs();
    let (x_distance, x_direction) = x_distance_direction(&moving.dest, &cur_pos);
    let (y_distance, y_direction) = y_distance_direction(&moving.dest, &cur_pos);
    if y_direction < 0
        && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
        && state.is_stationary
        && !matches!(
            state.last_moving_state,
            Some(PlayerLastMovingState::Falling)
        )
        && !moving.timeout.started
    {
        return Player::Falling(moving.pos(cur_pos));
    }

    if !moving.timeout.started {
        if require_stationary && !state.is_stationary {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            return Player::DoubleJumping(moving.pos(cur_pos), forced, require_stationary);
        }
        state.last_moving_state = Some(PlayerLastMovingState::DoubleJumping);
        state.use_immediate_control_flow = true;
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT * 2,
        |moving| Player::DoubleJumping(moving, forced, require_stationary),
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
        }),
        |mut moving| {
            if !moving.completed {
                if !forced {
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
                }
                if (!forced && x_distance >= DOUBLE_JUMP_THRESHOLD)
                    || x_changed <= DOUBLE_JUMPED_FORCE_THRESHOLD
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
                |action| match action {
                    PlayerAction::Fixed(action) => {
                        on_fixed_action(forced, action, x_distance, y_distance, moving)
                    }
                    PlayerAction::SolveRune => None,
                },
                || {
                    if moving.completed
                        && !forced
                        && x_distance <= DOUBLE_JUMP_GRAPPLING_THRESHOLD
                        && y_direction > 0
                    {
                        debug!(target: "player", "performs grappling on double jump");
                        Player::Grappling(moving.completed(false).timeout(Timeout::default()))
                    } else if moving.completed && moving.timeout.current >= PLAYER_MOVE_TIMEOUT {
                        Player::Moving(moving.dest, moving.exact)
                    } else {
                        Player::DoubleJumping(moving, forced, require_stationary)
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
    const GRAPPLING_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 10;
    const GRAPPLING_STOPPING_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 3;
    const GRAPPLING_STOPPING_THRESHOLD: i32 = 2;

    let key = state.grappling_key;
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
            let (distance, direction) = y_distance_direction(&moving.dest, &moving.pos);
            if moving.timeout.current >= PLAYER_MOVE_TIMEOUT && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout_current(GRAPPLING_TIMEOUT);
            }
            if !moving.completed {
                if direction <= 0 || distance <= GRAPPLING_STOPPING_THRESHOLD {
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
    state: &mut PlayerState,
    cur_pos: Point,
    moving: PlayerMoving,
) -> Player {
    const UP_JUMP_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 2;
    const UP_JUMPED_THRESHOLD: i32 = 4;

    if !moving.timeout.started {
        state.use_immediate_control_flow = true;
    }

    let y_changed = (cur_pos.y - moving.pos.y).abs();
    let (x_distance, _) = x_distance_direction(&moving.dest, &cur_pos);
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
                if let Some(key) = state.upjump_key {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                } else if y_changed <= UP_JUMPED_THRESHOLD {
                    // spamming space until the player y changes
                    // above a threshold as sending space twice
                    // doesn't work
                    let _ = context.keys.send(KeyKind::Space);
                } else {
                    moving = moving.completed(true);
                }
            } else if x_distance >= ADJUSTING_MEDIUM_THRESHOLD
                && moving.timeout.current >= PLAYER_MOVE_TIMEOUT
            {
                moving = moving.timeout_current(UP_JUMP_TIMEOUT);
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
    const TIMEOUT_EARLY_THRESHOLD: i32 = -2;

    let y_changed = cur_pos.y - moving.pos.y;
    let (x_distance, _) = x_distance_direction(&moving.dest, &cur_pos);
    if !moving.timeout.started {
        state.last_moving_state = Some(PlayerLastMovingState::Falling);
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        PLAYER_MOVE_TIMEOUT,
        |moving| {
            let _ = context.keys.send_down(KeyKind::Down);
            let _ = context.keys.send(KeyKind::Space);
            Player::Falling(moving)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Down);
        }),
        |mut moving| {
            if x_distance >= ADJUSTING_MEDIUM_THRESHOLD && y_changed <= TIMEOUT_EARLY_THRESHOLD {
                moving = moving.timeout_current(PLAYER_MOVE_TIMEOUT);
            }
            Player::Falling(moving)
        },
        ChangeAxis::Vertical,
    )
}

fn update_unstucking_context(
    context: &Context,
    cur_pos: Point,
    timeout: Timeout,
    to_right: bool,
) -> Player {
    const Y_IGNORE_THRESHOLD: i32 = 15;

    let Minimap::Idle(idle) = context.minimap else {
        return Player::Detecting;
    };
    let y = idle.bbox.height - cur_pos.y;

    update_timeout(
        timeout,
        PLAYER_MOVE_TIMEOUT,
        |timeout| {
            if y <= Y_IGNORE_THRESHOLD {
                return Player::Unstucking(timeout);
            }
            if to_right {
                let _ = context.keys.send_down(KeyKind::Right);
            } else {
                let _ = context.keys.send_down(KeyKind::Left);
            }
            let _ = context.keys.send(KeyKind::Esc);
            Player::Unstucking(timeout)
        },
        || {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            Player::Detecting
        },
        |timeout| {
            if y > Y_IGNORE_THRESHOLD {
                let _ = context.keys.send(KeyKind::Space);
            }
            Player::Unstucking(timeout)
        },
    )
}

fn update_solving_rune_context(
    context: &Context,
    detector: &mut impl Detector,
    state: &mut PlayerState,
    solving_rune: PlayerSolvingRune,
) -> Player {
    const RUNE_COOLDOWN_TIMEOUT: u32 = 305; // around 10 secs (cooldown or redetect)
    const SOLVE_RUNE_TIMEOUT: u32 = RUNE_COOLDOWN_TIMEOUT + 100;
    const DETECT_RUNE_ARROWS_INTERVAL: u32 = 35;
    const PRESS_KEY_INTERVAL: u32 = 10;
    const MAX_FAILED_COUNT: usize = 2;

    fn validate_rune_solved(
        context: &Context,
        solving_rune: PlayerSolvingRune,
        timeout: Timeout,
    ) -> Player {
        if timeout.current == 0 || timeout.current % RUNE_COOLDOWN_TIMEOUT != 0 {
            return Player::SolvingRune(PlayerSolvingRune {
                timeout,
                ..solving_rune
            });
        }
        if matches!(context.buffs[RUNE_BUFF_POSITION], Buff::HasBuff) {
            Player::Idle
        } else {
            let failed_count = solving_rune.failed_count + 1;
            Player::SolvingRune(PlayerSolvingRune {
                timeout: Timeout {
                    current: 0,
                    started: failed_count >= MAX_FAILED_COUNT,
                },
                failed_count,
                ..PlayerSolvingRune::default()
            })
        }
    }

    let next = update_timeout(
        solving_rune.timeout,
        SOLVE_RUNE_TIMEOUT,
        |timeout| {
            let _ = context.keys.send(state.interact_key);
            Player::SolvingRune(PlayerSolvingRune {
                timeout,
                ..solving_rune
            })
        },
        || {
            // likely a spinning rune if the bot can't detect and timeout
            if solving_rune.failed_count < MAX_FAILED_COUNT {
                Player::SolvingRune(PlayerSolvingRune {
                    failed_count: solving_rune.failed_count + 1,
                    ..PlayerSolvingRune::default()
                })
            } else {
                Player::Idle
            }
        },
        |mut timeout| {
            if solving_rune.failed_count >= MAX_FAILED_COUNT {
                return Player::CashShopThenExit(Timeout::default(), false, false);
            }
            if solving_rune.validating {
                return validate_rune_solved(context, solving_rune, timeout);
            }
            if matches!(context.buffs[RUNE_BUFF_POSITION], Buff::HasBuff) {
                return Player::Idle;
            }
            if solving_rune.keys.is_none() {
                let keys = if timeout.current % DETECT_RUNE_ARROWS_INTERVAL == 0 {
                    detector.detect_rune_arrows().ok()
                } else {
                    None
                };
                return Player::SolvingRune(PlayerSolvingRune {
                    timeout,
                    keys,
                    ..solving_rune
                });
            }
            if timeout.current % PRESS_KEY_INTERVAL != 0 {
                return Player::SolvingRune(PlayerSolvingRune {
                    timeout,
                    ..solving_rune
                });
            }
            let keys = solving_rune.keys.unwrap();
            let _ = context.keys.send(keys[solving_rune.key_index]);
            let need_validate = solving_rune.key_index >= keys.len() - 1;
            if need_validate {
                timeout.current = 0;
            }
            Player::SolvingRune(PlayerSolvingRune {
                timeout,
                validating: need_validate,
                key_index: solving_rune.key_index + 1,
                ..solving_rune
            })
        },
    );
    on_action(
        state,
        |action| match action {
            PlayerAction::SolveRune => Some((next, matches!(next, Player::Idle))),
            PlayerAction::Fixed(_) => unreachable!(),
        },
        || next,
    )
}

#[inline]
fn on_action(
    state: &mut PlayerState,
    on_action_context: impl FnOnce(PlayerAction) -> Option<(Player, bool)>,
    on_default_context: impl FnOnce() -> Player,
) -> Player {
    if let Some(action) = state.priority_action.or(state.normal_action) {
        let Some((next, is_terminal)) = on_action_context(action) else {
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

#[inline]
fn x_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.x - cur_pos.x;
    let distance = direction.abs();
    (distance, direction)
}

#[inline]
fn y_distance_direction(dest: &Point, cur_pos: &Point) -> (i32, i32) {
    let direction = dest.y - cur_pos.y;
    let distance = direction.abs();
    (distance, direction)
}

#[inline]
fn update_moving_axis_timeout(
    prev_pos: Point,
    cur_pos: Point,
    timeout: Timeout,
    max_timeout: u32,
    axis: ChangeAxis,
) -> Timeout {
    if timeout.current >= max_timeout {
        return Timeout {
            current: max_timeout,
            ..timeout
        };
    }
    let moved = match axis {
        ChangeAxis::Vertical => cur_pos.y != prev_pos.y,
        ChangeAxis::Both { .. } => cur_pos.x != prev_pos.x || cur_pos.y != prev_pos.y,
    };
    Timeout {
        current: if moved { 0 } else { timeout.current },
        ..timeout
    }
}

#[inline]
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
        |timeout| on_started(moving.pos(cur_pos).timeout(timeout)),
        || {
            if let Some(callback) = on_timeout {
                callback();
            }
            Player::Moving(moving.dest, moving.exact)
        },
        |timeout| on_update(moving.pos(cur_pos).timeout(timeout)),
    )
}

#[inline]
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

#[inline]
fn update_state(
    context: &Context,
    detector: &mut impl Detector,
    state: &mut PlayerState,
) -> Option<Point> {
    const STATIONARY_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 2;

    let Minimap::Idle(idle) = &context.minimap else {
        return None;
    };
    let minimap_bbox = idle.bbox;
    let Ok(bbox) = detector.detect_player(minimap_bbox) else {
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
    if last_known_pos != pos {
        state.unstuck_counter = 0;
        state.is_stationary_timeout.current = 0;
    }
    state.is_stationary_timeout = update_moving_axis_timeout(
        last_known_pos,
        pos,
        Timeout {
            current: state.is_stationary_timeout.current + 1,
            ..state.is_stationary_timeout
        },
        STATIONARY_TIMEOUT,
        ChangeAxis::Both,
    );
    state.is_stationary = state.is_stationary_timeout.current > PLAYER_MOVE_TIMEOUT;
    state.last_known_pos = Some(pos);
    Some(pos)
}
