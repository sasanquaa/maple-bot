use std::collections::HashMap;

use anyhow::Result;
use log::{debug, info};
use opencv::core::{Point, Rect};
use platforms::windows::KeyKind;
use strum::Display;

use crate::{
    Class, Position,
    array::Array,
    buff::Buff,
    context::{Context, Contextual, ControlFlow, RUNE_BUFF_POSITION},
    database::{ActionKeyDirection, ActionKeyWith, KeyBinding, LinkKeyBinding},
    detect::Detector,
    minimap::Minimap,
    pathing::{PlatformWithNeighbors, find_points_with},
    player_actions::{PlayerAction, PlayerActionAutoMob, PlayerActionKey, PlayerActionMove},
    task::{Task, Update, update_task_repeatable},
};

/// Maximum number of times `Player::Moving` state can be transitioned to without changing position
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
const GRAPPLING_THRESHOLD: i32 = 26;

/// Maximum y distance from the destination required to perform a grappling hook
pub const GRAPPLING_MAX_THRESHOLD: i32 = 41;

/// The number of times a reachable y must successfuly ensures the player moves to that exact y
/// Once the count is reached, it is considered "solidified" and guaranteed the reachable y is always
/// a y that has platform(s)
const AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT: u32 = 4;

/// The acceptable y range above and below the detected mob position when matched with a reachable y
const AUTO_MOB_REACHABLE_Y_THRESHOLD: i32 = 8;

/// The minimum x distance required to transition to `Player::UseKey` in auto mob action
const AUTO_MOB_USE_KEY_X_THRESHOLD: i32 = 14;

/// The minimum y distance required to transition to `Player::UseKey` in auto mob action
const AUTO_MOB_USE_KEY_Y_THRESHOLD: i32 = JUMP_THRESHOLD;

/// The maximum number of times rune solving can fail before transition to
/// `Player::CashShopThenExit`
const MAX_RUNE_FAILED_COUNT: u32 = 2;

#[derive(Debug, Clone, Copy, Default)]
pub struct PlayerConfiguration {
    pub class: Class,
    /// Enables platform pathing for rune
    pub rune_platforms_pathing: bool,
    /// Uses only up jump(s) in rune platform pathing
    pub rune_platforms_pathing_up_jump_only: bool,
    /// Enables platform pathing for auto mob
    pub auto_mob_platforms_pathing: bool,
    /// Uses only up jump(s) in auto mob platform pathing
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
    /// The potion key
    pub potion_key: KeyKind,
    /// Uses potion when health is below a percentage
    pub use_potion_below_percent: Option<f32>,
    /// Milliseconds interval to update current health
    pub update_health_millis: Option<u64>,
}

/// The player persistent states
#[derive(Debug, Default)]
pub struct PlayerState {
    pub config: PlayerConfiguration,
    /// The id of the normal action provided by `Rotator`
    normal_action_id: u32,
    /// A normal action requested by `Rotator`
    normal_action: Option<PlayerAction>,
    /// The id of the priority action provided by `Rotator`
    priority_action_id: u32,
    /// A priority action requested by `Rotator`, this action will override
    /// the normal action if it is in the middle of executing.
    priority_action: Option<PlayerAction>,
    /// The player current health and max health
    pub health: Option<(u32, u32)>,
    /// The task to update health
    health_task: Option<Task<Result<(u32, u32)>>>,
    /// The rectangular health bar region
    health_bar: Option<Rect>,
    /// The task for the health bar
    health_bar_task: Option<Task<Result<Rect>>>,
    /// Track if the player moved within a specified ticks to determine if the player is stationary
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
    /// Indicates the last movement
    /// Helps coordinating between movement states (e.g. falling + double jumping)
    /// Resets to `None` when the destination (possibly intermediate) is reached or in `Player::Idle`
    last_movement: Option<LastMovement>,
    // TODO: 2 maps fr?
    /// Tracks `last_movement` to abort normal action when its position is not accurate
    /// Clears when a normal action is completed or aborted
    last_movement_normal_map: HashMap<LastMovement, u32>,
    /// Tracks `last_movement` to abort priority action when its position is not accurate
    /// Clears when a priority action is completed or aborted
    last_movement_priority_map: HashMap<LastMovement, u32>,
    /// Tracks a map of "reachable" y
    /// A y is reachable if there is a platform the player can stand on
    auto_mob_reachable_y_map: HashMap<i32, u32>,
    /// The matched reachable y and also the key in `auto_mob_reachable_y_map`
    auto_mob_reachable_y: Option<i32>,
    /// Tracks whether movement-related actions do not change the player position after a while.
    /// Resets when a limit is reached (for unstucking) or position did change.
    unstuck_counter: u32,
    /// The number of consecutive times player transtioned to `Player::Unstucking`
    /// Resets when position did change
    unstuck_consecutive_counter: u32,
    /// Unstuck task for detecting settings when mis-pressing ESC key
    unstuck_task: Option<Task<Result<bool>>>,
    /// Rune solving task
    rune_task: Option<Task<Result<[KeyKind; 4]>>>,
    /// The number of times `Player::SolvingRune` failed
    rune_failed_count: u32,
    /// Indicates the state will be transitioned to `Player::CashShopThenExit` in the next tick
    rune_cash_shop: bool,
    rune_validate_timeout: Option<Timeout>,
    /// A state to return to after stalling
    /// Resets when `Player::Stalling` timed out or in `Player::Idle`
    stalling_timeout_state: Option<Player>,
}

impl PlayerState {
    #[inline]
    pub fn reset(&mut self) {
        *self = PlayerState {
            config: self.config,
            reset_to_idle_next_update: true,
            ..PlayerState::default()
        };
    }

    #[inline]
    pub fn normal_action_name(&self) -> Option<String> {
        self.normal_action.map(|action| action.to_string())
    }

    #[inline]
    pub fn normal_action_id(&self) -> Option<u32> {
        self.has_normal_action().then_some(self.normal_action_id)
    }

    #[inline]
    pub fn has_normal_action(&self) -> bool {
        self.normal_action.is_some()
    }

    #[inline]
    pub fn set_normal_action(&mut self, id: u32, action: PlayerAction) {
        self.reset_to_idle_next_update = true;
        self.normal_action_id = id;
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
    pub fn set_priority_action(&mut self, id: u32, action: PlayerAction) {
        let _ = self.replace_priority_action(id, action);
    }

    #[inline]
    pub fn take_priority_action(&mut self) -> Option<u32> {
        self.reset_to_idle_next_update = true;
        self.priority_action
            .take()
            .is_some()
            .then_some(self.priority_action_id)
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
    pub fn has_rune_action(&self) -> bool {
        matches!(self.priority_action, Some(PlayerAction::SolveRune))
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
    fn clear_action_and_movement(&mut self) {
        if self.has_priority_action() {
            self.priority_action = None;
            self.last_movement_priority_map.clear();
        } else {
            self.auto_mob_reachable_y = None;
            self.normal_action = None;
            self.last_movement_normal_map.clear();
        }
    }

    #[inline]
    fn has_auto_mob_action_only(&self) -> bool {
        matches!(self.normal_action, Some(PlayerAction::AutoMob(_))) && !self.has_priority_action()
    }

    /// Whether the auto mob reachable y requires "solidifying"
    #[inline]
    fn auto_mob_reachable_y_require_update(&self) -> bool {
        self.auto_mob_reachable_y.is_none_or(|y| {
            *self.auto_mob_reachable_y_map.get(&y).unwrap() < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT
        })
    }

    /// Gets the falling minimum `y` distance threshold
    ///
    /// In auto mob or intermediate destination, the threshold is relaxed for more
    /// fluid movement.
    #[inline]
    fn falling_threshold(&self, is_intermediate: bool) -> i32 {
        if self.has_auto_mob_action_only() || is_intermediate {
            AUTO_MOB_REACHABLE_Y_THRESHOLD
        } else {
            FALLING_THRESHOLD
        }
    }

    /// Gets the double jump minimum `x` distance threshold
    ///
    /// In auto mob and final destination, the threshold is relaxed for more
    /// fluid movement.
    #[inline]
    fn double_jump_threshold(&self, is_intermediate: bool) -> i32 {
        if self.has_auto_mob_action_only() && !is_intermediate {
            DOUBLE_JUMP_AUTO_MOB_THRESHOLD
        } else {
            DOUBLE_JUMP_THRESHOLD
        }
    }
}

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

/// A contextual state that stores moving-related data
#[derive(Clone, Copy, Debug)]
pub struct Moving {
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
impl Moving {
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
    fn pos(self, pos: Point) -> Moving {
        Moving { pos, ..self }
    }

    #[inline]
    fn completed(self, completed: bool) -> Moving {
        Moving { completed, ..self }
    }

    #[inline]
    fn timeout(self, timeout: Timeout) -> Moving {
        Moving { timeout, ..self }
    }

    #[inline]
    fn timeout_current(self, current: u32) -> Moving {
        Moving {
            timeout: Timeout {
                current,
                ..self.timeout
            },
            ..self
        }
    }

    #[inline]
    fn last_destination(&self) -> Point {
        if self.is_destination_intermediate() {
            let points = self.intermediates.unwrap().1;
            points[points.len() - 1].0
        } else {
            self.dest
        }
    }

    #[inline]
    fn is_destination_intermediate(&self) -> bool {
        self.intermediates
            .is_some_and(|(index, points)| index < points.len())
    }
}

/// The different stages of using key
#[derive(Clone, Copy, Debug)]
enum UseKeyStage {
    Precondition,
    ChangingDirection(Timeout),
    EnsuringUseWith,
    Using(Timeout, bool),
    PostCondition,
}

#[derive(Clone, Copy, Debug)]
pub struct UseKey {
    key: KeyBinding,
    link_key: Option<LinkKeyBinding>,
    count: u32,
    current_count: u32,
    direction: ActionKeyDirection,
    with: ActionKeyWith,
    wait_before_use_ticks: u32,
    wait_after_use_ticks: u32,
    stage: UseKeyStage,
}

impl UseKey {
    #[inline]
    fn from_action(action: PlayerAction) -> Self {
        match action {
            PlayerAction::Key(PlayerActionKey {
                key,
                link_key,
                count,
                direction,
                with,
                wait_before_use_ticks,
                wait_after_use_ticks,
                ..
            }) => Self {
                key,
                link_key,
                count,
                current_count: 0,
                direction,
                with,
                wait_before_use_ticks,
                wait_after_use_ticks,
                stage: UseKeyStage::Precondition,
            },
            PlayerAction::AutoMob(mob) => Self {
                key: mob.key,
                link_key: None,
                count: mob.count,
                current_count: 0,
                direction: ActionKeyDirection::Any,
                with: ActionKeyWith::Any,
                wait_before_use_ticks: mob.wait_before_ticks,
                wait_after_use_ticks: mob.wait_after_ticks,
                stage: UseKeyStage::Precondition,
            },
            PlayerAction::SolveRune | PlayerAction::Move { .. } => {
                unreachable!()
            }
        }
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct SolvingRune {
    timeout: Timeout,
    keys: Option<[KeyKind; 4]>,
    key_index: usize,
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
    Detecting,
    Idle,
    UseKey(UseKey),
    /// Movement-related coordinator state
    Moving(Point, bool, Option<(usize, Array<(Point, bool), 16>)>),
    /// Perform walk or small adjustment x-wise action
    Adjusting(Moving),
    /// Perform double jump action
    DoubleJumping(Moving, bool, bool),
    /// Perform a grappling action
    Grappling(Moving),
    Jumping(Moving),
    /// Perform an up jump action
    UpJumping(Moving),
    /// Perform a falling action
    Falling(Moving, Point),
    /// Unstuck when inside non-detecting position or because of `state.unstuck_counter`
    Unstucking(Timeout, Option<bool>),
    /// Stall for time and return to `Player::Idle` or `state.stalling_timeout_state`
    Stalling(Timeout, u32),
    /// Try to solve a rune
    SolvingRune(SolvingRune),
    /// Enter the cash shop then exit after 10 seconds
    CashShopThenExit(Timeout, CashShop),
}

impl Contextual for Player {
    type Persistent = PlayerState;

    // 草草ｗｗ。。。
    // TODO: detect if a point is reachable after number of retries?
    // TODO: split into smaller files?
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
            return ControlFlow::Next(Player::CashShopThenExit(
                Timeout::default(),
                CashShop::Entering,
            ));
        }
        let cur_pos = if state.ignore_pos_update {
            state.last_known_pos
        } else {
            update_state(context, detector, state)
        };
        let Some(cur_pos) = cur_pos else {
            // When the player detection fails, the possible causes are:
            // - Player moved inside the edges of the minimap
            // - Other UIs overlapping the minimap
            //
            // `update_non_positional_context` is here to continue updating
            // `Player::Unstucking` returned from below when the player
            // is inside the edges of the minimap
            if let Some(next) = update_non_positional_context(self, context, detector, state, true)
            {
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
        let next = update_non_positional_context(contextual, context, detector, state, false)
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
    detector: &impl Detector,
    state: &mut PlayerState,
    failed_to_detect_player: bool,
) -> Option<Player> {
    match contextual {
        Player::UseKey(use_key) => {
            (!failed_to_detect_player).then(|| update_use_key_context(context, state, use_key))
        }
        Player::Unstucking(timeout, has_settings) => Some(update_unstucking_context(
            context,
            detector,
            state,
            timeout,
            has_settings,
        )),
        Player::Stalling(timeout, max_timeout) => {
            (!failed_to_detect_player).then(|| update_stalling_context(state, timeout, max_timeout))
        }
        Player::SolvingRune(solving_rune) => (!failed_to_detect_player)
            .then(|| update_solving_rune_context(context, detector, state, solving_rune)),
        // TODO: Improve this?
        Player::CashShopThenExit(timeout, cash_shop) => {
            let next = match cash_shop {
                CashShop::Entering => {
                    let _ = context.keys.send(state.config.cash_shop_key);
                    let next = if detector.detect_player_in_cash_shop() {
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
                    let next = if detector.detect_player_in_cash_shop() {
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
        | Player::CashShopThenExit(_, _) => unreachable!(),
    }
}

/// Updates `Player::Idle` contextual state
///
/// This state does not do much on its own except when auto mobbing. It acts as entry
/// to other state when there is an action and helps clearing keys.
fn update_idle_context(context: &Context, state: &mut PlayerState, cur_pos: Point) -> Player {
    fn ensure_reachable_auto_mob_y(
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
        if state.config.auto_mob_platforms_pathing {
            if let Minimap::Idle(idle) = context.minimap {
                if let Some((point, exact, index, array)) = find_points(
                    &idle.platforms,
                    player_pos,
                    point,
                    mob_pos.allow_adjusting,
                    state.config.auto_mob_platforms_pathing_up_jump_only,
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
            PlayerAction::AutoMob(PlayerActionAutoMob { position, .. }) => Some((
                ensure_reachable_auto_mob_y(context, state, cur_pos, position),
                false,
            )),
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
                            Moving::new(cur_pos, cur_pos, false, None),
                            true,
                            true,
                        ),
                        false,
                    ))
                } else {
                    Some((Player::UseKey(UseKey::from_action(action)), false))
                }
            }
            PlayerAction::Key(PlayerActionKey {
                position: None,
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            }) => Some((Player::UseKey(UseKey::from_action(action)), false)),
            PlayerAction::SolveRune => {
                if let Minimap::Idle(idle) = context.minimap {
                    if let Some(rune) = idle.rune {
                        if state.config.rune_platforms_pathing {
                            if !state.is_stationary {
                                return Some((Player::Idle, false));
                            }
                            if let Some((point, exact, index, array)) = find_points(
                                &idle.platforms,
                                cur_pos,
                                rune,
                                true,
                                state.config.rune_platforms_pathing_up_jump_only,
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
    state.stalling_timeout_state = None;
    let _ = context.keys.send_up(KeyKind::Up);
    let _ = context.keys.send_up(KeyKind::Down);
    let _ = context.keys.send_up(KeyKind::Left);
    let _ = context.keys.send_up(KeyKind::Right);

    on_action_state_mut(
        state,
        |state, action| on_player_action(context, state, action, cur_pos),
        || Player::Idle,
    )
}

fn update_use_key_context(context: &Context, state: &mut PlayerState, use_key: UseKey) -> Player {
    const CHANGE_DIRECTION_TIMEOUT: u32 = 2;

    #[inline]
    fn ensure_direction(state: &PlayerState, direction: ActionKeyDirection) -> bool {
        match direction {
            ActionKeyDirection::Any => true,
            ActionKeyDirection::Left | ActionKeyDirection::Right => {
                direction == state.last_known_direction
            }
        }
    }

    #[inline]
    fn ensure_use_with(state: &PlayerState, use_key: UseKey) -> bool {
        match use_key.with {
            ActionKeyWith::Any => true,
            ActionKeyWith::Stationary => state.is_stationary,
            ActionKeyWith::DoubleJump => {
                matches!(state.last_movement, Some(LastMovement::DoubleJumping))
            }
        }
    }

    #[inline]
    fn update_link_key(
        context: &Context,
        class: Class,
        use_key: UseKey,
        timeout: Timeout,
        completed: bool,
    ) -> Player {
        debug_assert!(!timeout.started || !completed);
        let link_key = use_key.link_key.unwrap();
        let link_key_timeout = match class {
            Class::Cadena => 4,
            Class::Blaster => 8,
            Class::Ark => 10,
            Class::Generic => 5,
        };
        return update_with_timeout(
            timeout,
            link_key_timeout,
            |timeout| {
                if let LinkKeyBinding::Before(key) = link_key {
                    let _ = context.keys.send(key.into());
                }
                Player::UseKey(UseKey {
                    stage: UseKeyStage::Using(timeout, completed),
                    ..use_key
                })
            },
            || {
                if let LinkKeyBinding::After(key) = link_key {
                    let _ = context.keys.send(key.into());
                    if matches!(class, Class::Blaster) && !matches!(key, KeyBinding::Space) {
                        let _ = context.keys.send(KeyKind::Space);
                    }
                }
                Player::UseKey(UseKey {
                    stage: UseKeyStage::Using(timeout, true),
                    ..use_key
                })
            },
            |timeout| {
                Player::UseKey(UseKey {
                    stage: UseKeyStage::Using(timeout, completed),
                    ..use_key
                })
            },
        );
    }

    // TODO: Am I cooked?
    let next = match use_key.stage {
        UseKeyStage::Precondition => {
            debug_assert!(use_key.current_count < use_key.count);
            if !ensure_direction(state, use_key.direction) {
                return Player::UseKey(UseKey {
                    stage: UseKeyStage::ChangingDirection(Timeout::default()),
                    ..use_key
                });
            }
            if !ensure_use_with(state, use_key) {
                return Player::UseKey(UseKey {
                    stage: UseKeyStage::EnsuringUseWith,
                    ..use_key
                });
            }
            debug_assert!(
                matches!(use_key.direction, ActionKeyDirection::Any)
                    || use_key.direction == state.last_known_direction
            );
            debug_assert!(
                matches!(use_key.with, ActionKeyWith::Any)
                    || (matches!(use_key.with, ActionKeyWith::Stationary) && state.is_stationary)
                    || (matches!(use_key.with, ActionKeyWith::DoubleJump)
                        && matches!(state.last_movement, Some(LastMovement::DoubleJumping)))
            );
            let next = Player::UseKey(UseKey {
                stage: UseKeyStage::Using(Timeout::default(), false),
                ..use_key
            });
            if use_key.wait_before_use_ticks > 0 {
                state.stalling_timeout_state = Some(next);
                Player::Stalling(Timeout::default(), use_key.wait_before_use_ticks)
            } else {
                state.use_immediate_control_flow = true;
                next
            }
        }
        UseKeyStage::ChangingDirection(timeout) => {
            let key = match use_key.direction {
                ActionKeyDirection::Left => KeyKind::Left,
                ActionKeyDirection::Right => KeyKind::Right,
                ActionKeyDirection::Any => unreachable!(),
            };
            update_with_timeout(
                timeout,
                CHANGE_DIRECTION_TIMEOUT,
                |timeout| {
                    let _ = context.keys.send_down(key);
                    Player::UseKey(UseKey {
                        stage: UseKeyStage::ChangingDirection(timeout),
                        ..use_key
                    })
                },
                || {
                    let _ = context.keys.send_up(key);
                    state.last_known_direction = use_key.direction;
                    Player::UseKey(UseKey {
                        stage: UseKeyStage::Precondition,
                        ..use_key
                    })
                },
                |timeout| {
                    Player::UseKey(UseKey {
                        stage: UseKeyStage::ChangingDirection(timeout),
                        ..use_key
                    })
                },
            )
        }
        UseKeyStage::EnsuringUseWith => match use_key.with {
            ActionKeyWith::Any => unreachable!(),
            ActionKeyWith::Stationary => {
                let stage = if state.is_stationary {
                    UseKeyStage::Precondition
                } else {
                    UseKeyStage::EnsuringUseWith
                };
                Player::UseKey(UseKey { stage, ..use_key })
            }
            ActionKeyWith::DoubleJump => {
                let pos = state.last_known_pos.unwrap();
                Player::DoubleJumping(Moving::new(pos, pos, false, None), true, true)
            }
        },
        UseKeyStage::Using(timeout, completed) => {
            debug_assert!(use_key.link_key.is_some() || !completed);
            debug_assert!(state.stalling_timeout_state.is_none());
            match use_key.link_key {
                Some(LinkKeyBinding::After(_)) => {
                    if !timeout.started {
                        let _ = context.keys.send(use_key.key.into());
                    }
                    if !completed {
                        return update_link_key(
                            context,
                            state.config.class,
                            use_key,
                            timeout,
                            completed,
                        );
                    }
                }
                Some(LinkKeyBinding::AtTheSame(key)) => {
                    let _ = context.keys.send(key.into());
                    let _ = context.keys.send(use_key.key.into());
                }
                Some(LinkKeyBinding::Before(_)) | None => {
                    if use_key.link_key.is_some() && !completed {
                        return update_link_key(
                            context,
                            state.config.class,
                            use_key,
                            timeout,
                            completed,
                        );
                    }
                    debug_assert!(use_key.link_key.is_none() || completed);
                    let _ = context.keys.send(use_key.key.into());
                }
            }
            let next = Player::UseKey(UseKey {
                stage: UseKeyStage::PostCondition,
                ..use_key
            });
            if use_key.wait_after_use_ticks > 0 {
                state.stalling_timeout_state = Some(next);
                Player::Stalling(Timeout::default(), use_key.wait_after_use_ticks)
            } else {
                next
            }
        }
        UseKeyStage::PostCondition => {
            debug_assert!(state.stalling_timeout_state.is_none());
            if use_key.current_count + 1 < use_key.count {
                Player::UseKey(UseKey {
                    current_count: use_key.current_count + 1,
                    stage: UseKeyStage::Precondition,
                    ..use_key
                })
            } else {
                Player::Idle
            }
        }
    };

    on_action_state(
        state,
        |state, action| match action {
            PlayerAction::AutoMob(_) => {
                let is_terminal = matches!(next, Player::Idle);
                if is_terminal && state.auto_mob_reachable_y_require_update() {
                    return Some((Player::Stalling(Timeout::default(), MOVE_TIMEOUT), false));
                }
                Some((next, is_terminal))
            }
            PlayerAction::Key(_) => Some((next, matches!(next, Player::Idle))),
            PlayerAction::Move(_) | PlayerAction::SolveRune => None,
        },
        || next,
    )
}

/// Updates the `Player::Moving` contextual state
///
/// This state does not perform any movement but acts as coordinator
/// for other movement states. It keeps track of `state.unstuck_counter`, avoids
/// state looping and advancing `intermediates` when the current destination is reached.
///
/// It will first transition to `Player::DoubleJumping` and `Player::Adjusting` for
/// matching `x` of `dest`. Then, `Player::Grappling`, `Player::UpJumping`, `Player::Jump` or
/// `Player::Falling` for matching `y` of `dest`. (e.g. horizontal then vertical)
fn update_moving_context(
    state: &mut PlayerState,
    cur_pos: Point,
    dest: Point,
    exact: bool,
    intermediates: Option<(usize, Array<(Point, bool), 16>)>,
) -> Player {
    const HORIZONTAL_MOVEMENT_REPEAT_COUNT: u32 = 20;
    const VERTICAL_MOVEMENT_REPEAT_COUNT: u32 = 8;
    const AUTO_MOB_HORIZONTAL_MOVEMENT_REPEAT_COUNT: u32 = 8;
    const AUTO_MOB_VERTICAL_MOVEMENT_REPEAT_COUNT: u32 = 2;
    const UP_JUMP_THRESHOLD: i32 = 10;

    /// Aborts the action when state starts looping.
    fn abort_action_on_state_repeat(next: Player, state: &mut PlayerState) -> Player {
        if let Some(last_movement) = state.last_movement {
            let count_max = match last_movement {
                LastMovement::Adjusting | LastMovement::DoubleJumping => {
                    if state.has_auto_mob_action_only() {
                        AUTO_MOB_HORIZONTAL_MOVEMENT_REPEAT_COUNT
                    } else {
                        HORIZONTAL_MOVEMENT_REPEAT_COUNT
                    }
                }
                LastMovement::Falling
                | LastMovement::Grappling
                | LastMovement::UpJumping
                | LastMovement::Jumping => {
                    if state.has_auto_mob_action_only() {
                        AUTO_MOB_VERTICAL_MOVEMENT_REPEAT_COUNT
                    } else {
                        VERTICAL_MOVEMENT_REPEAT_COUNT
                    }
                }
            };
            let count_map = if state.has_priority_action() {
                &mut state.last_movement_priority_map
            } else {
                &mut state.last_movement_normal_map
            };
            let count = count_map.entry(last_movement).or_insert(0);
            debug_assert!(*count < count_max);
            *count += 1;
            let count = *count;
            debug!(target: "player", "last movement {:?}", count_map);
            if count >= count_max {
                info!(target: "player", "abort action due to repeated state");
                state.clear_action_and_movement();
                return Player::Idle;
            }
        }
        next
    }

    fn on_player_action(
        last_known_direction: ActionKeyDirection,
        action: PlayerAction,
        moving: Moving,
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
                    Some((Player::UseKey(UseKey::from_action(action)), false))
                }
            }
            PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::Any | ActionKeyWith::Stationary,
                ..
            })
            | PlayerAction::AutoMob(_) => {
                Some((Player::UseKey(UseKey::from_action(action)), false))
            }
            PlayerAction::SolveRune => Some((Player::SolvingRune(SolvingRune::default()), false)),
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
    let moving = Moving::new(cur_pos, dest, exact, intermediates);
    let is_intermediate = moving.is_destination_intermediate();

    match (x_distance, y_direction, y_distance) {
        (d, _, _) if d >= state.double_jump_threshold(is_intermediate) => {
            abort_action_on_state_repeat(Player::DoubleJumping(moving, false, false), state)
        }
        (d, _, _)
            if d >= ADJUSTING_MEDIUM_THRESHOLD || (exact && d >= ADJUSTING_SHORT_THRESHOLD) =>
        {
            abort_action_on_state_repeat(Player::Adjusting(moving), state)
        }
        // y > 0: cur_pos is below dest
        // y < 0: cur_pos is above of dest
        (_, y, d) if y > 0 && d >= GRAPPLING_THRESHOLD => {
            abort_action_on_state_repeat(Player::Grappling(moving), state)
        }
        (_, y, d) if y > 0 && d >= UP_JUMP_THRESHOLD => {
            abort_action_on_state_repeat(Player::UpJumping(moving), state)
        }
        (_, y, d) if y > 0 && d >= JUMP_THRESHOLD => {
            abort_action_on_state_repeat(Player::Jumping(moving), state)
        }
        // this probably won't work if the platforms are far apart,
        // which is weird to begin with and only happen in very rare place (e.g. Haven)
        (_, y, d) if y < 0 && d >= state.falling_threshold(is_intermediate) => {
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
                    } else {
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

/// Updates the `Player::Adjusting` contextual state
///
/// This state just walks towards the destination. If `moving.exact` is true,
/// then it will perform small movement to ensure the `x` is as close as possible.
fn update_adjusting_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: Moving,
) -> Player {
    const USE_KEY_Y_THRESHOLD: i32 = 2;
    const ADJUSTING_SHORT_TIMEOUT: u32 = 3;

    /// Handles `PlayerAction` for `Player::Adjusting`
    ///
    /// TODO
    fn on_player_action(
        context: &Context,
        state: &PlayerState,
        action: PlayerAction,
        x_distance: i32,
        y_distance: i32,
        moving: Moving,
    ) -> Option<(Player, bool)> {
        match action {
            PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::DoubleJump,
                direction,
                ..
            }) => {
                if !moving.completed || y_distance > 0 {
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
                    Some((Player::UseKey(UseKey::from_action(action)), false))
                }
            }
            PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::Any,
                ..
            }) => {
                if moving.completed && y_distance <= USE_KEY_Y_THRESHOLD {
                    Some((Player::UseKey(UseKey::from_action(action)), false))
                } else {
                    None
                }
            }
            PlayerAction::AutoMob(_) => {
                on_auto_mob_use_key_action(context, action, x_distance, y_distance)
            }
            PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::Stationary,
                ..
            })
            | PlayerAction::SolveRune
            | PlayerAction::Move(_) => None,
        }
    }

    debug_assert!(moving.timeout.started || !moving.completed);
    let (x_distance, x_direction) = x_distance_direction(moving.dest, cur_pos);
    let (y_distance, y_direction) = y_distance_direction(moving.dest, cur_pos);
    let is_intermediate = moving.is_destination_intermediate();
    if x_distance >= state.double_jump_threshold(is_intermediate) {
        state.use_immediate_control_flow = true;
        return Player::Moving(moving.dest, moving.exact, moving.intermediates);
    }
    if !moving.timeout.started {
        if !matches!(state.last_movement, Some(LastMovement::Falling))
            && x_distance >= ADJUSTING_MEDIUM_THRESHOLD
            && y_direction < 0
            && y_distance >= ADJUSTING_OR_DOUBLE_JUMPING_FALLING_THRESHOLD
            && !is_intermediate
        {
            return Player::Falling(moving.pos(cur_pos), cur_pos);
        }
        state.last_movement = Some(LastMovement::Adjusting);
    }

    update_moving_axis_context(
        moving,
        cur_pos,
        MOVE_TIMEOUT,
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

            on_action_state(
                state,
                |state, action| {
                    let dest = moving.last_destination();
                    let (x_distance, _) = x_distance_direction(dest, cur_pos);
                    let (y_distance, _) = y_distance_direction(dest, cur_pos);
                    on_player_action(context, state, action, x_distance, y_distance, moving)
                },
                || {
                    if !moving.completed {
                        Player::Adjusting(moving)
                    } else {
                        Player::Adjusting(moving.timeout_current(MOVE_TIMEOUT))
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
    moving: Moving,
    forced: bool,
    require_stationary: bool,
) -> Player {
    // Note: even in auto mob, also use the non-auto mob threshold
    const TIMEOUT: u32 = MOVE_TIMEOUT * 2;
    const USE_KEY_X_THRESHOLD: i32 = DOUBLE_JUMP_THRESHOLD;
    const USE_KEY_Y_THRESHOLD: i32 = 10;
    const GRAPPLING_THRESHOLD: i32 = 4;
    const FORCE_THRESHOLD: i32 = 3;

    fn on_player_action(
        context: &Context,
        forced: bool,
        action: PlayerAction,
        x_distance: i32,
        y_distance: i32,
        moving: Moving,
    ) -> Option<(Player, bool)> {
        match action {
            // ignore proximity check when it is forced to double jumped
            // this indicates the player is already near the destination
            PlayerAction::Key(PlayerActionKey {
                with: ActionKeyWith::DoubleJump | ActionKeyWith::Any,
                ..
            })
            | PlayerAction::AutoMob(_) => {
                if !moving.completed {
                    return None;
                }
                if forced
                    || (!moving.exact
                        && x_distance <= USE_KEY_X_THRESHOLD
                        && y_distance <= USE_KEY_Y_THRESHOLD)
                {
                    let _ = context.keys.send_up(KeyKind::Right);
                    let _ = context.keys.send_up(KeyKind::Left);
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

    debug_assert!(moving.timeout.started || !moving.completed);
    let ignore_grappling = forced
        || (state.has_auto_mob_action_only()
            && state.config.auto_mob_platforms_pathing
            && state.config.auto_mob_platforms_pathing_up_jump_only)
        || (state.has_rune_action()
            && state.config.rune_platforms_pathing
            && state.config.rune_platforms_pathing_up_jump_only);
    let x_changed = (cur_pos.x - moving.pos.x).abs();
    let (x_distance, x_direction) = x_distance_direction(moving.dest, cur_pos);
    let (y_distance, y_direction) = y_distance_direction(moving.dest, cur_pos);
    let is_intermediate = moving.is_destination_intermediate();
    if !moving.timeout.started {
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
                        .send(state.config.teleport_key.unwrap_or(KeyKind::Space));
                } else {
                    let _ = context.keys.send_up(KeyKind::Right);
                    let _ = context.keys.send_up(KeyKind::Left);
                    moving = moving.completed(true);
                }
            }
            on_action(
                state,
                |action| {
                    let dest = moving.last_destination();
                    let (x_distance, _) = x_distance_direction(dest, cur_pos);
                    let (y_distance, _) = y_distance_direction(dest, cur_pos);
                    on_player_action(context, forced, action, x_distance, y_distance, moving)
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

/// Updates the `Player::Grappling` contextual state
///
/// This state can only be transitioned via `Player::Moving` or `Player::DoubleJumping`
/// when the player has reached or close to the destination x-wise.
///
/// This state will use the Rope Lift skill
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

/// Updates the `Player::UpJumping` contextual state
///
/// This state can only be transitioned via `Player::Moving` when the
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
    let key = state.config.upjump_key;
    update_moving_axis_context(
        moving,
        cur_pos,
        TIMEOUT,
        |moving| {
            let _ = context.keys.send_down(KeyKind::Up);
            if key.is_none() {
                let _ = context.keys.send(KeyKind::Space);
            }
            Player::UpJumping(moving)
        },
        Some(|| {
            let _ = context.keys.send_up(KeyKind::Up);
        }),
        |mut moving| {
            match (moving.completed, key) {
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
                            let _ = context.keys.send(KeyKind::Space);
                        }
                    } else {
                        moving = moving.completed(true);
                    }
                }
                (true, _) => {
                    // this is when up jump like blaster still requires up key
                    // cancel early to avoid stucking to a rope
                    if key.is_some() && moving.timeout.total == STOP_UP_KEY_TICK {
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
                        let dest = moving.last_destination();
                        let (x_distance, _) = x_distance_direction(dest, cur_pos);
                        let (y_distance, _) = y_distance_direction(dest, cur_pos);
                        on_auto_mob_use_key_action(context, action, x_distance, y_distance)
                    }
                    PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune => None,
                },
                || Player::UpJumping(moving),
            )
        },
        ChangeAxis::Vertical,
    )
}

/// Updates the `Player::Falling` contextual state
///
/// This state will perform a drop down `Down Key + Space`
fn update_falling_context(
    context: &Context,
    state: &mut PlayerState,
    cur_pos: Point,
    moving: Moving,
    anchor: Point,
) -> Player {
    const STOP_DOWN_KEY_TICK: u32 = 2;
    const TIMEOUT: u32 = MOVE_TIMEOUT * 2;

    let y_changed = cur_pos.y - anchor.y;
    let (x_distance, _) = x_distance_direction(moving.dest, cur_pos);
    let is_stationary = state.is_stationary;
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
                let _ = context.keys.send(KeyKind::Space);
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
            if x_distance >= ADJUSTING_MEDIUM_THRESHOLD && y_changed < 0 {
                moving = moving.timeout_current(TIMEOUT);
            }
            on_action(
                state,
                |action| match action {
                    PlayerAction::AutoMob(_) => {
                        let dest = moving.last_destination();
                        let (x_distance, _) = x_distance_direction(dest, cur_pos);
                        let (y_distance, _) = y_distance_direction(dest, cur_pos);
                        on_auto_mob_use_key_action(context, action, x_distance, y_distance)
                    }
                    PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune => None,
                },
                || Player::Falling(moving, anchor),
            )
        },
        ChangeAxis::Vertical,
    )
}

/// Updates the `Player::Unstucking` contextual state
///
/// This state can only be transitioned to when `state.unstuck_counter` reached the fixed
/// threshold or when the player moved into the edges of the minimap.
/// If `state.unstuck_consecutive_counter` has not reached the threshold and the player
/// moved into the left/right/top edges of the minimap, it will try to move
/// out as appropriate. It will also try to press ESC key to exit any dialog.
///
/// Each initial transition to `Player::Unstucking` increases
/// the `state.unstuck_consecutive_counter` by one. If the threshold is reached, this
/// state will enter GAMBA mode. And by definition, it means `random bullsh*t go`.
fn update_unstucking_context(
    context: &Context,
    detector: &impl Detector,
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
                let _ = context.keys.send(KeyKind::Space);
            }
            Player::Unstucking(timeout, has_settings)
        },
    )
}

/// Updates the `Player::Stalling` contextual state
///
/// This state stalls for the specified number of `max_timeout`. Upon timing out,
/// it will return to `state.stalling_timeout_state` if `Some` or `Player::Idle` if `None`.
/// And `Player::Idle` is considered the terminal state if there is an action.
/// `state.stalling_timeout_state` is currently only `Some` when it is transitioned via `Player::UseKey`.
///
/// If this state timeout in auto mob with terminal state, it will perform
/// auto mob reachable `y` solidifying if needed.
fn update_stalling_context(state: &mut PlayerState, timeout: Timeout, max_timeout: u32) -> Player {
    let update = |timeout| Player::Stalling(timeout, max_timeout);
    let next = update_with_timeout(
        timeout,
        max_timeout,
        update,
        || state.stalling_timeout_state.take().unwrap_or(Player::Idle),
        update,
    );

    on_action_state_mut(
        state,
        |state, action| match action {
            PlayerAction::AutoMob(_) => {
                let is_terminal = matches!(next, Player::Idle);
                if is_terminal && state.auto_mob_reachable_y_require_update() {
                    if !state.is_stationary {
                        return Some((Player::Stalling(Timeout::default(), max_timeout), false));
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
                Some((next, is_terminal))
            }
            PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune => {
                Some((next, matches!(next, Player::Idle)))
            }
        },
        || next,
    )
}

/// Updates the `Player::SolvingRune` contextual state
///
/// Though this state can only be transitioned via `Player::Moving` with `PlayerAction::SolveRune`,
/// it is not required. This state does:
/// - On timeout start, sends the interact key
/// - On timeout update, detects the rune and sends the keys
/// - On timeout end or rune is solved before timing out, transitions to `Player::Idle`
fn update_solving_rune_context(
    context: &Context,
    detector: &impl Detector,
    state: &mut PlayerState,
    solving_rune: SolvingRune,
) -> Player {
    const TIMEOUT: u32 = 155;
    const PRESS_KEY_INTERVAL: u32 = 8;

    debug_assert!(state.rune_validate_timeout.is_none());
    debug_assert!(state.rune_failed_count < MAX_RUNE_FAILED_COUNT);
    debug_assert!(!state.rune_cash_shop);
    let next = update_with_timeout(
        solving_rune.timeout,
        TIMEOUT,
        |timeout| {
            let _ = context.keys.send(state.config.interact_key);
            Player::SolvingRune(SolvingRune {
                timeout,
                ..solving_rune
            })
        },
        || {
            // likely a spinning rune if the bot can't detect and timeout
            Player::Idle
        },
        |timeout| {
            if solving_rune.keys.is_none() {
                let detector = detector.clone();
                let Update::Complete(Ok(keys)) =
                    update_task_repeatable(500, &mut state.rune_task, move || {
                        detector.detect_rune_arrows()
                    })
                else {
                    return Player::SolvingRune(SolvingRune {
                        timeout,
                        ..solving_rune
                    });
                };
                return Player::SolvingRune(SolvingRune {
                    // reset current timeout for pressing keys
                    timeout: Timeout {
                        current: 1, // starts at 1 instead of 0 to avoid immediate key press
                        total: 1,
                        started: true,
                    },
                    keys: Some(keys),
                    ..solving_rune
                });
            }
            if timeout.current % PRESS_KEY_INTERVAL != 0 {
                return Player::SolvingRune(SolvingRune {
                    timeout,
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
                Player::Idle
            } else {
                Player::SolvingRune(SolvingRune {
                    timeout,
                    key_index,
                    ..solving_rune
                })
            }
        },
    );

    on_action_state_mut(
        state,
        |state, action| match action {
            PlayerAction::SolveRune => {
                let is_terminal = matches!(next, Player::Idle);
                if is_terminal {
                    if solving_rune.keys.is_some() {
                        state.rune_validate_timeout = Some(Timeout::default());
                    } else {
                        update_rune_fail_count_state(state);
                    }
                }
                Some((next, is_terminal))
            }
            PlayerAction::AutoMob(_) | PlayerAction::Key(_) | PlayerAction::Move(_) => {
                unreachable!()
            }
        },
        || next,
    )
}

/// Checks proximity in `PlayerAction::AutoMob` for transitioning to `Player::UseKey`
///
/// This is common logics shared with other contextual states when there is auto mob action
#[inline]
fn on_auto_mob_use_key_action(
    context: &Context,
    action: PlayerAction,
    x_distance: i32,
    y_distance: i32,
) -> Option<(Player, bool)> {
    if x_distance <= AUTO_MOB_USE_KEY_X_THRESHOLD && y_distance <= AUTO_MOB_USE_KEY_Y_THRESHOLD {
        let _ = context.keys.send_up(KeyKind::Down);
        let _ = context.keys.send_up(KeyKind::Up);
        let _ = context.keys.send_up(KeyKind::Left);
        let _ = context.keys.send_up(KeyKind::Right);
        Some((Player::UseKey(UseKey::from_action(action)), false))
    } else {
        None
    }
}

/// Callbacks for when there is a normal or priority `PlayerAction`
///
/// This version does not require `PlayerState` in the callbacks arguments
#[inline]
fn on_action(
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

/// Callbacks for when there is a normal or priority `PlayerAction`
///
/// This version requires a shared reference `PlayerState` in the callbacks arguments
#[inline]
fn on_action_state(
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

/// Callbacks for when there is a normal or priority `PlayerAction`
///
/// When there is a priority action, it takes precendece over the normal action. The callback
/// should return a tuple `Option<(Player, bool)>` with:
/// - `Some((Player, false))` indicating the callback is handled but `Player` is not terminal state
/// - `Some((Player, true))` indicating the callback is handled and `Player` is terminal state
/// - `None` indicating the callback is not handled and will be defaulted to `on_default_context`
///
/// When the returned tuple indicates a terminal state, the `PlayerAction` is considered complete.
/// Because this function passes a mutable reference of `PlayerState` to `on_action_context`,
/// caller should be aware not to clear the action but let this function handles it.
#[inline]
fn on_action_state_mut(
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
                // FIXME: clear only when has position?
                state.clear_action_and_movement();
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
    /// Detects a change in x direction
    Horizontal,
    /// Detects a change in y direction
    Vertical,
    /// Detects a change in both directions
    Both,
}

/// A struct that stores the current tick before timing out
///
/// Most contextual states can be timed out as there is no guaranteed
/// an action will be performed or a state can be transitioned. So timeout is used to retry
/// such action/state and to avoid looping in a single state forever. Or
/// for some contextual states to perform an action only after timing out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timeout {
    /// The current timeout tick.
    /// The timeout tick can be reset to 0 in the context of movement.
    current: u32,
    /// The total number of passed ticks. Useful when `current` can be reset
    /// Currently only used for delaying upjumping and stopping down key early in falling
    total: u32,
    /// Inidcates whether the timeout has started
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

/// Updates movement-related contextual states
///
/// This function helps resetting the `Timeout` when the player's position changed
/// based on `ChangeAxis`. Upon timing out, it returns to `Player::Moving`.
#[inline]
fn update_moving_axis_context(
    moving: Moving,
    cur_pos: Point,
    max_timeout: u32,
    on_started: impl FnOnce(Moving) -> Player,
    on_timeout: Option<impl FnOnce()>,
    on_update: impl FnOnce(Moving) -> Player,
    axis: ChangeAxis,
) -> Player {
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
            ChangeAxis::Horizontal => cur_pos.x != prev_pos.x,
            ChangeAxis::Vertical => cur_pos.y != prev_pos.y,
            ChangeAxis::Both { .. } => cur_pos.x != prev_pos.x || cur_pos.y != prev_pos.y,
        };
        Timeout {
            current: if moved { 0 } else { timeout.current },
            ..timeout
        }
    }

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
fn update_rune_fail_count_state(state: &mut PlayerState) {
    state.rune_failed_count += 1;
    if state.rune_failed_count >= MAX_RUNE_FAILED_COUNT {
        state.rune_failed_count = 0;
        state.rune_cash_shop = true;
    }
}

/// Updates the rune validation `Timeout`
///
/// `state.rune_validate_timeout` is `Some` only when `Player::SolvingRune`
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
            |timeout| Some(timeout),
            || {
                if matches!(context.buffs[RUNE_BUFF_POSITION], Buff::NoBuff) {
                    update_rune_fail_count_state(state);
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
    if state.config.use_potion_below_percent.is_none() {
        reset_health(state);
        return;
    }
    let percentage = state.config.use_potion_below_percent.unwrap();
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
        state.config.update_health_millis.unwrap_or(1000),
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
            let _ = context.keys.send(state.config.potion_key);
        }
    }
}

/// Updates the `PlayerState`
///
/// This function:
/// - Returns the player current position or `None` when the minimap or player cannot be detected
/// - Updates the stationary check via `state.is_stationary_timeout`
/// - Delegates to `update_health_state` and `update_rune_validating_state`
/// - Resets `state.unstuck_counter` and `state.unstuck_consecutive_counter` when position changed
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
        MOVE_TIMEOUT,
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

// TODO: ??????
// TODO: is 16 good?
#[inline]
fn find_points(
    platforms: &[PlatformWithNeighbors],
    cur_pos: Point,
    dest: Point,
    exact: bool,
    up_jump_only: bool,
) -> Option<(Point, bool, usize, Array<(Point, bool), 16>)> {
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
    let (point, exact) = array[0];
    Some((point, exact, 1, array))
}

// TODO: add more tests
#[cfg(test)]
mod tests {
    // use opencv::core::Rect;

    use std::assert_matches::assert_matches;

    use platforms::windows::KeyKind;

    use super::{PlayerState, UseKey, UseKeyStage, update_use_key_context};
    use crate::{
        ActionKeyDirection, ActionKeyWith, KeyBinding,
        context::{Context, MockKeySender},
        detect::MockDetector,
        player::{Player, Timeout, update_non_positional_context},
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
    fn up_jumping() {
        // TODO
    }

    #[test]
    fn falling() {
        // TODO
    }

    #[test]
    fn use_key_ensure_use_with() {
        let mut state = PlayerState::default();
        let context = Context::default();
        let use_key = UseKey {
            key: KeyBinding::A,
            link_key: None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            stage: UseKeyStage::Precondition,
        };

        // ensuring use with start
        let mut player = Player::UseKey(use_key);
        player = update_non_positional_context(
            player,
            &context,
            &MockDetector::new(),
            &mut state,
            false,
        )
        .unwrap();
        assert_matches!(
            player,
            Player::UseKey(UseKey {
                stage: UseKeyStage::EnsuringUseWith,
                ..
            })
        );

        // ensuring use with complete
        state.is_stationary = true;
        player = update_non_positional_context(
            player,
            &context,
            &MockDetector::new(),
            &mut state,
            false,
        )
        .unwrap();
        assert_matches!(
            player,
            Player::UseKey(UseKey {
                stage: UseKeyStage::Precondition,
                ..
            })
        );
    }

    #[test]
    fn use_key_change_direction() {
        let mut keys = MockKeySender::new();
        keys.expect_send_down()
            .withf(|key| matches!(key, KeyKind::Left))
            .returning(|_| Ok(()));
        keys.expect_send_up()
            .withf(|key| matches!(key, KeyKind::Left))
            .returning(|_| Ok(()));
        let mut state = PlayerState::default();
        let context = Context {
            keys: Box::leak(Box::new(keys)),
            ..Context::default()
        };
        let use_key = UseKey {
            key: KeyBinding::A,
            link_key: None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Left,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            stage: UseKeyStage::Precondition,
        };

        // changing direction
        let mut player = Player::UseKey(use_key);
        player = update_non_positional_context(
            player,
            &context,
            &MockDetector::new(),
            &mut state,
            false,
        )
        .unwrap();
        assert_matches!(state.last_known_direction, ActionKeyDirection::Any);
        assert_matches!(
            player,
            Player::UseKey(UseKey {
                stage: UseKeyStage::ChangingDirection(Timeout { started: false, .. }),
                ..
            })
        );

        // changing direction start
        player = update_non_positional_context(
            player,
            &context,
            &MockDetector::new(),
            &mut state,
            false,
        )
        .unwrap();
        assert_matches!(state.last_known_direction, ActionKeyDirection::Any);
        assert_matches!(
            player,
            Player::UseKey(UseKey {
                stage: UseKeyStage::ChangingDirection(Timeout { started: true, .. }),
                ..
            })
        );

        // changing direction complete
        let mut player = Player::UseKey(UseKey {
            stage: UseKeyStage::ChangingDirection(Timeout {
                started: true,
                current: 2,
                total: 2,
            }),
            ..use_key
        });
        player = update_non_positional_context(
            player,
            &context,
            &MockDetector::new(),
            &mut state,
            false,
        )
        .unwrap();
        assert_matches!(state.last_known_direction, ActionKeyDirection::Left);
        assert_matches!(
            player,
            Player::UseKey(UseKey {
                stage: UseKeyStage::Precondition,
                ..
            })
        )
    }

    #[test]
    fn use_key_count() {
        let mut keys = MockKeySender::new();
        keys.expect_send()
            .times(100)
            .withf(|key| matches!(key, KeyKind::A))
            .returning(|_| Ok(()));
        let mut state = PlayerState::default();
        let context = Context {
            keys: Box::leak(Box::new(keys)),
            ..Context::default()
        };
        let use_key = UseKey {
            key: KeyBinding::A,
            link_key: None,
            count: 100,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 0,
            wait_after_use_ticks: 0,
            stage: UseKeyStage::Precondition,
        };

        let mut player = Player::UseKey(use_key);
        for i in 0..100 {
            player = update_non_positional_context(
                player,
                &context,
                &MockDetector::new(),
                &mut state,
                false,
            )
            .unwrap();
            assert_matches!(
                player,
                Player::UseKey(UseKey {
                    stage: UseKeyStage::Using(_, _),
                    ..
                })
            );
            player = update_non_positional_context(
                player,
                &context,
                &MockDetector::new(),
                &mut state,
                false,
            )
            .unwrap();
            assert_matches!(
                player,
                Player::UseKey(UseKey {
                    stage: UseKeyStage::PostCondition,
                    ..
                })
            );
            player = update_non_positional_context(
                player,
                &context,
                &MockDetector::new(),
                &mut state,
                false,
            )
            .unwrap();
            if i == 99 {
                assert_matches!(player, Player::Idle);
            } else {
                assert_matches!(
                    player,
                    Player::UseKey(UseKey {
                        stage: UseKeyStage::Precondition,
                        ..
                    })
                );
            }
        }
    }

    #[test]
    fn use_key_stalling() {
        let mut keys = MockKeySender::new();
        keys.expect_send()
            .withf(|key| matches!(key, KeyKind::A))
            .return_once(|_| Ok(()));
        let mut state = PlayerState::default();
        let context = Context {
            keys: Box::leak(Box::new(keys)),
            ..Context::default()
        };
        let use_key = UseKey {
            key: KeyBinding::A,
            link_key: None,
            count: 1,
            current_count: 0,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 10,
            wait_after_use_ticks: 20,
            stage: UseKeyStage::Precondition,
        };

        // enter stalling state
        assert!(state.stalling_timeout_state.is_none());
        assert_matches!(
            update_use_key_context(&context, &mut state, use_key),
            Player::Stalling(_, 10)
        );
        assert_matches!(
            state.stalling_timeout_state,
            Some(Player::UseKey(UseKey {
                stage: UseKeyStage::Using(_, false),
                ..
            }))
        );

        // complete before stalling state and send key
        assert_matches!(
            update_non_positional_context(
                state.stalling_timeout_state.take().unwrap(),
                &context,
                &MockDetector::new(),
                &mut state,
                false
            ),
            Some(Player::Stalling(_, 20))
        );
        assert_matches!(
            state.stalling_timeout_state,
            Some(Player::UseKey(UseKey {
                stage: UseKeyStage::PostCondition,
                ..
            }))
        );

        // complete after stalling state and return idle
        assert_matches!(
            update_non_positional_context(
                state.stalling_timeout_state.take().unwrap(),
                &context,
                &MockDetector::new(),
                &mut state,
                false
            ),
            Some(Player::Idle)
        );
    }
}
