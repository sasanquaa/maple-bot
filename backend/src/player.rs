use std::{collections::HashMap, fmt::Debug, ops::Range};

use anyhow::Result;
use log::debug;
use opencv::core::{Point, Rect};
use platforms::windows::KeyKind;
use strum::Display;

use crate::{
    Position,
    array::Array,
    buff::Buff,
    context::{Context, Contextual, ControlFlow, MS_PER_TICK, RUNE_BUFF_POSITION},
    database::{Action, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove, KeyBinding},
    detect::Detector,
    minimap::Minimap,
    pathing::{PlatformWithNeighbors, find_points_with},
    task::{Task, Update, update_task_repeatable},
};

/// Maximum number of times adjusting or double jump states can be transitioned to without changing position
const UNSTUCK_TRACKER_THRESHOLD: u32 = 7;

/// Minimium y distance required to perform a fall and double jump/adjusting
const ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD: i32 = 8;

/// Minimum x distance from the destination required to spam small movement
const ADJUSTING_SHORT_THRESHOLD: i32 = 1;

/// Minimum x distance from the destination required to walk
const ADJUSTING_MEDIUM_THRESHOLD: i32 = 3;

const ADJUSTING_INTERMEDIATE_THRESHOLD: i32 = 7;

/// Minimum x distance from the destination required to perform a double jump
pub const DOUBLE_JUMP_THRESHOLD: i32 = 25;

/// Minimum x distance from the destination required to perform a double jump in auto mobbing
const DOUBLE_JUMP_AUTO_MOB_THRESHOLD: i32 = 12;

/// Maximum amount of ticks a change in x or y direction must be detected
const PLAYER_MOVE_TIMEOUT: u32 = 5;

const PLAYER_VERTICAL_MOVE_THRESHOLD: i32 = 4;

const PLAYER_GRAPPLING_THRESHOLD: i32 = 26;

pub const PLAYER_GRAPPLING_MAX_THRESHOLD: i32 = 41;

pub const PLAYER_JUMP_THRESHOLD: i32 = 7;

/// The number of times a reachable y must successfuly make the player moves to that exact y
/// Once the count is reached, it is considered "solidified" and guarantee that reachable y is always
/// a valid y (one that has platform and player can stand on)
const AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT: u32 = 4;

/// The acceptable y range above and below the detected mob position to match with a reachable y
const AUTO_MOB_REACHABLE_Y_THRESHOLD: i32 = 8;

const MAX_RUNE_FAILED_COUNT: u32 = 3;

#[derive(Debug, Default)]
pub struct PlayerState {
    /// A normal action requested by `Rotator`
    normal_action: Option<PlayerAction>,
    /// The id of the priority action provided by `Rotator`
    priority_action_id: u32,
    /// A priority action requested by `Rotator`, this action will override
    /// the normal action if it is in the middle of executing.
    priority_action: Option<PlayerAction>,
    pub rune_platforms_pathing: bool,
    pub rune_platforms_pathing_up_jump_only: bool,
    pub auto_mob_platforms_pathing: bool,
    pub auto_mob_platforms_pathing_up_jump_only: bool,
    /// The interact key
    pub interact_key: KeyKind,
    /// The RopeLift key
    pub grappling_key: KeyKind,
    /// The teleport key with `None` indicating double jump
    pub teleport_key: Option<KeyKind>,
    /// The up jump key with `None` indicating composite jump (Up arrow + Double Space)
    pub upjump_key: Option<KeyKind>,
    /// The cash shop key
    pub cash_shop_key: KeyKind,
    pub potion_key: KeyKind,
    pub use_potion_below_percent: Option<f32>,
    pub update_health_millis: Option<u64>,
    /// The player current health and max health
    pub health: Option<(u32, u32)>,
    /// The task to update health
    health_task: Option<Task<Result<(u32, u32)>>>,
    health_bar: Option<Rect>,
    /// The task for the health bar
    health_bar_task: Option<Task<Result<Rect>>>,
    /// Tracks if the player moved within a specified ticks to determine if the player is stationary
    is_stationary_timeout: Timeout,
    /// Whether the player is stationary
    is_stationary: bool,
    /// Approximates the player direction for using key
    last_known_direction: ActionKeyDirection,
    /// Tracks last destination points for displaying to UI
    /// Resets when all destinations are reached or in `Player::Idle`
    pub last_destinations: Option<Vec<Point>>,
    /// Last known position after each detection used for unstucking, also for displaying to UI
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
    last_movement: Option<PlayerLastMovement>,
    // TODO: 2 maps fr?
    /// Tracks `last_movement` to avoid looping when the position in normal action is not accurate
    /// Clears when in `Player::Idle` if there is no normal action
    last_movement_normal_map: HashMap<PlayerLastMovement, u32>,
    /// Tracks `last_movement` to avoid looping when the position in priority action is not accurate
    /// Clears when in `Player::Idle` if there is no priority action
    last_movement_priority_map: HashMap<PlayerLastMovement, u32>,
    /// Tracks a map of "reachable" y. A y is reachable if there is a platform player can stand on.
    auto_mob_reachable_y_map: HashMap<i32, u32>,
    /// The reachable y and also the key in `auto_mob_reachable_y_map`
    auto_mob_reachable_y: Option<i32>,
    /// Tracks whether movement-related actions do not change the player position after a while.
    /// Resets when a limit is reached (for unstucking) or position did change.
    unstuck_counter: u32,
    /// The number of consecutive times player transtioned to `Player::Unstucking`
    /// Resets when position did change
    unstuck_consecutive_counter: u32,
    /// Unstuck task for detecting settings
    unstuck_task: Option<Task<Result<bool>>>,
    /// Rune solving task
    rune_task: Option<Task<Result<[KeyKind; 4]>>>,
    rune_failed_count: u32,
    rune_cash_shop: bool,
    rune_validate_timeout: Option<Timeout>,
    /// A state to return to after stalling
    /// Resets when `Player::Stalling` timed out
    stalling_timeout_state: Option<Player>,
}

impl PlayerState {
    #[inline]
    pub fn reset_non_configuration_states(&mut self) {
        *self = PlayerState {
            normal_action: self.normal_action,
            priority_action_id: self.priority_action_id,
            priority_action: self.priority_action,
            rune_platforms_pathing: self.rune_platforms_pathing,
            rune_platforms_pathing_up_jump_only: self.rune_platforms_pathing_up_jump_only,
            auto_mob_platforms_pathing: self.auto_mob_platforms_pathing,
            auto_mob_platforms_pathing_up_jump_only: self.auto_mob_platforms_pathing_up_jump_only,
            interact_key: self.interact_key,
            grappling_key: self.grappling_key,
            teleport_key: self.teleport_key,
            upjump_key: self.upjump_key,
            cash_shop_key: self.cash_shop_key,
            potion_key: self.potion_key,
            use_potion_below_percent: self.use_potion_below_percent,
            update_health_millis: self.update_health_millis,
            reset_to_idle_next_update: true,
            ..PlayerState::default()
        };
    }

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
    pub fn priority_action_id(&self) -> Option<u32> {
        self.has_priority_action()
            .then_some(self.priority_action_id)
    }

    #[inline]
    pub fn has_priority_action(&self) -> bool {
        self.priority_action.is_some()
    }

    #[inline]
    pub fn has_rune_action(&self) -> bool {
        matches!(self.priority_action, Some(PlayerAction::SolveRune))
    }

    #[inline]
    pub fn set_priority_action(&mut self, id: u32, action: PlayerAction) {
        let _ = self.replace_priority_action(id, action);
    }

    #[inline]
    pub fn replace_priority_action(&mut self, id: u32, action: PlayerAction) -> Option<u32> {
        let prev_id = self.priority_action_id;
        self.reset_to_idle_next_update = true;
        self.priority_action_id = id;
        self.priority_action
            .replace(action)
            .is_some()
            .then_some(prev_id)
    }

    #[inline]
    pub fn is_validating_rune(&self) -> bool {
        self.rune_validate_timeout.is_some()
    }

    #[inline]
    pub fn abort_actions(&mut self) {
        self.reset_to_idle_next_update = true;
        self.priority_action = None;
        self.normal_action = None;
    }

    #[inline]
    fn has_auto_mob_action_only(&self) -> bool {
        matches!(self.normal_action, Some(PlayerAction::AutoMob(_))) && !self.has_priority_action()
    }

    #[inline]
    fn auto_mob_reachable_y_require_update(&self) -> bool {
        self.auto_mob_reachable_y.is_none_or(|y| {
            *self.auto_mob_reachable_y_map.get(&y).unwrap() < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT
        })
    }

    #[inline]
    fn falling_threshold(&self) -> i32 {
        if self.has_auto_mob_action_only() {
            AUTO_MOB_REACHABLE_Y_THRESHOLD
        } else {
            PLAYER_VERTICAL_MOVE_THRESHOLD
        }
    }

    #[inline]
    fn double_jump_threshold(&self) -> i32 {
        if self.has_auto_mob_action_only() {
            DOUBLE_JUMP_AUTO_MOB_THRESHOLD
        } else {
            DOUBLE_JUMP_THRESHOLD
        }
    }
}

/// The player previous movement-related conextual state.
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
enum PlayerLastMovement {
    Adjusting,
    DoubleJumping,
    Falling,
    Grappling,
    UpJumping,
    Jumping,
}

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
    position: Position,
    wait_after_move_ticks: u32,
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

/// A contextual state that stores moving-related data.
#[derive(Clone, Copy, Debug)]
pub struct PlayerMoving {
    /// The player's previous position and will be updated to current position
    /// after calling `update_moving_axis_timeout`
    pos: Point,
    /// The destination the player is moving to
    /// When `intermediates` is `Some(...)`, this could be an intermediate point
    dest: Point,
    /// Whether to allow adjusting to precise destination
    exact: bool,
    /// Whether the movement has completed
    completed: bool,
    /// Current timeout ticks for checking if the player position's changed
    timeout: Timeout,
    /// Intermediate points to move to before reaching the destination
    /// When `Some(...)`, the last point is the destination
    intermediates: Option<(usize, Array<(Point, bool), 16>)>,
}

/// Convenient implementations
impl PlayerMoving {
    #[inline]
    fn new(
        pos: Point,
        dest: Point,
        exact: bool,
        intermediates: Option<(usize, Array<(Point, bool), 16>)>,
    ) -> Self {
        Self {
            pos,
            dest,
            exact,
            completed: false,
            timeout: Timeout::default(),
            intermediates,
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

    #[inline]
    fn is_destination_intermediate(&self) -> bool {
        is_destination_intermediates(self.intermediates)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct PlayerUseKey {
    key: KeyBinding,
    count: u32,
    current_count: u32,
    direction: ActionKeyDirection,
    with: ActionKeyWith,
    wait_before: bool,
    wait_before_use_ticks: u32,
    wait_after_use_ticks: u32,
    wait_after: bool,
    timeout: Timeout,
}

impl PlayerUseKey {
    #[inline]
    fn new_from_action(action: PlayerAction) -> Self {
        match action {
            PlayerAction::Key(PlayerActionKey {
                key,
                count,
                direction,
                with,
                wait_before_use_ticks,
                wait_after_use_ticks,
                ..
            }) => Self {
                key,
                count,
                current_count: 0,
                direction,
                with,
                wait_before: false,
                wait_before_use_ticks,
                wait_after_use_ticks,
                wait_after: false,
                timeout: Timeout::default(),
            },
            PlayerAction::AutoMob(mob) => Self {
                key: mob.key,
                count: mob.count,
                current_count: 0,
                direction: ActionKeyDirection::Any,
                with: ActionKeyWith::Any,
                wait_before: false,
                wait_before_use_ticks: mob.wait_before_ticks,
                wait_after_use_ticks: mob.wait_after_ticks,
                timeout: Timeout::default(),
                wait_after: false,
            },
            PlayerAction::SolveRune | PlayerAction::Move { .. } => {
                unreachable!()
            }
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct PlayerSolvingRune {
    solve_timeout: Timeout,
    keys: Option<[KeyKind; 4]>,
    key_index: usize,
}

#[derive(Clone, Copy, Debug, Display)]
pub enum Player {
    Detecting,
    Idle,
    UseKey(PlayerUseKey),
    Moving(Point, bool, Option<(usize, Array<(Point, bool), 16>)>),
    Adjusting(PlayerMoving),
    DoubleJumping(PlayerMoving, bool, bool),
    Grappling(PlayerMoving),
    Jumping(PlayerMoving),
    UpJumping(PlayerMoving),
    Falling(PlayerMoving, Point),
    Unstucking(Timeout, Option<bool>),
    Stalling(Timeout, u32),
    SolvingRune(PlayerSolvingRune),
    CashShopThenExit(Timeout, bool, bool),
}

impl Contextual for Player {
    type Persistent = PlayerState;

    // 草草ｗｗ。。。
    // TODO: detect if a point is reachable after number of retries?
    // TODO: add unit tests
    fn update(
        self,
        context: &Context,
        detector: &impl Detector,
        state: &mut PlayerState,
    ) -> ControlFlow<Self> {
        if state.rune_cash_shop {
            let _ = context.keys.send_up(KeyKind::Up);
            let _ = context.keys.send_up(KeyKind::Down);
            let _ = context.keys.send_up(KeyKind::Left);
            let _ = context.keys.send_up(KeyKind::Right);
            state.rune_cash_shop = false;
            state.reset_to_idle_next_update = false;
            return ControlFlow::Next(Player::CashShopThenExit(Timeout::default(), false, false));
        }
        let cur_pos = if state.ignore_pos_update {
            state.last_known_pos
        } else {
            update_state(context, detector, state)
        };
        let Some(cur_pos) = cur_pos else {
            if let Some(next) = update_non_positional_context(self, context, detector, state, true)
            {
                return ControlFlow::Next(next);
            }
            let next = if !context.halting
                && let Minimap::Idle(idle) = context.minimap
            {
                if idle.partially_overlapping {
                    Player::Detecting
                } else {
                    Player::Unstucking(Timeout::default(), None)
                }
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
        let next = update_non_positional_context(contextual, context, detector, state, false)
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

#[inline]
fn update_non_positional_context(
    contextual: Player,
    context: &Context,
    detector: &impl Detector,
    state: &mut PlayerState,
    fail_to_detect: bool,
) -> Option<Player> {
    match contextual {
        Player::UseKey(use_key) => {
            (!fail_to_detect).then_some(update_use_key_context(context, state, use_key))
        }
        Player::Unstucking(timeout, has_settings) => Some(update_unstucking_context(
            context,
            detector,
            state,
            timeout,
            has_settings,
        )),
        Player::Stalling(timeout, max_timeout) => {
            (!fail_to_detect).then_some(update_stalling_context(state, timeout, max_timeout))
        }
        Player::SolvingRune(solving_rune) => (!fail_to_detect).then_some(
            update_solving_rune_context(context, detector, state, solving_rune),
        ),
        // TODO: Improve this?
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
                    update_with_timeout(
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
                    PlayerAction::AutoMob(_) | PlayerAction::Key(_) | PlayerAction::Move(_) => None,
                    PlayerAction::SolveRune => Some((next, false)),
                },
                || next,
            ))
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
                state.last_movement = Some(PlayerLastMovement::Jumping);
            }
            update_moving_axis_context(
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
            )
        }
        Player::Falling(moving, anchor) => {
            update_falling_context(context, state, cur_pos, moving, anchor)
        }
        Player::UseKey(_)
        | Player::Unstucking(_, _)
        | Player::Stalling(_, _)
        | Player::SolvingRune(_)
        | Player::CashShopThenExit(_, _, _) => unreachable!(),
    }
}

fn update_idle_context(context: &Context, state: &mut PlayerState, cur_pos: Point) -> Player {
    fn ensure_reachable_y(
        context: &Context,
        state: &mut PlayerState,
        player_pos: Point,
        mob_pos: Position,
    ) -> Player {
        if state.auto_mob_reachable_y_map.is_empty() {
            if !state.is_stationary {
                return Player::Idle;
            }
            debug!(target: "player", "auto mob initial reachable y {}", state.last_known_pos.unwrap().y);
            state.auto_mob_reachable_y_map.insert(
                state.last_known_pos.unwrap().y,
                AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT - 1,
            );
        }
        debug_assert!(!state.auto_mob_reachable_y_map.is_empty());
        let y = state
            .auto_mob_reachable_y_map
            .keys()
            .copied()
            .min_by_key(|y| (mob_pos.y - y).abs())
            .filter(|y| (mob_pos.y - y).abs() <= AUTO_MOB_REACHABLE_Y_THRESHOLD);
        let point = Point::new(mob_pos.x, y.unwrap_or(mob_pos.y));
        state.auto_mob_reachable_y = y;
        debug!(target: "player", "auto mob reachable y {:?} {:?}", y, state.auto_mob_reachable_y_map);
        if state.auto_mob_platforms_pathing {
            if let Minimap::Idle(idle) = context.minimap {
                if let Some((point, exact, index, array)) = find_points(
                    &idle.platforms,
                    player_pos,
                    point,
                    mob_pos.allow_adjusting,
                    state.auto_mob_platforms_pathing_up_jump_only,
                ) {
                    state.last_destinations =
                        Some(array.into_iter().map(|(point, _)| point).collect());
                    return Player::Moving(point, exact, Some((index, array)));
                }
            }
        }
        state.last_destinations = Some(vec![point]);
        Player::Moving(point, mob_pos.allow_adjusting, None)
    }

    fn on_player_action(
        context: &Context,
        state: &mut PlayerState,
        action: PlayerAction,
        cur_pos: Point,
    ) -> Option<(Player, bool)> {
        match action {
            PlayerAction::AutoMob(PlayerActionAutoMob { position, .. }) => {
                Some((ensure_reachable_y(context, state, cur_pos, position), false))
            }
            PlayerAction::Move(PlayerActionMove { position, .. }) => {
                debug!(target: "player", "handling move: {} {}", position.x, position.y);
                Some((
                    Player::Moving(
                        Point::new(position.x, position.y),
                        position.allow_adjusting,
                        None,
                    ),
                    false,
                ))
            }
            PlayerAction::Key(PlayerActionKey {
                position: Some(position),
                ..
            }) => {
                debug!(target: "player", "handling move: {} {}", position.x, position.y);
                Some((
                    Player::Moving(
                        Point::new(position.x, position.y),
                        position.allow_adjusting,
                        None,
                    ),
                    false,
                ))
            }
            PlayerAction::Key(PlayerActionKey {
                position: None,
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            }) => {
                if matches!(direction, ActionKeyDirection::Any)
                    || direction == state.last_known_direction
                {
                    Some((
                        Player::DoubleJumping(
                            PlayerMoving::new(cur_pos, cur_pos, false, None),
                            true,
                            true,
                        ),
                        false,
                    ))
                } else {
                    Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false))
                }
            }
            PlayerAction::Key(PlayerActionKey {
                position: None,
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            }) => Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
            PlayerAction::SolveRune => {
                if let Minimap::Idle(idle) = context.minimap {
                    if let Some(rune) = idle.rune {
                        if state.rune_platforms_pathing {
                            if !state.is_stationary {
                                return Some((Player::Idle, false));
                            }
                            if let Some((point, exact, index, array)) = find_points(
                                &idle.platforms,
                                cur_pos,
                                rune,
                                true,
                                state.rune_platforms_pathing_up_jump_only,
                            ) {
                                state.last_destinations =
                                    Some(array.into_iter().map(|(point, _)| point).collect());
                                return Some((
                                    Player::Moving(point, exact, Some((index, array))),
                                    false,
                                ));
                            }
                        }
                        state.last_destinations = Some(vec![rune]);
                        return Some((Player::Moving(rune, true, None), false));
                    }
                }
                Some((Player::Idle, true))
            }
        }
    }

    state.last_destinations = None;
    state.last_movement = None;
    if !state.has_normal_action() {
        state.auto_mob_reachable_y = None;
        state.last_movement_normal_map.clear();
    }
    if !state.has_priority_action() {
        state.last_movement_priority_map.clear();
    }
    let _ = context.keys.send_up(KeyKind::Up);
    let _ = context.keys.send_up(KeyKind::Down);
    let _ = context.keys.send_up(KeyKind::Left);
    let _ = context.keys.send_up(KeyKind::Right);

    on_action_mut_state(
        state,
        |state, action| on_player_action(context, state, action, cur_pos),
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

    let next = update_with_timeout(
        use_key.timeout,
        USE_KEY_TIMEOUT,
        |timeout| Player::UseKey(PlayerUseKey { timeout, ..use_key }),
        || Player::Idle,
        |timeout| {
            if !update_direction(context, state, timeout, use_key.direction) {
                return Player::UseKey(PlayerUseKey { timeout, ..use_key });
            }
            match use_key.with {
                ActionKeyWith::Any => (),
                ActionKeyWith::Stationary => {
                    if !state.is_stationary {
                        return Player::UseKey(PlayerUseKey { timeout, ..use_key });
                    }
                }
                ActionKeyWith::DoubleJump => {
                    if !matches!(state.last_movement, Some(PlayerLastMovement::DoubleJumping)) {
                        let pos = state.last_known_pos.unwrap();
                        return Player::DoubleJumping(
                            PlayerMoving::new(pos, pos, false, None),
                            true,
                            true,
                        );
                    }
                }
            }
            if !use_key.wait_before && use_key.wait_before_use_ticks > 0 {
                state.stalling_timeout_state = Some(Player::UseKey(PlayerUseKey {
                    timeout,
                    wait_before: true,
                    ..use_key
                }));
                return Player::Stalling(Timeout::default(), use_key.wait_before_use_ticks);
            }
            debug_assert!(use_key.current_count < use_key.count);
            debug_assert!(use_key.wait_before_use_ticks == 0 || use_key.wait_before);
            debug_assert!(state.stalling_timeout_state.is_none());
            let _ = context.keys.send(use_key.key.into());
            if !use_key.wait_after && use_key.wait_after_use_ticks > 0 {
                state.stalling_timeout_state = Some(Player::UseKey(PlayerUseKey {
                    timeout,
                    wait_after: true,
                    ..use_key
                }));
                return Player::Stalling(Timeout::default(), use_key.wait_after_use_ticks);
            }
            debug_assert!(use_key.wait_after_use_ticks == 0 || use_key.wait_after);
            debug_assert!(state.stalling_timeout_state.is_none());
            if use_key.current_count + 1 < use_key.count {
                return Player::UseKey(PlayerUseKey {
                    timeout: Timeout {
                        started: true,
                        ..Timeout::default()
                    },
                    wait_before: false,
                    wait_after: false,
                    current_count: use_key.current_count + 1,
                    ..use_key
                });
            }
            if state.has_auto_mob_action_only() && state.auto_mob_reachable_y_require_update() {
                return Player::Stalling(Timeout::default(), PLAYER_MOVE_TIMEOUT);
            }
            Player::Idle
        },
    );

    on_action(
        state,
        |action| match action {
            PlayerAction::AutoMob(_) | PlayerAction::Key(_) => {
                Some((next, matches!(next, Player::Idle)))
            }
            PlayerAction::Move(_) | PlayerAction::SolveRune => None,
        },
        || next,
    )
}

fn update_moving_context(
    state: &mut PlayerState,
    cur_pos: Point,
    dest: Point,
    exact: bool,
    intermediates: Option<(usize, Array<(Point, bool), 16>)>,
) -> Player {
    const ACTION_HORIZONTAL_STATE_REPEAT_COUNT: u32 = 20;
    const AUTO_MOB_HORIZONTAL_STATE_REPEAT_COUNT: u32 = 5;
    const ACTION_VERTICAL_STATE_REPEAT_COUNT: u32 = 8;
    const AUTO_MOB_VERTICAL_STATE_REPEAT_COUNT: u32 = 2;
    const PLAYER_UP_JUMP_THRESHOLD: i32 = 10;
    const PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD: Range<i32> = const {
        debug_assert!(PLAYER_JUMP_THRESHOLD < PLAYER_UP_JUMP_THRESHOLD);
        PLAYER_JUMP_THRESHOLD..PLAYER_UP_JUMP_THRESHOLD
    };

    /// Aborts the action when state starts looping.
    fn abort_action_on_state_repeat(next: Player, state: &mut PlayerState) -> Player {
        if let Some(last_moving_state) = state.last_movement {
            let count_max = match last_moving_state {
                PlayerLastMovement::Adjusting | PlayerLastMovement::DoubleJumping => {
                    if state.has_auto_mob_action_only() {
                        AUTO_MOB_HORIZONTAL_STATE_REPEAT_COUNT
                    } else {
                        ACTION_HORIZONTAL_STATE_REPEAT_COUNT
                    }
                }
                PlayerLastMovement::Falling
                | PlayerLastMovement::Grappling
                | PlayerLastMovement::UpJumping
                | PlayerLastMovement::Jumping => {
                    if state.has_auto_mob_action_only() {
                        AUTO_MOB_VERTICAL_STATE_REPEAT_COUNT
                    } else {
                        ACTION_VERTICAL_STATE_REPEAT_COUNT
                    }
                }
            };
            let count_map = if state.has_priority_action() {
                &mut state.last_movement_priority_map
            } else {
                &mut state.last_movement_normal_map
            };
            let count = count_map.entry(last_moving_state).or_insert(0);
            debug_assert!(*count < count_max);
            *count += 1;
            let count = *count;
            debug!(target: "player", "last movement {:?}", count_map);
            if count >= count_max {
                debug!(target: "player", "abort action due to repeated state");
                if state.has_priority_action() {
                    state.priority_action = None;
                } else if state.has_normal_action() {
                    state.normal_action = None;
                }
                return Player::Idle;
            }
        }
        next
    }

    fn on_player_action(
        last_known_direction: ActionKeyDirection,
        action: PlayerAction,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            PlayerAction::Move(PlayerActionMove {
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
            PlayerAction::Key(PlayerActionKey {
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
            PlayerAction::AutoMob(_)
            | PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            }) => Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
            PlayerAction::SolveRune => {
                Some((Player::SolvingRune(PlayerSolvingRune::default()), false))
            }
        }
    }

    debug_assert!(intermediates.is_none() || intermediates.unwrap().0 > 0);
    state.use_immediate_control_flow = true;
    state.unstuck_counter += 1;
    if state.unstuck_counter >= UNSTUCK_TRACKER_THRESHOLD {
        state.unstuck_counter = 0;
        return Player::Unstucking(Timeout::default(), None);
    }

    let (x_distance, _) = x_distance_direction(dest, cur_pos);
    let (y_distance, y_direction) = y_distance_direction(dest, cur_pos);
    let moving = PlayerMoving::new(cur_pos, dest, exact, intermediates);

    match (x_distance, y_direction, y_distance) {
        (d, _, _) if d >= state.double_jump_threshold() => {
            abort_action_on_state_repeat(Player::DoubleJumping(moving, false, false), state)
        }
        (d, _, _)
            if (is_destination_intermediates(intermediates)
                && d >= ADJUSTING_INTERMEDIATE_THRESHOLD)
                || (exact && d >= ADJUSTING_SHORT_THRESHOLD)
                || (!exact && d >= ADJUSTING_MEDIUM_THRESHOLD) =>
        {
            abort_action_on_state_repeat(Player::Adjusting(moving), state)
        }
        // y > 0: cur_pos is below dest
        // y < 0: cur_pos is above of dest
        (_, y, d) if y > 0 && d >= PLAYER_GRAPPLING_THRESHOLD => {
            abort_action_on_state_repeat(Player::Grappling(moving), state)
        }
        (_, y, d)
            if y > 0
                && (d >= PLAYER_UP_JUMP_THRESHOLD
                    || PLAYER_JUMP_TO_UP_JUMP_RANGE_THRESHOLD.contains(&d)) =>
        {
            abort_action_on_state_repeat(Player::UpJumping(moving), state)
        }
        (_, y, d) if y > 0 && d >= PLAYER_JUMP_THRESHOLD => {
            abort_action_on_state_repeat(Player::Jumping(moving), state)
        }
        // this probably won't work if the platforms are far apart,
        // which is weird to begin with and only happen in very rare place (e.g. Haven)
        (_, y, d) if y < 0 && d >= state.falling_threshold() => {
            abort_action_on_state_repeat(Player::Falling(moving, cur_pos), state)
        }
        _ => {
            debug!(
                target: "player",
                "reached {:?} with actual position {:?}",
                dest, cur_pos
            );
            state.last_movement = None;
            if let Some((index, points)) = intermediates {
                if index < points.len() {
                    state.unstuck_counter = 0;
                    if state.has_priority_action() {
                        state.last_movement_priority_map.clear();
                    } else if state.has_normal_action() {
                        state.last_movement_normal_map.clear();
                    }
                    let (dest, exact) = points[index];
                    return Player::Moving(dest, exact, Some((index + 1, points)));
                }
            }
            state.last_destinations = None;
            let last_known_direction = state.last_known_direction;
            on_action(
                state,
                |action| on_player_action(last_known_direction, action, moving),
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
    const USE_KEY_AT_X_PROXIMITY_AUTO_MOB_THRESHOLD: i32 = 10;
    const USE_KEY_AT_Y_PROXIMITY_THRESHOLD: i32 = 2;
    const USE_KEY_AT_Y_PROXIMITY_AUTO_MOB_THRESHOLD: i32 = 5;
    const ADJUSTING_SHORT_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT - 2;

    fn on_player_action(
        context: &Context,
        state: &mut PlayerState,
        action: PlayerAction,
        x_distance: i32,
        y_distance: i32,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            }) => {
                if !moving.completed || y_distance > USE_KEY_AT_Y_DOUBLE_JUMP_THRESHOLD {
                    return None;
                }
                if matches!(direction, ActionKeyDirection::Any)
                    || direction == state.last_known_direction
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
            PlayerAction::AutoMob(_)
            | PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::Any,
                ..
            }) => {
                if state.has_auto_mob_action_only()
                    && x_distance <= USE_KEY_AT_X_PROXIMITY_AUTO_MOB_THRESHOLD
                    && y_distance <= USE_KEY_AT_Y_PROXIMITY_AUTO_MOB_THRESHOLD
                {
                    let _ = context.keys.send_up(KeyKind::Right);
                    let _ = context.keys.send_up(KeyKind::Left);
                    return Some((Player::UseKey(PlayerUseKey::new_from_action(action)), false));
                }
                (moving.completed && y_distance <= USE_KEY_AT_Y_PROXIMITY_THRESHOLD)
                    .then_some((Player::UseKey(PlayerUseKey::new_from_action(action)), false))
            }
            PlayerAction::SolveRune
            | PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::Stationary,
                ..
            })
            | PlayerAction::Move { .. } => None,
        }
    }

    let (x_distance, x_direction) = x_distance_direction(moving.dest, cur_pos);
    let (y_distance, y_direction) = y_distance_direction(moving.dest, cur_pos);
    if x_distance >= state.double_jump_threshold() {
        state.use_immediate_control_flow = true;
        return Player::Moving(moving.dest, moving.exact, moving.intermediates);
    }
    // TODO: ????????????
    if y_direction < 0
        && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
        && x_distance >= ADJUSTING_MEDIUM_THRESHOLD
        && state.is_stationary
        && !matches!(state.last_movement, Some(PlayerLastMovement::Falling))
        && !moving.timeout.started
        && moving.intermediates.is_none()
    {
        return Player::Falling(moving.pos(cur_pos), cur_pos);
    }

    if !moving.timeout.started {
        state.use_immediate_control_flow = true;
        state.last_movement = Some(PlayerLastMovement::Adjusting);
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
            if moving.is_destination_intermediate() {
                return if x_distance < ADJUSTING_INTERMEDIATE_THRESHOLD {
                    Player::Adjusting(moving.timeout_current(PLAYER_MOVE_TIMEOUT))
                } else {
                    Player::Adjusting(moving)
                };
            }
            on_action_mut_state(
                state,
                |state, action| {
                    on_player_action(context, state, action, x_distance, y_distance, moving)
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
    // Note: even in auto mob, also use the non-auto mob threshold
    const DOUBLE_JUMP_USE_KEY_X_PROXIMITY_THRESHOLD: i32 = DOUBLE_JUMP_THRESHOLD;
    const DOUBLE_JUMP_USE_KEY_Y_PROXIMITY_THRESHOLD: i32 = 10;
    const DOUBLE_JUMP_GRAPPLING_THRESHOLD: i32 = 4;
    const DOUBLE_JUMP_FORCE_THRESHOLD: i32 = 3;

    fn on_player_action(
        forced: bool,
        action: PlayerAction,
        x_distance: i32,
        y_distance: i32,
        moving: PlayerMoving,
    ) -> Option<(Player, bool)> {
        match action {
            // ignore proximity check when it is forced to double jumped
            // this indicates the player is already near the destination
            PlayerAction::AutoMob(_)
            | PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::DoubleJump | ActionKeyWith::Any,
                ..
            }) => (moving.completed
                && ((!moving.exact
                    && x_distance <= DOUBLE_JUMP_USE_KEY_X_PROXIMITY_THRESHOLD
                    && y_distance <= DOUBLE_JUMP_USE_KEY_Y_PROXIMITY_THRESHOLD)
                    || forced))
                .then_some((Player::UseKey(PlayerUseKey::new_from_action(action)), false)),
            PlayerAction::SolveRune
            | PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::Stationary,
                ..
            })
            | PlayerAction::Move { .. } => None,
        }
    }

    debug_assert!(moving.timeout.started || !moving.completed);

    let ignore_grappling = (state.has_auto_mob_action_only()
        && state.auto_mob_platforms_pathing
        && state.auto_mob_platforms_pathing_up_jump_only)
        || (state.has_rune_action()
            && state.rune_platforms_pathing
            && state.rune_platforms_pathing_up_jump_only);
    let x_changed = (cur_pos.x - moving.pos.x).abs();
    let (x_distance, x_direction) = x_distance_direction(moving.dest, cur_pos);
    let (y_distance, y_direction) = y_distance_direction(moving.dest, cur_pos);
    if y_direction < 0
        && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
        && state.is_stationary
        && !matches!(state.last_movement, Some(PlayerLastMovement::Falling))
        && !moving.timeout.started
        && moving.intermediates.is_none()
    {
        return Player::Falling(moving.pos(cur_pos), cur_pos);
    }

    if !moving.timeout.started {
        if require_stationary && !state.is_stationary {
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            return Player::DoubleJumping(moving.pos(cur_pos), forced, require_stationary);
        }
        state.last_movement = Some(PlayerLastMovement::DoubleJumping);
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
                // mage teleportation requires a direction
                if !forced || state.teleport_key.is_some() {
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
                            if state.teleport_key.is_some() {
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
                if (!forced && x_distance >= state.double_jump_threshold())
                    || x_changed <= DOUBLE_JUMP_FORCE_THRESHOLD
                {
                    let _ = context
                        .keys
                        .send(state.teleport_key.unwrap_or(KeyKind::Space));
                } else {
                    let _ = context.keys.send_up(KeyKind::Right);
                    let _ = context.keys.send_up(KeyKind::Left);
                    moving = moving.completed(true);
                }
            }
            on_action(
                state,
                |action| on_player_action(forced, action, x_distance, y_distance, moving),
                || {
                    if !ignore_grappling
                        && !forced
                        && moving.completed
                        && x_distance <= DOUBLE_JUMP_GRAPPLING_THRESHOLD
                        && y_direction > 0
                    {
                        debug!(target: "player", "performs grappling on double jump");
                        Player::Grappling(moving.completed(false).timeout(Timeout::default()))
                    } else if moving.completed && moving.timeout.current >= PLAYER_MOVE_TIMEOUT {
                        Player::Moving(moving.dest, moving.exact, moving.intermediates)
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

    if !moving.timeout.started {
        state.last_movement = Some(PlayerLastMovement::Grappling);
    }

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
            let (distance, direction) = y_distance_direction(moving.dest, moving.pos);
            if moving.timeout.current >= PLAYER_MOVE_TIMEOUT && x_changed {
                // during double jump and grappling failed
                moving = moving.timeout_current(GRAPPLING_TIMEOUT);
            }
            if !moving.completed {
                if direction <= 0 || distance <= GRAPPLING_STOPPING_THRESHOLD {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                }
            } else if state.has_auto_mob_action_only()
                || moving.timeout.current >= GRAPPLING_STOPPING_TIMEOUT
            {
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
    const UP_JUMP_SPAM_DELAY: u32 = 7;
    const UP_JUMP_TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 2;
    const UP_JUMPED_THRESHOLD: i32 = 5;

    if !moving.timeout.started {
        if state.has_auto_mob_action_only() {
            if let Minimap::Idle(idle) = context.minimap {
                for portal in idle.portals {
                    if portal.x <= cur_pos.x
                        && cur_pos.x < portal.x + portal.width
                        && portal.y >= cur_pos.y
                        && portal.y - portal.height < cur_pos.y
                    {
                        debug!(target: "player", "abort auto mob action due to potential map moving");
                        state.normal_action = None;
                        return Player::Idle;
                    }
                }
            }
        }
        state.use_immediate_control_flow = true;
        state.last_movement = Some(PlayerLastMovement::UpJumping);
    }

    let y_changed = (cur_pos.y - moving.pos.y).abs();
    let (x_distance, _) = x_distance_direction(moving.dest, cur_pos);
    let key = state.upjump_key;
    update_moving_axis_context(
        moving,
        cur_pos,
        UP_JUMP_TIMEOUT,
        |moving| {
            if key.is_none() {
                let _ = context.keys.send_down(KeyKind::Up);
                let _ = context.keys.send(KeyKind::Space);
            }
            Player::UpJumping(moving)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Up);
        }),
        |mut moving| {
            if !moving.completed {
                if let Some(key) = key {
                    let _ = context.keys.send(key);
                    moving = moving.completed(true);
                } else if y_changed <= UP_JUMPED_THRESHOLD {
                    // spamming space until the player y changes
                    // above a threshold as sending space twice
                    // doesn't work
                    if moving.timeout.total >= UP_JUMP_SPAM_DELAY {
                        let _ = context.keys.send(KeyKind::Space);
                    }
                } else {
                    moving = moving.completed(true);
                }
            } else if state.has_auto_mob_action_only()
                || (x_distance >= ADJUSTING_MEDIUM_THRESHOLD
                    && moving.timeout.current >= PLAYER_MOVE_TIMEOUT)
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
    anchor: Point,
) -> Player {
    const TIMEOUT: u32 = PLAYER_MOVE_TIMEOUT * 2;
    const TIMEOUT_EARLY_THRESHOLD: i32 = -4;

    let y_changed = cur_pos.y - anchor.y;
    let (x_distance, _) = x_distance_direction(moving.dest, cur_pos);
    if !moving.timeout.started {
        state.last_movement = Some(PlayerLastMovement::Falling);
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            if state.is_stationary {
                let _ = context.keys.send_down(KeyKind::Down);
                let _ = context.keys.send(KeyKind::Space);
            }
            Player::Falling(moving, anchor)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Down);
        }),
        |mut moving| {
            if x_distance >= ADJUSTING_MEDIUM_THRESHOLD && y_changed <= TIMEOUT_EARLY_THRESHOLD {
                moving = moving.timeout_current(TIMEOUT);
            }
            Player::Falling(moving, anchor)
        },
        ChangeAxis::Vertical,
    )
}

fn update_unstucking_context(
    context: &Context,
    detector: &impl Detector,
    state: &mut PlayerState,
    timeout: Timeout,
    has_settings: Option<bool>,
) -> Player {
    const Y_IGNORE_THRESHOLD: i32 = 18;
    // what is gamba mode? i am disappointed if you don't know
    const GAMBA_MODE_COUNT: u32 = 2;
    /// Random threshold to choose unstucking direction
    const X_TO_RIGHT_THRESHOLD: i32 = 10;

    let Minimap::Idle(idle) = context.minimap else {
        return Player::Detecting;
    };

    if !timeout.started {
        if state.unstuck_consecutive_counter + 1 < GAMBA_MODE_COUNT && has_settings.is_none() {
            let detector = detector.clone();
            let Update::Complete(Ok(has_settings)) =
                update_task_repeatable(0, &mut state.unstuck_task, move || {
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
        PLAYER_MOVE_TIMEOUT,
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
                let _ = context.keys.send(KeyKind::Space);
            }
            Player::Unstucking(timeout, has_settings)
        },
    )
}

fn update_stalling_context(state: &mut PlayerState, timeout: Timeout, max_timeout: u32) -> Player {
    let update = |timeout| Player::Stalling(timeout, max_timeout);
    let next = update_with_timeout(
        timeout,
        max_timeout,
        update,
        || {
            if state.stalling_timeout_state.is_none()
                && state.has_auto_mob_action_only()
                && state.auto_mob_reachable_y_require_update()
            {
                if !state.is_stationary {
                    return Player::Stalling(Timeout::default(), max_timeout);
                }
                // state.last_known_pos is explicitly used instead of state.auto_mob_reachable_y
                // because they might not be the same
                if let Some(pos) = state.last_known_pos {
                    if state.auto_mob_reachable_y.is_some_and(|y| y != pos.y) {
                        let y = state.auto_mob_reachable_y.unwrap();
                        let count = state.auto_mob_reachable_y_map.get_mut(&y).unwrap();
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            state.auto_mob_reachable_y_map.remove(&y);
                            state.auto_mob_reachable_y = None;
                        }
                    }
                    let count = state.auto_mob_reachable_y_map.entry(pos.y).or_insert(0);
                    if *count < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT {
                        *count += 1;
                    }
                    debug_assert!(*count <= AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT);
                    debug!(target: "player", "auto mob additional reachable y {} / {}", pos.y, count);
                }
            }
            state.stalling_timeout_state.take().unwrap_or(Player::Idle)
        },
        update,
    );
    on_action(
        state,
        |_| Some((next, matches!(next, Player::Idle))),
        || next,
    )
}

fn update_solving_rune_context(
    context: &Context,
    detector: &impl Detector,
    state: &mut PlayerState,
    solving_rune: PlayerSolvingRune,
) -> Player {
    const RUNE_SOLVING_TIMEOUT: u32 = 155;
    const PRESS_KEY_INTERVAL: u32 = 10;

    debug_assert!(state.rune_validate_timeout.is_none());
    debug_assert!(state.rune_failed_count < MAX_RUNE_FAILED_COUNT);
    debug_assert!(!state.rune_cash_shop);
    let detector = detector.clone();
    let next = update_with_timeout(
        solving_rune.solve_timeout,
        RUNE_SOLVING_TIMEOUT,
        |timeout| {
            let _ = context.keys.send(state.interact_key);
            Player::SolvingRune(PlayerSolvingRune {
                solve_timeout: timeout,
                ..solving_rune
            })
        },
        || {
            // likely a spinning rune if the bot can't detect and timeout
            state.rune_failed_count += 1;
            if state.rune_failed_count >= MAX_RUNE_FAILED_COUNT {
                state.rune_failed_count = 0;
                state.rune_cash_shop = true;
            }
            Player::Idle
        },
        |timeout| {
            if solving_rune.keys.is_none() {
                let Update::Complete(Ok(keys)) =
                    update_task_repeatable(1000, &mut state.rune_task, move || {
                        detector.detect_rune_arrows()
                    })
                else {
                    return Player::SolvingRune(PlayerSolvingRune {
                        solve_timeout: timeout,
                        ..solving_rune
                    });
                };
                return Player::SolvingRune(PlayerSolvingRune {
                    // reset current timeout for pressing keys
                    solve_timeout: Timeout {
                        current: 1, // starts at 1 instead of 0 to avoid immediate key press
                        total: 1,
                        started: true,
                    },
                    keys: Some(keys),
                    ..solving_rune
                });
            }
            if timeout.current % PRESS_KEY_INTERVAL != 0 {
                return Player::SolvingRune(PlayerSolvingRune {
                    solve_timeout: timeout,
                    ..solving_rune
                });
            }
            debug_assert!(solving_rune.key_index != 0 || timeout.current == PRESS_KEY_INTERVAL);
            debug_assert!(
                solving_rune
                    .keys
                    .is_some_and(|keys| solving_rune.key_index < keys.len())
            );
            let keys = solving_rune.keys.unwrap();
            let key_index = solving_rune.key_index;
            let _ = context.keys.send(keys[key_index]);
            let key_index = solving_rune.key_index + 1;
            if key_index >= keys.len() {
                state.rune_validate_timeout = Some(Timeout::default());
                Player::Idle
            } else {
                Player::SolvingRune(PlayerSolvingRune {
                    solve_timeout: timeout,
                    key_index,
                    ..solving_rune
                })
            }
        },
    );

    on_action(
        state,
        |action| match action {
            PlayerAction::SolveRune => Some((next, matches!(next, Player::Idle))),
            PlayerAction::AutoMob(_) | PlayerAction::Key(_) | PlayerAction::Move(_) => {
                unreachable!()
            }
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
    on_action_mut_state(
        state,
        |_, action| on_action_context(action),
        on_default_context,
    )
}

#[inline]
fn on_action_mut_state(
    state: &mut PlayerState,
    on_action_context: impl FnOnce(&mut PlayerState, PlayerAction) -> Option<(Player, bool)>,
    on_default_context: impl FnOnce() -> Player,
) -> Player {
    if let Some(action) = state.priority_action.or(state.normal_action) {
        if let Some((next, is_terminal)) = on_action_context(state, action) {
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
                        state.unstuck_consecutive_counter = 0;
                    }
                    PlayerAction::Key(PlayerActionKey { position: None, .. }) => (),
                }
                if state.priority_action.is_some() {
                    state.priority_action = None;
                } else {
                    state.normal_action = None;
                }
            }
            return next;
        }
    }
    on_default_context()
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
/// an action will be performed or state can be transitioned. So timeout is used to retry
/// such action/state and to avoid looping in a single state forever. Or
/// for some contextual states to perform an action only after timing out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timeout {
    /// The current timeout tick.
    /// The timeout tick can be reset to 0 in the context of movement.
    current: u32,
    /// The total number of passed ticks. Useful when `current` can be reset.
    /// Currently only used for delaying upjumping
    total: u32,
    /// Inidcates whether the timeout has started.
    started: bool,
}

#[inline]
fn update_with_timeout<T>(
    timeout: Timeout,
    max_timeout: u32,
    on_started: impl FnOnce(Timeout) -> T,
    on_timeout: impl FnOnce() -> T,
    on_update: impl FnOnce(Timeout) -> T,
) -> T {
    debug_assert!(max_timeout > 0, "max_timeout must be positive");
    debug_assert!(
        timeout.started || timeout == Timeout::default(),
        "started timeout in non-default state"
    );
    debug_assert!(
        timeout.current <= max_timeout,
        "current timeout tick larger than max_timeout"
    );

    match timeout {
        Timeout { started: false, .. } => on_started(Timeout {
            started: true,
            ..timeout
        }),
        Timeout { current, .. } if current >= max_timeout => on_timeout(),
        timeout => on_update(Timeout {
            current: timeout.current + 1,
            total: timeout.total + 1,
            ..timeout
        }),
    }
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
        return timeout;
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
    update_with_timeout(
        update_moving_axis_timeout(moving.pos, cur_pos, moving.timeout, max_timeout, axis),
        max_timeout,
        |timeout| on_started(moving.pos(cur_pos).timeout(timeout)),
        || {
            if let Some(callback) = on_timeout {
                callback();
            }
            Player::Moving(moving.dest, moving.exact, moving.intermediates)
        },
        |timeout| on_update(moving.pos(cur_pos).timeout(timeout)),
    )
}

#[inline]
fn reset_health(state: &mut PlayerState) {
    state.health = None;
    state.health_task = None;
    state.health_bar = None;
    state.health_bar_task = None;
}

#[inline]
fn update_rune_validating_state(context: &Context, state: &mut PlayerState) {
    const VALIDATE_TIMEOUT: u32 = 375;

    debug_assert!(state.rune_failed_count < MAX_RUNE_FAILED_COUNT);
    debug_assert!(!state.rune_cash_shop);
    state.rune_validate_timeout = state.rune_validate_timeout.and_then(|timeout| {
        update_with_timeout(
            timeout,
            VALIDATE_TIMEOUT,
            |timeout| Some(timeout),
            || {
                if matches!(context.buffs[RUNE_BUFF_POSITION], Buff::NoBuff) {
                    state.rune_failed_count += 1;
                    if state.rune_failed_count >= MAX_RUNE_FAILED_COUNT {
                        state.rune_failed_count = 0;
                        state.rune_cash_shop = true;
                    }
                } else {
                    state.rune_failed_count = 0;
                }
                None
            },
            |timeout| Some(timeout),
        )
    });
}

// TODO: This should be a PlayerAction?
#[inline]
fn update_health_state(context: &Context, detector: &impl Detector, state: &mut PlayerState) {
    if let Player::SolvingRune(_) = context.player {
        return;
    }
    if state.use_potion_below_percent.is_none() {
        reset_health(state);
        return;
    }
    let percentage = state.use_potion_below_percent.unwrap();
    let detector = detector.clone();
    let Some(health_bar) = state.health_bar else {
        let update = update_task_repeatable(1000, &mut state.health_bar_task, move || {
            detector.detect_player_health_bar()
        });
        if let Update::Complete(Ok(health_bar)) = update {
            state.health_bar = Some(health_bar);
        }
        return;
    };
    let Update::Complete(health) = update_task_repeatable(
        state.update_health_millis.unwrap_or(1000),
        &mut state.health_task,
        move || {
            let (current_bar, max_bar) =
                detector.detect_player_current_max_health_bars(health_bar)?;
            let health = detector.detect_player_health(current_bar, max_bar)?;
            debug!(target: "player", "health updated {:?}", health);
            Ok(health)
        },
    ) else {
        return;
    };
    state.health = health.ok();
    if let Some((current, max)) = state.health {
        let ratio = current as f32 / max as f32;
        if ratio <= percentage {
            let _ = context.keys.send(state.potion_key);
        }
    }
}

#[inline]
fn update_state(
    context: &Context,
    detector: &impl Detector,
    state: &mut PlayerState,
) -> Option<Point> {
    let Minimap::Idle(idle) = &context.minimap else {
        reset_health(state);
        return None;
    };
    let minimap_bbox = idle.bbox;
    let Ok(bbox) = detector.detect_player(minimap_bbox) else {
        reset_health(state);
        return None;
    };
    let tl = bbox.tl();
    let br = bbox.br();
    let x = ((tl.x + br.x) / 2) as f32 / idle.scale_w;
    let y = (minimap_bbox.height - br.y) as f32 / idle.scale_h;
    let pos = Point::new(x as i32, y as i32);
    let last_known_pos = state.last_known_pos.unwrap_or(pos);
    if last_known_pos != pos {
        state.unstuck_counter = 0;
        state.unstuck_consecutive_counter = 0;
        state.is_stationary_timeout = Timeout::default();
    }
    let (is_stationary, is_stationary_timeout) = update_with_timeout(
        state.is_stationary_timeout,
        PLAYER_MOVE_TIMEOUT,
        |timeout| (false, timeout),
        || (true, state.is_stationary_timeout),
        |timeout| (false, timeout),
    );
    state.is_stationary = is_stationary;
    state.is_stationary_timeout = is_stationary_timeout;
    state.last_known_pos = Some(pos);
    update_health_state(context, detector, state);
    update_rune_validating_state(context, state);
    Some(pos)
}

#[inline]
fn is_destination_intermediates(intermediates: Option<(usize, Array<(Point, bool), 16>)>) -> bool {
    intermediates.is_some_and(|(index, points)| index < points.len())
}

// TODO: ??????
#[inline]
fn find_points(
    platforms: &[PlatformWithNeighbors],
    cur_pos: Point,
    dest: Point,
    exact: bool,
    up_jump_only: bool,
) -> Option<(Point, bool, usize, Array<(Point, bool), 16>)> {
    let vertical_threshold = if up_jump_only {
        PLAYER_GRAPPLING_THRESHOLD
    } else {
        PLAYER_GRAPPLING_MAX_THRESHOLD
    };
    let vec = find_points_with(
        platforms,
        cur_pos,
        dest,
        DOUBLE_JUMP_THRESHOLD,
        PLAYER_JUMP_THRESHOLD,
        vertical_threshold,
    )?;
    let len = vec.len();
    let array = Array::from_iter(
        vec.into_iter()
            .enumerate()
            .map(|(i, point)| (point, if i == len - 1 { exact } else { false })),
    );
    let (point, exact) = array[0];
    Some((point, exact, 1, array))
}

// #[cfg(test)]
// mod tests {
//     use opencv::core::Rect;

//     use crate::{context::Context, detect::MockDetector};

//     use super::PlayerState;

//     fn create_mock_detector() -> MockDetector {
//         let rect = Rect::new(0, 0, 100, 100);
//         let player = Rect::new(50, 50, 10, 10);
//         let mut detector = MockDetector::new();
//         detector.expect_clone().returning(|| create_mock_detector());
//         detector.expect_detect_player().return_const(Ok(player));
//         detector
//     }

//     #[tokio::test(start_paused = true)]
//     async fn update_health_state() {
//         let rect = Rect::new(0, 0, 100, 100);
//         let context = Context::default();
//         let state = PlayerState::default();
//         // update_health_state("");
//     }
// }
