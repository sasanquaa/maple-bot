use std::collections::HashMap;

use anyhow::Result;
use opencv::core::{Point, Rect};
use platforms::windows::KeyKind;
use rand::seq::IteratorRandom;

use super::{
    AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT, DOUBLE_JUMP_AUTO_MOB_THRESHOLD, DOUBLE_JUMP_THRESHOLD,
    FALLING_THRESHOLD, JUMP_THRESHOLD, LastMovement, Player, PlayerAction, timeout::Timeout,
};
use crate::{ActionKeyDirection, Class, task::Task};

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
    /// Stores points to periodically move to when auto mobbing
    ///
    /// Helps changing location for detecting more mobs. It is populated in terminal state of
    /// [`Player::UseKey`].
    pub(super) auto_mob_pathing_points: Vec<Point>,
    /// Tracks whether movement-related actions do not change the player position after a while.
    ///
    /// Resets when a limit is reached (for unstucking) or position did change.
    pub(super) unstuck_counter: u32,
    /// The number of consecutive times player transtioned to [`Player::Unstucking`]
    ///
    /// Resets when position did change
    pub(super) unstuck_consecutive_counter: u32,
    /// Unstuck task for detecting settings when mis-pressing ESC key
    pub(super) unstuck_task: Option<Task<Result<bool>>>,
    /// Rune solving task
    pub(super) rune_task: Option<Task<Result<[KeyKind; 4]>>>,
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

    #[inline]
    pub fn has_priority_action(&self) -> bool {
        self.priority_action.is_some()
    }

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
    pub(super) fn clear_action_and_movement(&mut self) {
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
}
