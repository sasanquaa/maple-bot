use std::{
    assert_matches::debug_assert_matches,
    collections::VecDeque,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
    u32,
};

use anyhow::Result;
use log::debug;
use opencv::core::{Point, Rect};
use ordered_hash_map::OrderedHashMap;
use rand::seq::IndexedRandom;

use crate::{
    ActionKeyDirection, ActionKeyWith, AutoMobbing, KeyBinding, Position, RotationMode,
    buff::Buff,
    context::{Context, ERDA_SHOWER_SKILL_POSITION, MS_PER_TICK, RUNE_BUFF_POSITION},
    database::{Action, ActionCondition, ActionKey, ActionMove},
    detect::Detector,
    minimap::Minimap,
    player::{Player, PlayerState},
    player_actions::{PlayerAction, PlayerActionAutoMob, PlayerActionKey},
    skill::Skill,
    task::{Task, Update, update_task_repeatable},
};

const COOLDOWN_BETWEEN_QUEUE_MILLIS: u128 = 20_000;
const COOLDOWN_BETWEEN_POTION_QUEUE_MILLIS: u128 = 2_000;

/// Predicate for when a priority action can be queued
struct Condition(Box<dyn Fn(&Context, &mut PlayerState, Option<Instant>) -> bool>);

impl std::fmt::Debug for Condition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "dyn Fn(...)")
    }
}

/// A priority action that can override a normal action
///
/// This includes all non-`ActionCondition::Any` actions
///
/// When a player is in the middle of doing a normal action, this type of action
/// can override most of the player's current state and forced to perform this action.
/// However, it cannot override player states that are considered "terminal". These states
/// include stalling, using key and forced double jumping. It also cannot override linked action.
///
/// When this type of action has `queue_to_front` set, it will be queued to the front and override
/// other non-queue-to-front priority action. The overriden action is simply placed back to the queue in
/// front. It is mostly useful for action such as `press attack after x seconds even in the middle
/// of moving`.
#[derive(Debug)]
struct PriorityAction {
    condition: Condition,
    condition_kind: Option<ActionCondition>,
    inner: RotatorAction,
    queue_to_front: bool,
    ignoring: bool,
    last_queued_time: Option<Instant>,
}

/// The action that will be passed to the player
///
/// There are `Single` and `Linked` actions. `Single` action is self-explanatory and `Linked`
/// action is a linked list of actions. `Linked` action is executed in order, until completion and
/// cannot be replaced by any other type of actions.
#[derive(Clone, Debug)]
enum RotatorAction {
    Single(PlayerAction),
    Linked(LinkedAction),
}

/// A linked list of actions
#[derive(Clone, Debug)]
struct LinkedAction {
    inner: PlayerAction,
    next: Option<Box<LinkedAction>>,
}

/// The rotator's rotation mode
#[derive(Default, Debug)]
pub enum RotatorMode {
    StartToEnd,
    #[default]
    StartToEndThenReverse,
    AutoMobbing {
        bound: Rect,
        key: KeyBinding,
        key_count: u32,
        key_wait_before_millis: u64,
        key_wait_after_millis: u64,
    },
}

impl From<RotationMode> for RotatorMode {
    fn from(mode: RotationMode) -> Self {
        match mode {
            RotationMode::StartToEnd => RotatorMode::StartToEnd,
            RotationMode::StartToEndThenReverse => RotatorMode::StartToEndThenReverse,
            RotationMode::AutoMobbing(AutoMobbing {
                bound,
                key,
                key_count,
                key_wait_before_millis,
                key_wait_after_millis,
            }) => RotatorMode::AutoMobbing {
                bound: bound.into(),
                key,
                key_count,
                key_wait_before_millis,
                key_wait_after_millis,
            },
        }
    }
}

#[derive(Default, Debug)]
pub struct Rotator {
    // this is literally free postfix increment!
    id_counter: AtomicU32,
    normal_actions: Vec<(u32, RotatorAction)>,
    normal_queuing_linked_action: Option<(u32, Box<LinkedAction>)>,
    normal_index: usize,
    normal_actions_backward: bool,
    normal_rotate_mode: RotatorMode,
    auto_mob_task: Option<Task<Result<Vec<Point>>>>,
    priority_actions: OrderedHashMap<u32, PriorityAction>,
    priority_queuing_linked_action: Option<(u32, Box<LinkedAction>)>,
    priority_actions_queue: VecDeque<u32>,
}

impl Rotator {
    pub fn build_actions(
        &mut self,
        actions: &[Action],
        buffs: &[(usize, KeyBinding)],
        potion_key: KeyBinding,
        mode: RotatorMode,
    ) {
        debug!(target: "rotator", "preparing actions {actions:?} {buffs:?}");
        self.reset_queue();
        self.normal_actions.clear();
        self.normal_rotate_mode = mode;
        self.priority_actions.clear();

        let mut i = 0;
        while i < actions.len() {
            let action = actions[i];
            let condition = match action {
                Action::Move(ActionMove { condition, .. })
                | Action::Key(ActionKey { condition, .. }) => condition,
            };
            let queue_to_front = match action {
                Action::Move(_) => false,
                Action::Key(ActionKey { queue_to_front, .. }) => queue_to_front.unwrap_or_default(),
            };
            debug_assert!(i != 0 || !matches!(condition, ActionCondition::Linked));
            let (action, offset) = rotator_action(action, i, actions);
            match condition {
                ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown => {
                    self.priority_actions.insert(
                        self.id_counter.fetch_add(1, Ordering::Relaxed),
                        priority_action(action, condition, queue_to_front),
                    );
                }
                ActionCondition::Any => {
                    if matches!(self.normal_rotate_mode, RotatorMode::AutoMobbing { .. }) {
                        continue;
                    }
                    self.normal_actions
                        .push((self.id_counter.fetch_add(1, Ordering::Relaxed), action))
                }
                ActionCondition::Linked => unreachable!(),
            }
            i += offset;
        }

        self.priority_actions.insert(
            self.id_counter.fetch_add(1, Ordering::Relaxed),
            elite_boss_potion_spam_priority_action(potion_key),
        );
        self.priority_actions.insert(
            self.id_counter.fetch_add(1, Ordering::Relaxed),
            solve_rune_priority_action(),
        );
        for (i, key) in buffs.iter().copied() {
            self.priority_actions.insert(
                self.id_counter.fetch_add(1, Ordering::Relaxed),
                buff_priority_action(i, key),
            );
        }
    }

    #[inline]
    pub fn reset_queue(&mut self) {
        self.normal_actions_backward = false;
        self.normal_index = 0;
        self.normal_queuing_linked_action = None;
        self.priority_actions_queue.clear();
        self.priority_queuing_linked_action = None;
    }

    #[inline]
    pub fn rotate_action(
        &mut self,
        context: &Context,
        detector: &impl Detector,
        player: &mut PlayerState,
    ) {
        if context.halting || matches!(context.player, Player::CashShopThenExit(_, _)) {
            return;
        }
        self.rotate_priority_actions(context, player);
        self.rotate_priority_actions_queue(context, player);
        if !player.has_priority_action() && !player.has_normal_action() {
            match self.normal_rotate_mode {
                RotatorMode::StartToEnd => self.rotate_start_to_end(player),
                RotatorMode::StartToEndThenReverse => self.rotate_start_to_end_then_reverse(player),
                RotatorMode::AutoMobbing {
                    bound,
                    key,
                    key_count,
                    key_wait_before_millis,
                    key_wait_after_millis,
                } => self.rotate_auto_mobbing(
                    context,
                    detector,
                    player,
                    bound,
                    key,
                    key_count,
                    key_wait_before_millis,
                    key_wait_after_millis,
                ),
            }
        }
    }

    /// Check if the provided `id` is a linked action in queue or executing
    #[inline]
    fn is_priority_linked_action_queuing_or_executing(
        &self,
        player: &PlayerState,
        id: u32,
    ) -> bool {
        if self
            .priority_queuing_linked_action
            .as_ref()
            .is_some_and(|(action_id, _)| *action_id == id)
        {
            return true;
        }
        player.priority_action_id().is_some_and(|action_id| {
            action_id == id
                && self
                    .priority_actions
                    .get(&id)
                    .is_some_and(|action| matches!(action.inner, RotatorAction::Linked(_)))
        })
    }

    /// Check if the player or the queue has an Erda action
    #[inline]
    fn has_erda_action_queuing_or_executing(&self, player: &PlayerState) -> bool {
        if player.priority_action_id().is_some_and(|id| {
            self.priority_actions.get(&id).is_some_and(|action| {
                matches!(
                    action.condition_kind,
                    Some(ActionCondition::ErdaShowerOffCooldown)
                )
            })
        }) {
            return true;
        }
        self.priority_actions_queue.iter().any(|id| {
            matches!(
                self.priority_actions.get(id).unwrap().condition_kind,
                Some(ActionCondition::ErdaShowerOffCooldown)
            )
        })
    }

    /// Rotate the actions inside the `priority_actions`
    ///
    /// This function does not pass the action to the player but only pushes the action to
    /// `priority_actions_queue`. It is responsible for checking queuing condition.
    fn rotate_priority_actions(&mut self, context: &Context, player: &mut PlayerState) {
        // keep ignoring while there is any type of erda condition action inside the queue
        let has_erda_action = self.has_erda_action_queuing_or_executing(player);
        let ids = self.priority_actions.keys().copied().collect::<Vec<_>>(); // why?
        for id in ids {
            // ignore for as long as the action is a linked action that is queuing
            // or executing
            let has_linked_action = self.is_priority_linked_action_queuing_or_executing(player, id);
            let action = self.priority_actions.get_mut(&id).unwrap();
            action.ignoring = match action.condition_kind {
                Some(ActionCondition::ErdaShowerOffCooldown) => {
                    has_erda_action || has_linked_action
                }
                Some(ActionCondition::Linked) | Some(ActionCondition::EveryMillis(_)) | None => {
                    player // the player currently executing action
                        .priority_action_id()
                        .is_some_and(|action_id| action_id == id)
                        || self // the action is in queue
                            .priority_actions_queue
                            .iter()
                            .any(|action_id| *action_id == id)
                        || has_linked_action
                }
                Some(ActionCondition::Any) => unreachable!(),
            };
            if action.ignoring {
                action.last_queued_time = Some(Instant::now());
                continue;
            }
            if (action.condition.0)(context, player, action.last_queued_time) {
                if action.queue_to_front {
                    self.priority_actions_queue.push_front(id);
                } else {
                    self.priority_actions_queue.push_back(id);
                }
                action.last_queued_time = Some(Instant::now());
            }
        }
    }

    /// Check if the player is queuing or executing normal a linked action
    ///
    /// This prevents `rotate_priority_actions_queue` to override the normal linked action
    #[inline]
    fn has_normal_linked_action_queuing_or_executing(&self, player: &PlayerState) -> bool {
        if self.normal_queuing_linked_action.is_some() {
            return true;
        }
        player.normal_action_id().is_some_and(|id| {
            self.normal_actions.iter().any(|(action_id, action)| {
                *action_id == id && matches!(action, RotatorAction::Linked(_))
            })
        })
    }

    /// Check if the player is executing priority a linked action
    ///
    /// This does not check the queuing linked action because this check is to allow the linked
    /// action to be rotated in `rotate_priority_actions_queue`
    #[inline]
    fn has_priority_linked_action_executing(&self, player: &PlayerState) -> bool {
        player.priority_action_id().is_some_and(|id| {
            self.priority_actions
                .get(&id)
                .is_some_and(|action| matches!(action.inner, RotatorAction::Linked(_)))
        })
    }

    /// Rotate the actions inside the `priority_actions_queue`
    ///
    /// If there is any on-going linked action:
    /// - For normal action, it will wait until the action is completed by the normal rotation
    /// - For priority action, it will rotate and wait until all the actions are executed
    ///
    /// After that, it will rotate actions inside `priority_actions_queue`
    fn rotate_priority_actions_queue(&mut self, context: &Context, player: &mut PlayerState) {
        if self.priority_actions_queue.is_empty() && self.priority_queuing_linked_action.is_none() {
            return;
        }
        if !can_override_player_state(context)
            || self.has_normal_linked_action_queuing_or_executing(player)
            || self.has_priority_linked_action_executing(player)
        {
            return;
        }
        if self.rotate_queuing_linked_action(player, true) {
            return;
        }
        let id = *self.priority_actions_queue.front().unwrap();
        let Some(action) = self.priority_actions.get(&id) else {
            self.priority_actions_queue.pop_front();
            return;
        };
        let has_queue_to_front = player
            .priority_action_id()
            .and_then(|id| {
                self.priority_actions
                    .get(&id)
                    .map(|action| action.queue_to_front)
            })
            .unwrap_or_default();
        if has_queue_to_front {
            return;
        }
        if player.has_priority_action() && !action.queue_to_front {
            return;
        }
        self.priority_actions_queue.pop_front();
        match action.inner.clone() {
            RotatorAction::Single(inner) => {
                if action.queue_to_front {
                    if let Some(id) = player.replace_priority_action(id, inner) {
                        self.priority_actions_queue.push_front(id);
                    }
                } else {
                    player.set_priority_action(id, inner);
                }
            }
            RotatorAction::Linked(linked) => {
                if action.queue_to_front {
                    if let Some(id) = player.take_priority_action() {
                        self.priority_actions_queue.push_front(id);
                    }
                }
                self.priority_queuing_linked_action = Some((id, Box::new(linked)));
                self.rotate_queuing_linked_action(player, true);
            }
        }
    }

    fn rotate_auto_mobbing(
        &mut self,
        context: &Context,
        detector: &impl Detector,
        player: &mut PlayerState,
        bound: Rect,
        key: KeyBinding,
        key_count: u32,
        key_wait_before_millis: u64,
        key_wait_after_millis: u64,
    ) {
        debug_assert!(!player.has_normal_action() && !player.has_priority_action());
        let Minimap::Idle(idle) = context.minimap else {
            return;
        };
        let Some(pos) = player.last_known_pos else {
            return;
        };
        let detector = detector.clone();
        let Update::Complete(Ok(points)) =
            update_task_repeatable(0, &mut self.auto_mob_task, move || {
                detector.detect_mobs(idle.bbox, bound, pos)
            })
        else {
            return;
        };
        let Some(point) = points.choose(&mut rand::rng()) else {
            return;
        };
        player.set_normal_action(
            u32::MAX,
            PlayerAction::AutoMob(PlayerActionAutoMob {
                key,
                count: if key_count == 0 { 1 } else { key_count },
                wait_before_ticks: (key_wait_before_millis / MS_PER_TICK) as u32,
                wait_after_ticks: (key_wait_after_millis / MS_PER_TICK) as u32,
                position: Position {
                    x: point.x,
                    y: idle.bbox.height - point.y,
                    allow_adjusting: false,
                },
            }),
        );
    }

    fn rotate_start_to_end(&mut self, player: &mut PlayerState) {
        debug_assert!(!player.has_normal_action() && !player.has_priority_action());
        if self.normal_actions.is_empty() {
            return;
        }
        if self.rotate_queuing_linked_action(player, false) {
            return;
        }
        debug_assert!(self.normal_index < self.normal_actions.len());
        let (id, action) = self.normal_actions[self.normal_index].clone();
        self.normal_index = (self.normal_index + 1) % self.normal_actions.len();
        match action {
            RotatorAction::Single(action) => {
                player.set_normal_action(id, action);
            }
            RotatorAction::Linked(action) => {
                self.normal_queuing_linked_action = Some((id, Box::new(action)));
                self.rotate_queuing_linked_action(player, false);
            }
        }
    }

    fn rotate_start_to_end_then_reverse(&mut self, player: &mut PlayerState) {
        debug_assert!(!player.has_normal_action() && !player.has_priority_action());
        if self.normal_actions.is_empty() {
            return;
        }
        if self.rotate_queuing_linked_action(player, false) {
            return;
        }
        debug_assert!(self.normal_index < self.normal_actions.len());
        let len = self.normal_actions.len();
        let i = if self.normal_actions_backward {
            (len - self.normal_index).saturating_sub(1)
        } else {
            self.normal_index
        };
        if (self.normal_index + 1) == len {
            self.normal_actions_backward = !self.normal_actions_backward
        }
        let (id, action) = self.normal_actions[i].clone();
        self.normal_index = (self.normal_index + 1) % len;
        match action {
            RotatorAction::Single(action) => {
                player.set_normal_action(id, action);
            }
            RotatorAction::Linked(action) => {
                self.normal_queuing_linked_action = Some((id, Box::new(action)));
                self.rotate_queuing_linked_action(player, false);
            }
        }
    }

    #[inline]
    fn rotate_queuing_linked_action(
        &mut self,
        player: &mut PlayerState,
        is_priority: bool,
    ) -> bool {
        let linked_action = if is_priority {
            &mut self.priority_queuing_linked_action
        } else {
            &mut self.normal_queuing_linked_action
        };
        if linked_action.is_none() {
            return false;
        }
        let (id, action) = linked_action.take().unwrap();
        *linked_action = action.next.map(|action| (id, action));
        if is_priority {
            player.set_priority_action(id, action.inner);
        } else {
            player.set_normal_action(id, action.inner);
        }
        true
    }
}

#[inline]
fn rotator_action(
    start_action: Action,
    start_index: usize,
    actions: &[Action],
) -> (RotatorAction, usize) {
    if start_index + 1 < actions.len() {
        match actions[start_index + 1] {
            Action::Move(ActionMove {
                condition: ActionCondition::Linked,
                ..
            })
            | Action::Key(ActionKey {
                condition: ActionCondition::Linked,
                ..
            }) => (),
            _ => return (RotatorAction::Single(start_action.into()), 1),
        }
    }
    let mut head = LinkedAction {
        inner: start_action.into(),
        next: None,
    };
    let mut current = &mut head;
    let mut offset = 1;
    for i in start_index + 1..actions.len() {
        match actions[i] {
            Action::Move(ActionMove {
                condition: ActionCondition::Linked,
                ..
            })
            | Action::Key(ActionKey {
                condition: ActionCondition::Linked,
                ..
            }) => {
                let action = LinkedAction {
                    inner: actions[i].into(),
                    next: None,
                };
                current.next = Some(Box::new(action));
                current = current.next.as_mut().unwrap();
                offset += 1;
            }
            _ => break,
        }
    }
    (RotatorAction::Linked(head), offset)
}

#[inline]
fn priority_action(
    action: RotatorAction,
    condition: ActionCondition,
    queue_to_front: bool,
) -> PriorityAction {
    debug_assert_matches!(
        condition,
        ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown
    );
    PriorityAction {
        inner: action,
        condition: Condition(Box::new(move |context, _, last_queued_time| {
            should_queue_fixed_action(context, last_queued_time, condition)
        })),
        condition_kind: Some(condition),
        queue_to_front,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn elite_boss_potion_spam_priority_action(key: KeyBinding) -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(|context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_POTION_QUEUE_MILLIS)
            {
                return false;
            }
            if let Minimap::Idle(idle) = context.minimap {
                return idle.has_elite_boss;
            }
            false
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Key(PlayerActionKey {
            key,
            link_key: None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 0,
        })),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn solve_rune_priority_action() -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(|context, player, last_queued_time| {
            if player.is_validating_rune() {
                return false;
            }
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_QUEUE_MILLIS) {
                return false;
            }
            if let Minimap::Idle(idle) = context.minimap {
                return idle.rune.is_some()
                    && matches!(context.buffs[RUNE_BUFF_POSITION], Buff::NoBuff);
            }
            false
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::SolveRune),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn buff_priority_action(buff_index: usize, key: KeyBinding) -> PriorityAction {
    PriorityAction {
        condition: Condition(Box::new(move |context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_QUEUE_MILLIS) {
                return false;
            }
            if !matches!(context.minimap, Minimap::Idle(_)) {
                return false;
            }
            matches!(context.buffs[buff_index], Buff::NoBuff)
        })),
        condition_kind: None,
        inner: RotatorAction::Single(PlayerAction::Key(PlayerActionKey {
            key,
            link_key: None,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 10,
            wait_after_use_ticks: 10,
        })),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn can_override_player_state(context: &Context) -> bool {
    !matches!(
        context.player,
        Player::UseKey(_) | Player::Stalling(_, _) | Player::DoubleJumping(_, false, _)
    )
}

#[inline]
fn at_least_millis_passed_since(last_queued_time: Option<Instant>, millis: u128) -> bool {
    last_queued_time
        .map(|instant| Instant::now().duration_since(instant).as_millis() >= millis)
        .unwrap_or(true)
}

#[inline]
fn should_queue_fixed_action(
    context: &Context,
    last_queued_time: Option<Instant>,
    condition: ActionCondition,
) -> bool {
    let millis_should_passed = match condition {
        ActionCondition::EveryMillis(millis) => millis as u128,
        ActionCondition::ErdaShowerOffCooldown => COOLDOWN_BETWEEN_QUEUE_MILLIS,
        ActionCondition::Linked | ActionCondition::Any => unreachable!(),
    };
    if !at_least_millis_passed_since(last_queued_time, millis_should_passed) {
        return false;
    }
    if matches!(condition, ActionCondition::ErdaShowerOffCooldown)
        && !matches!(
            context.skills[ERDA_SHOWER_SKILL_POSITION],
            Skill::Idle(_, _)
        )
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use opencv::core::{Point, Vec4b};

    use super::*;
    use crate::{Position, detect::MockDetector, minimap::MinimapIdle};

    const NORMAL_ACTION: Action = Action::Move(ActionMove {
        position: Position {
            x: 0,
            y: 0,
            allow_adjusting: false,
        },
        condition: ActionCondition::Any,
        wait_after_move_millis: 0,
    });
    const PRIORITY_ACTION: Action = Action::Move(ActionMove {
        position: Position {
            x: 0,
            y: 0,
            allow_adjusting: false,
        },
        condition: ActionCondition::ErdaShowerOffCooldown,
        wait_after_move_millis: 0,
    });

    #[test]
    fn rotator_at_least_millis_passed_since() {
        let now = Instant::now();
        assert!(at_least_millis_passed_since(None, 1000));
        assert!(at_least_millis_passed_since(
            Some(now - Duration::from_millis(2000)),
            1000
        ));
        assert!(!at_least_millis_passed_since(
            Some(now - Duration::from_millis(500)),
            1000
        ));
    }

    #[test]
    fn rotator_should_queue_fixed_action_every_millis() {
        let context = Context::default();
        let now = Instant::now();

        assert!(should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(3000)),
            ActionCondition::EveryMillis(2000)
        ));
        assert!(!should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(1000)),
            ActionCondition::EveryMillis(2000)
        ));
    }

    #[test]
    fn rotator_should_queue_fixed_action_erda_shower() {
        let mut context = Context::default();
        let now = Instant::now();

        context.skills[ERDA_SHOWER_SKILL_POSITION] =
            Skill::Idle(Point::default(), Vec4b::default());
        assert!(!should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64 - 1000)),
            ActionCondition::ErdaShowerOffCooldown
        ));
        assert!(should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64)),
            ActionCondition::ErdaShowerOffCooldown
        ));

        context.skills[ERDA_SHOWER_SKILL_POSITION] = Skill::Detecting;
        assert!(!should_queue_fixed_action(
            &context,
            Some(now - Duration::from_millis(COOLDOWN_BETWEEN_QUEUE_MILLIS as u64)),
            ActionCondition::ErdaShowerOffCooldown
        ));
    }

    #[test]
    fn rotator_build_actions() {
        let mut rotator = Rotator::default();
        let actions = vec![NORMAL_ACTION, NORMAL_ACTION, PRIORITY_ACTION];
        let buffs = vec![(0, KeyBinding::default()); 4];

        rotator.build_actions(&actions, &buffs, KeyBinding::A, RotatorMode::default());
        assert_eq!(rotator.priority_actions.len(), 7);
        assert_eq!(rotator.normal_actions.len(), 2);
    }

    #[test]
    fn rotator_rotate_action_start_to_end_then_reverse() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::default();
        let detector = MockDetector::new();
        rotator.normal_rotate_mode = RotatorMode::StartToEndThenReverse;
        for i in 0..2 {
            rotator
                .normal_actions
                .push((i, RotatorAction::Single(NORMAL_ACTION.into())));
        }

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);

        player.abort_actions();

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 0);
    }

    #[test]
    fn rotator_rotate_action_start_to_end() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::default();
        let detector = MockDetector::new();
        rotator.normal_rotate_mode = RotatorMode::StartToEnd;
        for i in 0..2 {
            rotator
                .normal_actions
                .push((i, RotatorAction::Single(NORMAL_ACTION.into())));
        }

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 1);

        player.abort_actions();

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_actions_backward);
        assert_eq!(rotator.normal_index, 0);
    }

    #[test]
    fn rotator_priority_action_queue() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let mut minimap = MinimapIdle::default();
        minimap.rune = Some(Point::default());
        let detector = MockDetector::new();
        let mut context = Context {
            minimap: Minimap::Idle(minimap),
            ..Context::default()
        };
        context.buffs[RUNE_BUFF_POSITION] = Buff::NoBuff;
        rotator.priority_actions.insert(55, PriorityAction {
            condition: Condition(Box::new(|context, _, _| {
                matches!(context.minimap, Minimap::Idle(_))
            })),
            condition_kind: None,
            inner: RotatorAction::Single(PlayerAction::SolveRune),
            queue_to_front: true,
            ignoring: false,
            last_queued_time: None,
        });

        rotator.rotate_action(&context, &detector, &mut player);
        assert_eq!(rotator.priority_actions_queue.len(), 0);
        assert_eq!(player.priority_action_id(), Some(55));
    }

    #[test]
    fn rotator_priority_action_queue_to_front() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let detector = MockDetector::new();
        let context = Context::default();
        // queue 2 non-front priority actions
        rotator.priority_actions.insert(2, PriorityAction {
            condition: Condition(Box::new(|_, _, _| true)),
            condition_kind: None,
            inner: RotatorAction::Single(NORMAL_ACTION.into()),
            queue_to_front: false,
            ignoring: false,
            last_queued_time: None,
        });
        rotator.priority_actions.insert(3, PriorityAction {
            condition: Condition(Box::new(|_, _, _| true)),
            condition_kind: None,
            inner: RotatorAction::Single(NORMAL_ACTION.into()),
            queue_to_front: false,
            ignoring: false,
            last_queued_time: None,
        });

        rotator.rotate_action(&context, &detector, &mut player);
        assert_eq!(rotator.priority_actions_queue.len(), 1);
        assert_eq!(player.priority_action_id(), Some(2));

        // add 1 front priority action
        rotator.priority_actions.insert(4, PriorityAction {
            condition: Condition(Box::new(|_, _, _| true)),
            condition_kind: None,
            inner: RotatorAction::Single(NORMAL_ACTION.into()),
            queue_to_front: true,
            ignoring: false,
            last_queued_time: None,
        });

        // non-front priority action get replaced
        rotator.rotate_action(&context, &detector, &mut player);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([2, 3].into_iter())
        );
        assert_eq!(player.priority_action_id(), Some(4));

        // add another front priority action
        rotator.priority_actions.insert(5, PriorityAction {
            condition: Condition(Box::new(|_, _, _| true)),
            condition_kind: None,
            inner: RotatorAction::Single(NORMAL_ACTION.into()),
            queue_to_front: true,
            ignoring: false,
            last_queued_time: None,
        });

        // queued front priority action cannot be replaced
        // by another front priority action
        rotator.rotate_action(&context, &detector, &mut player);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([5, 2, 3].into_iter())
        );
        assert_eq!(player.priority_action_id(), Some(4));
    }

    #[test]
    fn rotator_priority_linked_action() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let detector = MockDetector::new();
        let context = Context::default();
        rotator.priority_actions.insert(2, PriorityAction {
            condition: Condition(Box::new(|_, _, _| true)),
            condition_kind: None,
            inner: RotatorAction::Linked(LinkedAction {
                inner: NORMAL_ACTION.into(),
                next: Some(Box::new(LinkedAction {
                    inner: NORMAL_ACTION.into(),
                    next: None,
                })),
            }),
            queue_to_front: false,
            ignoring: false,
            last_queued_time: None,
        });

        // linked action queued
        rotator.rotate_action(&context, &detector, &mut player);
        assert!(rotator.priority_actions_queue.is_empty());
        assert!(rotator.priority_queuing_linked_action.is_some());
        assert_eq!(player.priority_action_id(), Some(2));

        // linked action cannot be replaced by queue to front
        rotator.priority_actions.insert(4, PriorityAction {
            condition: Condition(Box::new(|_, _, _| true)),
            condition_kind: None,
            inner: RotatorAction::Single(PlayerAction::SolveRune),
            queue_to_front: true,
            ignoring: false,
            last_queued_time: None,
        });
        rotator.rotate_action(&context, &detector, &mut player);
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([4].into_iter())
        );

        player.abort_actions();
        rotator.rotate_action(&context, &detector, &mut player);
        assert!(rotator.priority_queuing_linked_action.is_none());
        assert_eq!(
            rotator.priority_actions_queue,
            VecDeque::from_iter([4].into_iter())
        );
        assert_eq!(player.priority_action_id(), Some(2));
    }
}
