use std::{collections::HashMap, range::Range};

use anyhow::Result;
use log::debug;
use opencv::core::{Point, Rect};
use platforms::windows::KeyKind;
use rand::seq::IteratorRandom;

use super::{
    DOUBLE_JUMP_THRESHOLD, JUMP_THRESHOLD, MOVE_TIMEOUT, Player, PlayerAction,
    double_jump::DOUBLE_JUMP_AUTO_MOB_THRESHOLD, fall::FALLING_THRESHOLD, timeout::Timeout,
};
use crate::{
    ActionKeyDirection, Class, Position,
    buff::{Buff, BuffKind},
    context::Context,
    detect::ArrowsState,
    minimap::Minimap,
    network::NotificationKind,
    player::{moving::find_intermediate_points, timeout::update_with_timeout},
    task::{Task, Update, update_detection_task},
};

/// The maximum number of times rune solving can fail before transition to
/// `Player::CashShopThenExit`
pub const MAX_RUNE_FAILED_COUNT: u32 = 8;

const HORIZONTAL_MOVEMENT_REPEAT_COUNT: u32 = 20;

const VERTICAL_MOVEMENT_REPEAT_COUNT: u32 = 8;

/// The number of times a reachable y must successfuly ensures the player moves to that exact y
///
/// Once the count is reached, it is considered "solidified" and guaranteed the reachable y is
/// always a y that has platform(s)
const AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT: u32 = 4;

/// The number of times an auto-mob position has made the player aborted the auto-mob action
///
/// If the count is reached, subsequent auto-mob position falling within the x range will be ignored
const AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT: u32 = 3;

/// The range an ignored auto-mob x position spans
///
/// If an auto-mob x position is 5, then the range is [2, 8]
const AUTO_MOB_IGNORE_XS_RANGE: i32 = 3;

/// The maximum of number points for auto mobbing to periodically move to
const AUTO_MOB_MAX_PATHING_POINTS: usize = 3;

/// The acceptable y range above and below the detected mob position when matched with a reachable y
const AUTO_MOB_REACHABLE_Y_THRESHOLD: i32 = 10;

const AUTO_MOB_HORIZONTAL_MOVEMENT_REPEAT_COUNT: u32 = 4;

const AUTO_MOB_VERTICAL_MOVEMENT_REPEAT_COUNT: u32 = 3;

/// Maximum number of times [`Player::Moving`] state can be transitioned to
/// without changing position
const UNSTUCK_TRACKER_THRESHOLD: u32 = 7;

/// The number of times [`Player::Unstucking`] can be transitioned to before entering GAMBA MODE
const UNSTUCK_GAMBA_MODE_COUNT: u32 = 3;

/// The player previous movement-related contextual state
#[derive(Clone, Copy, Eq, PartialEq, Hash, Debug)]
pub enum LastMovement {
    Adjusting,
    DoubleJumping,
    Falling,
    Grappling,
    UpJumping,
    Jumping,
}

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
    /// Uses platforms to compute auto mobbing bound
    ///
    /// TODO: This shouldn't be here...
    pub auto_mob_platforms_bound: bool,
    /// The interact key
    pub interact_key: KeyKind,
    /// The RopeLift key
    pub grappling_key: KeyKind,
    /// The teleport key with [`None`] indicating double jump
    pub teleport_key: Option<KeyKind>,
    /// The jump key
    ///
    /// Replaces the previously default [`KeyKind::Space`] key
    pub jump_key: KeyKind,
    /// The up jump key with [`None`] indicating composite jump (Up arrow + Double Space)
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
///
/// TODO: Should have a separate struct or trait for Rotator to access PlayerState
/// TODO: Counter should not be u32 but usize?
/// TODO: Reduce visibility to private for complex states
#[derive(Debug, Default)]
pub struct PlayerState {
    pub config: PlayerConfiguration,
    /// The id of the normal action provided by [`Rotator`]
    normal_action_id: u32,
    /// A normal action requested by [`Rotator`]
    pub(super) normal_action: Option<PlayerAction>,
    /// The id of the priority action provided by [`Rotator`]
    priority_action_id: u32,
    /// A priority action requested by [`Rotator`]
    ///
    /// This action will override the normal action if it is in the middle of executing.
    pub(super) priority_action: Option<PlayerAction>,
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
    pub(super) is_stationary: bool,
    /// Whether the player is dead
    pub is_dead: bool,
    /// The task for detecting if player is dead
    is_dead_task: Option<Task<Result<bool>>>,
    /// Approximates the player direction for using key
    pub(super) last_known_direction: ActionKeyDirection,
    /// Tracks last destination points for displaying to UI
    ///
    /// Resets when all destinations are reached or in [`Player::Idle`]
    pub last_destinations: Option<Vec<Point>>,
    /// Last known position after each detection used for unstucking, also for displaying to UI
    pub last_known_pos: Option<Point>,
    /// Indicates whether to use [`ControlFlow::Immediate`] on this update
    pub(super) use_immediate_control_flow: bool,
    /// Indicates whether to ignore update_pos and use last_known_pos on next update
    ///
    /// This is true whenever [`Self::use_immediate_control_flow`] is true
    pub(super) ignore_pos_update: bool,
    /// Indicates whether to reset the contextual state back to [`Player::Idle`] on next update
    ///
    /// This is true each time player receives [`PlayerAction`]
    pub(super) reset_to_idle_next_update: bool,
    /// Indicates the last movement
    ///
    /// Helps coordinating between movement states (e.g. falling + double jumping). And resets
    /// to [`None`] when the destination (possibly intermediate) is reached or
    /// in [`Player::Idle`].
    pub(super) last_movement: Option<LastMovement>,
    // TODO: 2 maps fr?
    /// Tracks [`Self::last_movement`] to abort normal action when its position is not accurate
    ///
    /// Clears when a normal action is completed or aborted
    last_movement_normal_map: HashMap<LastMovement, u32>,
    /// Tracks [`Self::last_movement`] to abort priority action when its position is not accurate
    ///
    /// Clears when a priority action is completed or aborted
    last_movement_priority_map: HashMap<LastMovement, u32>,
    /// Tracks a map of "reachable" y
    ///
    /// A y is reachable if there is a platform the player can stand on
    auto_mob_reachable_y_map: HashMap<i32, u32>,
    /// The matched reachable y and also the key in [`Self::auto_mob_reachable_y_map`]
    auto_mob_reachable_y: Option<i32>,
    /// Tracks a map of reachable y to x ranges that can be ignored
    ///
    /// This will help auto-mobbing ignores positions that are known to be not reachable
    auto_mob_ignore_xs_map: HashMap<i32, Vec<(Range<i32>, u32)>>,
    /// Stores points to periodically move to when auto mobbing
    ///
    /// Helps changing location for detecting more mobs. It is populated in terminal state of
    /// [`Player::UseKey`].
    auto_mob_pathing_points: Vec<Point>,
    /// Tracks whether movement-related actions do not change the player position after a while.
    ///
    /// Resets when a limit is reached (for unstucking) or position did change.
    unstuck_count: u32,
    /// The number of times player transtioned to [`Player::Unstucking`]
    ///
    /// Resets when threshold reached or position changed
    unstuck_transitioned_count: u32,
    /// Unstuck task for detecting settings when mis-pressing ESC key
    pub(super) unstuck_task: Option<Task<Result<bool>>>,
    /// Rune solving task
    pub(super) rune_task: Option<Task<Result<ArrowsState>>>,
    /// The number of times [`Player::SolvingRune`] failed
    pub(super) rune_failed_count: u32,
    /// Indicates the state will be transitioned to [`Player::CashShopThenExit`] in the next tick
    pub(super) rune_cash_shop: bool,
    /// [`Timeout`] for validating whether the rune is solved
    ///
    /// This is [`Some`] when [`Player::SolvingRune`] successfully detects the rune
    /// and sends all the keys
    pub(super) rune_validate_timeout: Option<Timeout>,
    /// A state to return to after stalling
    ///
    /// Resets when [`Player::Stalling`] timed out or in [`Player::Idle`]
    pub(super) stalling_timeout_state: Option<Player>,
}

impl PlayerState {
    /// Resets the player state except for configuration
    ///
    /// Used whenever minimap data or configuration changes
    #[inline]
    pub fn reset(&mut self) {
        *self = PlayerState {
            config: self.config,
            reset_to_idle_next_update: true,
            ..PlayerState::default()
        };
    }

    /// The normal action name for displaying to UI
    #[inline]
    pub fn normal_action_name(&self) -> Option<String> {
        self.normal_action.map(|action| action.to_string())
    }

    /// The normal action id provided by [`Rotator`]
    #[inline]
    pub fn normal_action_id(&self) -> Option<u32> {
        self.has_normal_action().then_some(self.normal_action_id)
    }

    /// Whether is a normal action
    #[inline]
    pub fn has_normal_action(&self) -> bool {
        self.normal_action.is_some()
    }

    /// Sets the normal action to `id` and `action` and resets to [`Player::Idle`] on next update
    #[inline]
    pub fn set_normal_action(&mut self, id: u32, action: PlayerAction) {
        self.reset_to_idle_next_update = true;
        self.normal_action_id = id;
        self.normal_action = Some(action);
    }

    /// Removes the current normal action
    #[inline]
    pub fn reset_normal_action(&mut self) {
        self.reset_to_idle_next_update = true;
        self.normal_action = None;
    }

    /// The priority action name for displaying to UI
    #[inline]
    pub fn priority_action_name(&self) -> Option<String> {
        self.priority_action.map(|action| action.to_string())
    }

    /// The priority action id provided by [`Rotator`]
    #[inline]
    pub fn priority_action_id(&self) -> Option<u32> {
        self.has_priority_action()
            .then_some(self.priority_action_id)
    }

    /// Whether there is a priority action
    #[inline]
    pub fn has_priority_action(&self) -> bool {
        self.priority_action.is_some()
    }

    /// Sets the priority action to `id` and `action` and resets to [`Player::Idle`] on next update
    #[inline]
    pub fn set_priority_action(&mut self, id: u32, action: PlayerAction) {
        let _ = self.replace_priority_action(id, action);
    }

    /// Removes the current priority action and returns its id if there is one.
    #[inline]
    pub fn take_priority_action(&mut self) -> Option<u32> {
        self.reset_to_idle_next_update = true;
        self.priority_action
            .take()
            .is_some()
            .then_some(self.priority_action_id)
    }

    /// Replaces the current priority action with `id` and `action` and returns the previous
    /// action id if there is one.
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

    /// Whether the player is validating whether the rune is solved
    #[inline]
    pub fn is_validating_rune(&self) -> bool {
        self.rune_validate_timeout.is_some()
    }

    /// Whether there is a priority rune action
    #[inline]
    pub fn has_rune_action(&self) -> bool {
        matches!(self.priority_action, Some(PlayerAction::SolveRune))
    }

    /// Whether there is only auto mob action
    #[inline]
    pub(super) fn has_auto_mob_action_only(&self) -> bool {
        !self.has_priority_action() && matches!(self.normal_action, Some(PlayerAction::AutoMob(_)))
    }

    /// Clears both on-going normal and priority actions due to being aborted
    #[inline]
    pub fn clear_actions_aborted(&mut self) {
        self.reset_to_idle_next_update = true;
        self.priority_action = None;
        self.normal_action = None;
    }

    /// Clears either normal or priority due to completion
    #[inline]
    pub(super) fn clear_action_completed(&mut self) {
        self.clear_last_movement();
        if self.has_priority_action() {
            self.priority_action = None;
        } else {
            self.auto_mob_reachable_y = None;
            self.normal_action = None;
        }
    }

    /// Clears the last movement tracking for either normal or priority action
    #[inline]
    pub(super) fn clear_last_movement(&mut self) {
        if self.has_priority_action() {
            self.last_movement_priority_map.clear();
        } else {
            self.last_movement_normal_map.clear();
        }
    }

    #[inline]
    pub(super) fn clear_unstucking(&mut self, include_transitioned_count: bool) {
        self.unstuck_count = 0;
        if include_transitioned_count {
            self.unstuck_transitioned_count = 0;
        }
    }

    /// Increments the rune validation fail count and sets [`PlayerState::rune_cash_shop`] if needed
    #[inline]
    pub(super) fn track_rune_fail_count(&mut self) {
        self.rune_failed_count += 1;
        if self.rune_failed_count >= MAX_RUNE_FAILED_COUNT {
            self.rune_failed_count = 0;
            self.rune_cash_shop = true;
        }
    }

    /// Increments the unstucking transitioned counter
    ///
    /// Returns `true` when [`Player::Unstucking`] should enter GAMBA MODE
    #[inline]
    pub(super) fn track_unstucking_transitioned(&mut self) -> bool {
        self.unstuck_transitioned_count += 1;
        if self.unstuck_transitioned_count >= UNSTUCK_GAMBA_MODE_COUNT {
            self.unstuck_transitioned_count = 0;
            true
        } else {
            false
        }
    }

    /// Increments the unstucking counter
    ///
    /// Returns `true` when the player should transition to [`Player::Unstucking`]
    #[inline]
    pub(super) fn track_unstucking(&mut self) -> bool {
        self.unstuck_count += 1;
        if self.unstuck_count >= UNSTUCK_TRACKER_THRESHOLD {
            self.unstuck_count = 0;
            true
        } else {
            false
        }
    }

    /// Tracks the last movement to determine whether the state has repeated passing a threshold
    #[inline]
    pub(super) fn track_last_movement_repeated(&mut self) -> bool {
        if self.last_movement.is_none() {
            return false;
        }

        let last_movement = self.last_movement.unwrap();
        let count_max = match last_movement {
            LastMovement::Adjusting | LastMovement::DoubleJumping => {
                if self.has_auto_mob_action_only() {
                    AUTO_MOB_HORIZONTAL_MOVEMENT_REPEAT_COUNT
                } else {
                    HORIZONTAL_MOVEMENT_REPEAT_COUNT
                }
            }
            LastMovement::Falling
            | LastMovement::Grappling
            | LastMovement::UpJumping
            | LastMovement::Jumping => {
                if self.has_auto_mob_action_only() {
                    AUTO_MOB_VERTICAL_MOVEMENT_REPEAT_COUNT
                } else {
                    VERTICAL_MOVEMENT_REPEAT_COUNT
                }
            }
        };

        let count_map = if self.has_priority_action() {
            &mut self.last_movement_priority_map
        } else {
            &mut self.last_movement_normal_map
        };
        let count = count_map.entry(last_movement).or_insert(0);
        if *count < count_max {
            *count += 1;
        }
        let count = *count;
        debug!(target: "player", "last movement {:?}", count_map);
        count >= count_max
    }

    /// Gets the falling minimum `y` distance threshold
    ///
    /// In auto mob or intermediate destination, the threshold is relaxed for more
    /// fluid movement.
    #[inline]
    pub(super) fn falling_threshold(&self, is_intermediate: bool) -> i32 {
        if self.has_auto_mob_action_only() || is_intermediate {
            JUMP_THRESHOLD
        } else {
            FALLING_THRESHOLD
        }
    }

    /// Gets the double jump minimum `x` distance threshold
    ///
    /// In auto mob and final destination, the threshold is relaxed for more
    /// fluid movement.
    #[inline]
    pub(super) fn double_jump_threshold(&self, is_intermediate: bool) -> i32 {
        if self.has_auto_mob_action_only() && !is_intermediate {
            DOUBLE_JUMP_AUTO_MOB_THRESHOLD
        } else {
            DOUBLE_JUMP_THRESHOLD
        }
    }

    #[inline]
    pub(super) fn should_disable_grappling(&self) -> bool {
        // FIXME: ....
        (self.has_auto_mob_action_only()
            && self.config.auto_mob_platforms_pathing
            && self.config.auto_mob_platforms_pathing_up_jump_only)
            || (self.has_rune_action()
                && self.config.rune_platforms_pathing
                && self.config.rune_platforms_pathing_up_jump_only)
    }

    /// Picks a pathing point in auto mobbing to move to
    #[inline]
    pub fn auto_mob_pathing_point(&mut self, minimap: Rect) -> Option<Point> {
        let point = self
            .auto_mob_pathing_points
            .iter()
            .enumerate()
            .map(|(i, point)| (i, *point))
            .choose(&mut rand::rng());
        if let Some((i, _)) = point {
            // I don't know guys
            // Just want variations
            if rand::random_bool(0.5) {
                self.auto_mob_pathing_points.remove(i);
            }
        }
        point
            .map(|(_, point)| Point::new(point.x, minimap.height - point.y))
            .or_else(|| {
                // Last resort
                self.auto_mob_reachable_y_map
                    .keys()
                    .min()
                    .map(|y| Point::new(minimap.width / 2, minimap.height - y))
            })
    }

    /// Populates pathing points for an auto mob action
    ///
    /// After using key state is fully complete, it will try to populate a pathing point to be used
    /// when [`Rotator`] fails the mob detection. This will will help [`Rotator`] re-uses the previous
    /// detected mob point for moving to area with more mobs.
    pub(super) fn auto_mob_populate_pathing_points(&mut self, context: &Context) {
        if self.auto_mob_pathing_points.len() >= AUTO_MOB_MAX_PATHING_POINTS
            || self.auto_mob_reachable_y_require_update()
        {
            return;
        }

        let (minimap_width, platforms) = match context.minimap {
            Minimap::Idle(idle) => (idle.bbox.width, idle.platforms),
            _ => unreachable!(),
        };
        // Flip a coin, use platform as pathing point
        if !platforms.is_empty() && rand::random_bool(0.5) {
            let platform = platforms[rand::random_range(0..platforms.len())];
            let xs = platform.xs();
            let y = platform.y();
            let point = Point::new(xs.start.midpoint(xs.end), y);
            // Platform pathing point can bypass y restriction
            if !self
                .auto_mob_pathing_points
                .iter()
                .any(|pt| pt.y == point.y && pt.x == point.x)
            {
                self.auto_mob_pathing_points.push(point);
                debug!(target: "player", "auto mob pathing point from platform {:?}", point);
                return;
            }
        }

        // The idea is to pick a pathing point with a different y from existing points and with x
        // within 70% on both sides from the middle of the minimap
        let minimap_mid = minimap_width / 2;
        let minimap_threshold = (minimap_mid as f32 * 0.7) as i32;
        let pos = self.last_known_pos.unwrap();
        let x_offset = (pos.x - minimap_mid).abs();
        let y = self.auto_mob_reachable_y.unwrap();
        if x_offset > minimap_threshold
            || self
                .auto_mob_pathing_points
                .iter()
                .any(|point| point.y == y)
        {
            return;
        }
        self.auto_mob_pathing_points.push(Point::new(pos.x, y));
        debug!(target: "player", "auto mob pathing points {:?}", self.auto_mob_pathing_points);
    }

    /// Whether the auto mob reachable y requires "solidifying"
    #[inline]
    pub(super) fn auto_mob_reachable_y_require_update(&self) -> bool {
        self.auto_mob_reachable_y.is_none_or(|y| {
            *self.auto_mob_reachable_y_map.get(&y).unwrap() < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT
        })
    }

    /// Picks a reachable y for reaching `mob_pos`
    ///
    /// After calling this function, the state is updated to track the current reachable `y`.
    /// Caller is expected to call [`Self::auto_mob_track_reachable_y`] afterward when the
    /// auto-mob action has completed (e.g. in terminal state of [`Player::UseKey`]).
    ///
    /// Returns [`Player::Moving`] indicating the moving action for the player to reach to mob or
    /// [`Player::Idle`] indicating the action should be aborted.
    pub(super) fn auto_mob_pick_reachable_y_contextual_state(
        &mut self,
        context: &Context,
        mob_pos: Position,
    ) -> Player {
        if self.auto_mob_reachable_y_map.is_empty() {
            self.auto_mob_populate_reachable_y(context);
        }
        debug_assert!(!self.auto_mob_reachable_y_map.is_empty());

        let y = self
            .auto_mob_reachable_y_map
            .keys()
            .copied()
            .min_by_key(|y| (mob_pos.y - y).abs())
            .filter(|y| (mob_pos.y - y).abs() <= AUTO_MOB_REACHABLE_Y_THRESHOLD);

        // Checking whether y is solidified yet is not needed because y will only be added
        // to the xs map when it is solidified. As for populated xs from platforms, the
        // corresponding y must have already been populated.
        if let Some(y) = y
            && self.auto_mob_ignore_xs_map.get(&y).is_some_and(|ranges| {
                ranges.iter().any(|(range, count)| {
                    *count >= AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT && range.contains(&mob_pos.x)
                })
            })
        {
            debug!(target: "player", "auto mob aborted because of wrong mob x position");
            return Player::Idle;
        }

        let point = Point::new(mob_pos.x, y.unwrap_or(mob_pos.y));
        let intermediates = if self.config.auto_mob_platforms_pathing {
            match context.minimap {
                Minimap::Idle(idle) => find_intermediate_points(
                    &idle.platforms,
                    self.last_known_pos.unwrap(),
                    point,
                    mob_pos.allow_adjusting,
                    self.config.auto_mob_platforms_pathing_up_jump_only,
                ),
                _ => unreachable!(),
            }
        } else {
            None
        };
        debug!(target: "player", "auto mob reachable y {:?} {:?}", y, self.auto_mob_reachable_y_map);

        self.auto_mob_reachable_y = y;
        self.last_destinations = intermediates
            .map(|intermediates| {
                intermediates
                    .inner
                    .into_iter()
                    .map(|(point, _)| point)
                    .collect::<Vec<_>>()
            })
            .or(Some(vec![point]));
        intermediates
            .map(|mut intermediates| {
                let (point, exact) = intermediates.next().unwrap();
                Player::Moving(point, exact, Some(intermediates))
            })
            .unwrap_or(Player::Moving(point, mob_pos.allow_adjusting, None))
    }

    fn auto_mob_populate_reachable_y(&mut self, context: &Context) {
        match context.minimap {
            Minimap::Idle(idle) => {
                // Believes in user input lets goo...
                for platform in idle.platforms {
                    self.auto_mob_reachable_y_map
                        .insert(platform.y(), AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT);
                }
            }
            _ => unreachable!(),
        }
        let _ = self.auto_mob_reachable_y_map.try_insert(
            self.last_known_pos.unwrap().y,
            AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT - 1,
        );
        debug!(target: "player", "auto mob initial reachable y map {:?}", self.auto_mob_reachable_y_map);
    }

    /// Tracks the currently picked reachable y to solidify the y position
    ///
    /// After [`Self::auto_mob_pick_reachable_y_moving_state`] has been called in the action entry,
    /// this function should be called in the terminal state of the action.
    pub(super) fn auto_mob_track_reachable_y(&mut self) {
        // state.last_known_pos is explicitly used instead of state.auto_mob_reachable_y
        // because they might not be the same
        if let Some(pos) = self.last_known_pos {
            if self.auto_mob_reachable_y.is_some_and(|y| y != pos.y) {
                let y = self.auto_mob_reachable_y.unwrap();
                let count = self.auto_mob_reachable_y_map.get_mut(&y).unwrap();
                *count = count.saturating_sub(1);
                if *count == 0 {
                    self.auto_mob_reachable_y_map.remove(&y);
                    self.auto_mob_reachable_y = None;
                }
            }

            let count = self.auto_mob_reachable_y_map.entry(pos.y).or_insert(0);
            if *count < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT {
                *count += 1;
            }
            debug_assert!(*count <= AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT);

            debug!(target: "player", "auto mob additional reachable y {} / {}", pos.y, count);
        }
    }

    /// Tracks whether to ignore a x range for the current reachable y
    // TODO: This tracking currently does not clamp to bound, should clamp to non-negative
    pub(super) fn auto_mob_track_ignore_xs(&mut self, context: &Context, is_aborted: bool) {
        if !self.has_auto_mob_action_only() {
            return;
        }
        if self.auto_mob_ignore_xs_map.is_empty() {
            self.auto_mob_populate_ignore_xs(context);
        }

        let Some(y) = self.auto_mob_reachable_y else {
            return;
        };
        if *self.auto_mob_reachable_y_map.get(&y).unwrap() < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT {
            return;
        }

        let x = match self.normal_action.unwrap() {
            PlayerAction::AutoMob(mob) => mob.position.x,
            PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune => {
                unreachable!()
            }
        };
        let vec = self
            .auto_mob_ignore_xs_map
            .entry(y)
            .or_insert_with(|| vec![auto_mob_ignore_xs_range_value(x)]);

        if is_aborted
            && vec.len() >= 2
            && vec.iter().array_chunks::<2>().any(
                |[(first_range, first_count), (second_range, second_count)]| {
                    second_range.start < first_range.end
                        && (*first_count >= AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT
                            || *second_count >= AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT)
                },
            )
        {
            // Merge overlapping adjacent ranges with the same y
            let mut merged = Vec::<(Range<i32>, u32)>::new();
            for (range, count) in vec.drain(..) {
                if let Some((last_range, last_count)) = merged.last_mut() {
                    // Checking range start less than last_range end is sufficient because
                    // these ranges are previously sorted and are never empty
                    let overlapping = range.start < last_range.end;
                    let should_merge = (*last_count >= AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT)
                        || (count >= AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT);

                    if overlapping && should_merge {
                        last_range.end = last_range.end.max(range.end);
                        *last_count = AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT;
                        continue;
                    }
                }
                merged.push((range, count));
            }
            *vec = merged;
            debug!(target: "player", "auto mob merged ignore xs {y} = {vec:?}");
        }

        if let Some((i, (_, count))) = vec
            .iter_mut()
            .enumerate()
            .find(|(_, (xs, _))| xs.contains(&x))
        {
            if *count < AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT {
                *count = if is_aborted {
                    count.saturating_add(1)
                } else {
                    count.saturating_sub(1)
                };
                if !is_aborted && *count == 0 {
                    vec.remove(i);
                }
                debug!(target: "player", "auto mob updated ignore xs {:?}", self.auto_mob_ignore_xs_map);
            }
            return;
        }

        if is_aborted {
            let (range, count) = auto_mob_ignore_xs_range_value(x);
            vec.push((range, count + 1));
            vec.sort_by_key(|(r, _)| r.start);
            debug!(target: "player", "auto mob new ignore xs {:?}", self.auto_mob_ignore_xs_map);
        }
    }

    pub(super) fn auto_mob_populate_ignore_xs(&mut self, context: &Context) {
        let (platforms, minimap_width) = match context.minimap {
            Minimap::Idle(idle) => (idle.platforms, idle.bbox.width),
            Minimap::Detecting => unreachable!(),
        };
        if platforms.is_empty() {
            return;
        }

        // Group platform ranges by y
        let mut y_map: HashMap<i32, Vec<Range<i32>>> = HashMap::new();
        for platform in platforms {
            y_map.entry(platform.y()).or_default().push(platform.xs());
        }

        for (y, mut ranges) in y_map {
            // Sort by start of the range
            ranges.sort_by_key(|r| r.start);

            let mut last_end = ranges[0].end;
            let ignores = self.auto_mob_ignore_xs_map.entry(y).or_default();

            let first_gap = 0..ranges[0].start;
            if !first_gap.is_empty() {
                ignores.push((first_gap.into(), AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT));
            }

            let last_gap = ranges.last().unwrap().end..minimap_width;
            if !last_gap.is_empty() {
                ignores.push((last_gap.into(), AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT));
            }

            for r in ranges.into_iter().skip(1) {
                if r.start > last_end {
                    let gap = last_end..r.start;
                    if !gap.is_empty() {
                        ignores.push((gap.into(), AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT));
                    }
                }
                last_end = last_end.max(r.end);
            }
        }
    }

    /// Updates the [`PlayerState`] on each tick.
    ///
    /// This function updates the player states including current position, health, whether the
    /// player is dead, stationary state and rune validation state. It also resets
    /// [`PlayerState::unstuck_counter`] and [`PlayerState::unstuck_consecutive_counter`] when the
    /// player position changes.
    #[inline]
    pub(super) fn update_state(&mut self, context: &Context) -> bool {
        if self.update_position_state(context) {
            self.update_health_state(context);
            self.update_rune_validating_state(context);
            self.update_is_dead_state(context);
            return true;
        }
        false
    }

    /// Updates the player current position
    ///
    /// The player position (as well as other positions in relation to the player) does not follow
    /// OpenCV top-left coordinate but flipped to bottom-left by subtracting the minimap height
    /// with the y position. This is more intuitive both for the UI and development experience.
    #[inline]
    fn update_position_state(&mut self, context: &Context) -> bool {
        let minimap_bbox = match &context.minimap {
            Minimap::Detecting => return false,
            Minimap::Idle(idle) => idle.bbox,
        };
        let Ok(player_bbox) = context.detector_unwrap().detect_player(minimap_bbox) else {
            return false;
        };
        let tl = player_bbox.tl();
        let br = player_bbox.br();
        let x = (tl.x + br.x) / 2;
        // The native coordinate of OpenCV is top-left and this flips to bottom-left for
        // for better intution to the UI. All player states and actions also operate on this
        // bottom-left coordinate.
        //
        // TODO: Should keep original coordinate? And flips before passing to UI?
        let y = minimap_bbox.height - br.y;
        let pos = Point::new(x, y);
        let last_known_pos = self.last_known_pos.unwrap_or(pos);
        if last_known_pos != pos {
            self.unstuck_count = 0;
            self.unstuck_transitioned_count = 0;
            self.is_stationary_timeout = Timeout::default();
        }

        let (is_stationary, is_stationary_timeout) = update_with_timeout(
            self.is_stationary_timeout,
            MOVE_TIMEOUT,
            |timeout| (false, timeout),
            || (true, self.is_stationary_timeout),
            |timeout| (false, timeout),
        );
        self.is_stationary = is_stationary;
        self.is_stationary_timeout = is_stationary_timeout;
        self.last_known_pos = Some(pos);
        true
    }

    /// Updates the rune validation [`Timeout`]
    ///
    /// [`PlayerState::rune_validate_timeout`] is [`Some`] only when [`Player::SolvingRune`]
    /// successfully detects and sends all the keys. After about 12 seconds, it
    /// will check if the player has the rune buff.
    #[inline]
    fn update_rune_validating_state(&mut self, context: &Context) {
        const VALIDATE_TIMEOUT: u32 = 375;

        debug_assert!(self.rune_failed_count < MAX_RUNE_FAILED_COUNT);
        debug_assert!(!self.rune_cash_shop);
        self.rune_validate_timeout = self.rune_validate_timeout.and_then(|timeout| {
            update_with_timeout(
                timeout,
                VALIDATE_TIMEOUT,
                Some,
                || {
                    if matches!(context.buffs[BuffKind::Rune], Buff::NoBuff) {
                        self.track_rune_fail_count();
                    } else {
                        self.rune_failed_count = 0;
                    }
                    None
                },
                Some,
            )
        });
    }

    /// Updates the player current health
    ///
    /// The detection first detects the HP bar and caches the result. The HP bar is then used
    /// to crop into the game image and detects the current health bar and max health bar. These
    /// bars are then cached and used to extract the current health and max health.
    // TODO: This should be a PlayerAction?
    #[inline]
    fn update_health_state(&mut self, context: &Context) {
        if let Player::SolvingRune(_) = context.player {
            return;
        }
        if self.config.use_potion_below_percent.is_none() {
            {
                let this = &mut *self;
                this.health = None;
                this.health_task = None;
                this.health_bar = None;
                this.health_bar_task = None;
            };
            return;
        }

        let Some(health_bar) = self.health_bar else {
            let update =
                update_detection_task(context, 1000, &mut self.health_bar_task, move |detector| {
                    detector.detect_player_health_bar()
                });
            if let Update::Ok(health_bar) = update {
                self.health_bar = Some(health_bar);
            }
            return;
        };

        let Update::Ok(health) = update_detection_task(
            context,
            self.config.update_health_millis.unwrap_or(1000),
            &mut self.health_task,
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

        let percentage = self.config.use_potion_below_percent.unwrap();
        let (current, max) = health;
        let ratio = current as f32 / max as f32;

        self.health = Some(health);
        if ratio <= percentage {
            let _ = context.keys.send(self.config.potion_key);
        }
    }

    /// Updates whether the player is dead
    ///
    /// Upon being dead, a notification will be scheduled to notify the user.
    #[inline]
    fn update_is_dead_state(&mut self, context: &Context) {
        let Update::Ok(is_dead) =
            update_detection_task(context, 5000, &mut self.is_dead_task, |detector| {
                Ok(detector.detect_player_is_dead())
            })
        else {
            return;
        };
        if is_dead && !self.is_dead {
            let _ = context
                .notification
                .schedule_notification(NotificationKind::PlayerIsDead);
        }
        self.is_dead = is_dead;
    }
}

#[inline]
fn auto_mob_ignore_xs_range_value(x: i32) -> (Range<i32>, u32) {
    let x_start = x - AUTO_MOB_IGNORE_XS_RANGE;
    let x_end = x + AUTO_MOB_IGNORE_XS_RANGE + 1;
    let range = x_start..x_end;
    (range.into(), 0)
}

#[cfg(test)]
mod tests {
    use std::{assert_matches::assert_matches, collections::HashMap};

    use opencv::core::{Point, Rect};

    use crate::{
        Position,
        array::Array,
        context::Context,
        minimap::{Minimap, MinimapIdle},
        pathing::{Platform, find_neighbors},
        player::{Player, PlayerAction, PlayerActionAutoMob, PlayerState},
    };

    #[test]
    fn auto_mob_pick_reachable_y_should_ignore_solidified_x_range() {
        let context = Context::new(None, None);
        let mut state = PlayerState {
            auto_mob_reachable_y_map: HashMap::from([(50, 1)]),
            auto_mob_ignore_xs_map: HashMap::from([(50, vec![((53..58).into(), 3)])]),
            ..Default::default()
        };

        assert_matches!(
            state.auto_mob_pick_reachable_y_contextual_state(
                &context,
                Position {
                    x: 55,
                    y: 50,
                    ..Default::default()
                },
            ),
            Player::Idle
        );
    }

    #[test]
    fn auto_mob_pick_reachable_y_in_threshold() {
        let context = Context::new(None, None);
        let mut state = PlayerState {
            auto_mob_reachable_y_map: [100, 120, 150].into_iter().map(|y| (y, 1)).collect(),
            last_known_pos: Some(Point::new(0, 0)),
            ..Default::default()
        };
        let mob_pos = Position {
            x: 50,
            y: 125,
            ..Default::default()
        };

        // Expect 120 to be chosen since it's closest to 125
        assert_matches!(
            state.auto_mob_pick_reachable_y_contextual_state(&context, mob_pos),
            Player::Moving(Point { x: 50, y: 120 }, false, None)
        );
        assert_eq!(state.auto_mob_reachable_y, Some(120));
    }

    #[test]
    fn auto_mob_pick_reachable_y_out_of_threshold() {
        let context = Context::new(None, None);
        let mut state = PlayerState {
            auto_mob_reachable_y_map: [1000, 2000].into_iter().map(|y| (y, 1)).collect(),
            last_known_pos: Some(Point::new(0, 0)),
            ..Default::default()
        };
        let mob_pos = Position {
            x: 50,
            y: 125,
            ..Default::default()
        };

        // No y value is chosen so the original y is used
        assert_matches!(
            state.auto_mob_pick_reachable_y_contextual_state(&context, mob_pos),
            Player::Moving(Point { x: 50, y: 125 }, false, None)
        );
        assert_eq!(state.auto_mob_reachable_y, None);
    }

    #[test]
    fn auto_mob_track_reachable_y() {
        let mut player = PlayerState {
            auto_mob_reachable_y: Some(100),
            auto_mob_reachable_y_map: HashMap::from([
                (100, 1), // Will be decremented and removed
                (120, 2), // Will be incremented
            ]),
            last_known_pos: Some(Point::new(0, 120)), // y != auto_mob_reachable_y
            ..Default::default()
        };

        player.auto_mob_track_reachable_y();

        // The old reachable y (100) should be removed
        assert!(!player.auto_mob_reachable_y_map.contains_key(&100));
        // The current position y (120) should be incremented
        assert_eq!(player.auto_mob_reachable_y_map.get(&120), Some(&3));
        // auto_mob_reachable_y should be cleared
        assert_eq!(player.auto_mob_reachable_y, None);
    }

    #[test]
    fn auto_mob_track_ignore_xs_conditional_merge() {
        let y = 100;
        let context = Context::new(None, None);
        let mut player = PlayerState {
            normal_action: Some(PlayerAction::AutoMob(PlayerActionAutoMob {
                position: Position {
                    x: 50,
                    y,
                    ..Default::default()
                },
                ..Default::default()
            })),
            auto_mob_reachable_y: Some(y),
            auto_mob_reachable_y_map: HashMap::from([(y, 4)]), // 4 = solidify
            auto_mob_ignore_xs_map: HashMap::from([(
                y,
                vec![
                    ((45..55).into(), 3), // 3 = solidify
                    ((54..64).into(), 1), // not solidified, but overlaps
                ],
            )]),
            ..Default::default()
        };

        player.auto_mob_track_ignore_xs(&context, true);

        let ranges = player.auto_mob_ignore_xs_map.get(&y).unwrap();
        assert_eq!(ranges.len(), 1); // Should be merged
        assert_eq!(ranges[0].0, (45..64).into());

        // Now test that they don’t merge if neither is solidified
        player.normal_action = Some(PlayerAction::AutoMob(PlayerActionAutoMob {
            position: Position {
                x: 60,
                y,
                ..Default::default()
            },
            ..Default::default()
        }));
        player.auto_mob_ignore_xs_map = HashMap::from([(
            y,
            vec![
                ((55..65).into(), 1), // not solidified but incremented because of 60
                ((63..75).into(), 1), // not solidified, overlapping adjacent
            ],
        )]);

        player.auto_mob_track_ignore_xs(&context, true);

        let ranges = player.auto_mob_ignore_xs_map.get(&y).unwrap();
        assert_eq!(ranges.len(), 2); // Should remain unmerged but incremented
        assert_eq!(ranges, &vec![((55..65).into(), 2), ((63..75).into(), 1)])
    }

    #[test]
    fn auto_mob_populate_ignore_xs_detects_gaps_correctly() {
        let platforms = vec![
            Platform::new(1..5, 10),
            Platform::new(10..15, 10),
            Platform::new(20..25, 10),
            Platform::new(0..10, 5), // A different y-level
        ];
        let platforms = find_neighbors(&platforms, 25, 7, 41);

        let mut idle = MinimapIdle::default();
        idle.platforms = Array::from_iter(platforms);
        idle.bbox = Rect::new(0, 0, 100, 100);

        let context = Context {
            minimap: Minimap::Idle(idle),
            ..Context::new(None, None)
        };

        let mut state = PlayerState::default();
        state.auto_mob_populate_ignore_xs(&context);

        let map = &state.auto_mob_ignore_xs_map;

        assert_eq!(map.len(), 2);
        let gaps = map.get(&10).unwrap();
        assert_eq!(gaps.len(), 4);
        assert_eq!(gaps[0].0, (0..1).into());
        assert_eq!(gaps[1].0, (25..100).into());
        assert_eq!(gaps[2].0, (5..10).into());
        assert_eq!(gaps[3].0, (15..20).into());

        let gaps = map.get(&5).unwrap();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].0, (10..100).into());
    }
}
