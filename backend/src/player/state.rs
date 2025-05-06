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
    ActionKeyDirection, Class,
    buff::{Buff, BuffKind},
    context::Context,
    detect::ArrowsState,
    minimap::Minimap,
    network::NotificationKind,
    player::timeout::update_with_timeout,
    task::{Task, Update, update_detection_task},
};

/// The maximum number of times rune solving can fail before transition to
/// `Player::CashShopThenExit`
pub const MAX_RUNE_FAILED_COUNT: u32 = 8;

/// The number of times a reachable y must successfuly ensures the player moves to that exact y
///
/// Once the count is reached, it is considered "solidified" and guaranteed the reachable y is
/// always a y that has platform(s)
pub const AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT: u32 = 4;

/// The number of times an auto-mob position has made the player aborted the auto-mob action
///
/// If the count is reached, subsequent auto-mob position falling within the x range will be ignored
pub const AUTO_MOB_IGNORE_XS_SOLIDIFY_COUNT: u32 = 3;

/// The range an ignored auto-mob x position spans
///
/// If an auto-mob x position is 5, then the range is [2, 8]
pub const AUTO_MOB_IGNORE_XS_RANGE: u32 = 3;

/// The maximum of number points for auto mobbing to periodically move to
pub const AUTO_MOB_MAX_PATHING_POINTS: usize = 3;

/// The acceptable y range above and below the detected mob position when matched with a reachable y
pub const AUTO_MOB_REACHABLE_Y_THRESHOLD: i32 = 10;

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
/// TODO: Should have a separate struct or trait for Rotator to access PlayerState instead direct
/// TODO: access
/// TODO: counter should not be u32?
#[derive(Debug, Default)]
pub struct PlayerState {
    pub config: PlayerConfiguration,
    /// The id of the normal action provided by [`Rotator`]
    pub(super) normal_action_id: u32,
    /// A normal action requested by [`Rotator`]
    pub(super) normal_action: Option<PlayerAction>,
    /// The id of the priority action provided by [`Rotator`]
    pub(super) priority_action_id: u32,
    /// A priority action requested by [`Rotator`]
    ///
    /// This action will override the normal action if it is in the middle of executing.
    pub(super) priority_action: Option<PlayerAction>,
    /// The player current health and max health
    pub health: Option<(u32, u32)>,
    /// The task to update health
    pub(super) health_task: Option<Task<Result<(u32, u32)>>>,
    /// The rectangular health bar region
    pub(super) health_bar: Option<Rect>,
    /// The task for the health bar
    pub(super) health_bar_task: Option<Task<Result<Rect>>>,
    /// Track if the player moved within a specified ticks to determine if the player is stationary
    pub(super) is_stationary_timeout: Timeout,
    /// Whether the player is stationary
    pub(super) is_stationary: bool,
    /// Whether the player is dead
    pub is_dead: bool,
    /// The task for detecting if player is dead
    pub(super) is_dead_task: Option<Task<Result<bool>>>,
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
    pub(super) last_movement_normal_map: HashMap<LastMovement, u32>,

    /// Tracks [`Self::last_movement`] to abort priority action when its position is not accurate
    ///
    /// Clears when a priority action is completed or aborted
    pub(super) last_movement_priority_map: HashMap<LastMovement, u32>,
    /// Tracks a map of "reachable" y
    ///
    /// A y is reachable if there is a platform the player can stand on
    pub(super) auto_mob_reachable_y_map: HashMap<i32, u32>,
    /// The matched reachable y and also the key in [`Self::auto_mob_reachable_y_map`]
    pub(super) auto_mob_reachable_y: Option<i32>,
    /// Tracks a map of reachable y to x ranges that can be ignored
    ///
    /// This will help auto-mobbing ignores positions that are known to be not reachable
    pub(super) auto_mob_ignore_xs_map: HashMap<i32, (Range<i32>, u32)>,
    /// Stores points to periodically move to when auto mobbing
    ///
    /// Helps changing location for detecting more mobs. It is populated in terminal state of
    /// [`Player::UseKey`].
    pub(super) auto_mob_pathing_points: Vec<Point>,
    /// Tracks whether movement-related actions do not change the player position after a while.
    ///
    /// Resets when a limit is reached (for unstucking) or position did change.
    pub(super) unstuck_counter: u32,
    /// The number of times player transtioned to [`Player::Unstucking`]
    ///
    /// Resets when threshold reached or position changed
    pub(super) unstuck_transitioned_counter: u32,
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

    /// Whether there is a priority rune action
    #[inline]
    pub fn has_rune_action(&self) -> bool {
        matches!(self.priority_action, Some(PlayerAction::SolveRune))
    }

    /// Whether the player is validating whether the rune is solved
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
    pub(super) fn mark_action_completed(&mut self) {
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
    pub(super) fn has_auto_mob_action_only(&self) -> bool {
        matches!(self.normal_action, Some(PlayerAction::AutoMob(_))) && !self.has_priority_action()
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

    /// Whether the auto mob reachable y requires "solidifying"
    #[inline]
    pub(super) fn auto_mob_reachable_y_require_update(&self) -> bool {
        self.auto_mob_reachable_y.is_none_or(|y| {
            *self.auto_mob_reachable_y_map.get(&y).unwrap() < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT
        })
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

    /// Increments the rune validation fail count and sets [`PlayerState::rune_cash_shop`] if needed
    #[inline]
    pub(super) fn update_rune_fail_count_state(&mut self) {
        self.rune_failed_count += 1;
        if self.rune_failed_count >= MAX_RUNE_FAILED_COUNT {
            self.rune_failed_count = 0;
            self.rune_cash_shop = true;
        }
    }

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
            self.unstuck_counter = 0;
            self.unstuck_transitioned_counter = 0;
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
                        self.update_rune_fail_count_state();
                    } else {
                        self.rune_failed_count = 0;
                    }
                    None
                },
                Some,
            )
        });
    }

    // TODO: This should be a PlayerAction?
    #[inline]
    fn update_health_state(&mut self, context: &Context) {
        if let Player::SolvingRune(_) = context.player {
            return;
        }
        if self.config.use_potion_below_percent.is_none() {
            self.reset_health();
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

    #[inline]
    fn reset_health(&mut self) {
        self.health = None;
        self.health_task = None;
        self.health_bar = None;
        self.health_bar_task = None;
    }

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
