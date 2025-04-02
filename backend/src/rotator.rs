use std::{
    assert_matches::debug_assert_matches,
    collections::VecDeque,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
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

type Condition = Box<dyn Fn(&Context, &mut PlayerState, Option<Instant>) -> bool>;

struct PriorityAction {
    condition: Condition,
    condition_kind: Option<ActionCondition>,
    action: PlayerAction,
    queue_to_front: bool,
    ignoring: bool,
    last_queued_time: Option<Instant>,
}

// TODO: merge RotationMode in Configuration with Minimap
#[derive(Default)]
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

#[derive(Default)]
pub struct Rotator {
    normal_actions: Vec<PlayerAction>,
    normal_index: usize,
    normal_action_backward: bool,
    normal_rotate_mode: RotatorMode,
    auto_mob_task: Option<Task<Result<Vec<Point>>>>,
    // this is literally free postfix increment!
    priority_id_counter: AtomicU32,
    priority_actions: OrderedHashMap<u32, PriorityAction>,
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

        for action in actions.iter().copied() {
            match action {
                Action::Move(ActionMove { condition, .. })
                | Action::Key(ActionKey { condition, .. }) => match condition {
                    ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown => {
                        self.priority_actions.insert(
                            self.priority_id_counter.fetch_add(1, Ordering::Relaxed),
                            priority_action(action, condition),
                        );
                    }
                    ActionCondition::Any => {
                        if matches!(self.normal_rotate_mode, RotatorMode::AutoMobbing { .. }) {
                            continue;
                        }
                        self.normal_actions.push(action.into())
                    }
                },
            }
        }
        self.priority_actions.insert(
            self.priority_id_counter.fetch_add(1, Ordering::Relaxed),
            elite_boss_potion_spam_priority_action(potion_key),
        );
        self.priority_actions.insert(
            self.priority_id_counter.fetch_add(1, Ordering::Relaxed),
            solve_rune_priority_action(),
        );
        for (i, key) in buffs.iter().copied() {
            self.priority_actions.insert(
                self.priority_id_counter.fetch_add(1, Ordering::Relaxed),
                buff_priority_action(i, key),
            );
        }
    }

    #[inline]
    pub fn reset_queue(&mut self) {
        self.normal_action_backward = false;
        self.normal_index = 0;
        self.priority_actions_queue.clear();
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
                RotatorMode::StartToEnd => self.rotate_start_end(player),
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

    fn has_erda_action_in_queue(&self, player: &PlayerState) -> bool {
        player
            .priority_action_id()
            .map(|id| {
                self.priority_actions.get(&id).is_some_and(|action| {
                    matches!(
                        action.condition_kind,
                        Some(ActionCondition::ErdaShowerOffCooldown)
                    )
                })
            })
            .unwrap_or_else(|| {
                self.priority_actions_queue.iter().any(|id| {
                    matches!(
                        self.priority_actions.get(id).unwrap().condition_kind,
                        Some(ActionCondition::ErdaShowerOffCooldown)
                    )
                })
            })
    }

    fn rotate_priority_actions(&mut self, context: &Context, player: &mut PlayerState) {
        let has_erda_action = self.has_erda_action_in_queue(player);
        for (id, action) in self.priority_actions.iter_mut() {
            action.ignoring = match action.condition_kind {
                Some(ActionCondition::ErdaShowerOffCooldown) => has_erda_action,
                Some(ActionCondition::EveryMillis(_)) | None => {
                    player
                        .priority_action_id()
                        .is_some_and(|action_id| action_id == *id)
                        || self
                            .priority_actions_queue
                            .iter()
                            .any(|action_id| action_id == id)
                }
                Some(ActionCondition::Any) => unreachable!(),
            };
            if action.ignoring {
                action.last_queued_time = Some(Instant::now());
                continue;
            }
            if (action.condition)(context, player, action.last_queued_time) {
                if action.queue_to_front {
                    self.priority_actions_queue.push_front(*id);
                } else {
                    self.priority_actions_queue.push_back(*id);
                }
                action.last_queued_time = Some(Instant::now());
            }
        }
    }

    fn rotate_priority_actions_queue(&mut self, context: &Context, player: &mut PlayerState) {
        if self.priority_actions_queue.is_empty() {
            return;
        }
        if !can_add_priority_action(context) {
            return;
        }
        let id = self.priority_actions_queue.pop_front().unwrap();
        let Some(action) = self.priority_actions.get(&id) else {
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
        if action.queue_to_front && !has_queue_to_front {
            if let Some(id) = player.replace_priority_action(id, action.action) {
                self.priority_actions_queue.push_front(id);
            }
        } else if !player.has_priority_action() {
            player.set_priority_action(id, action.action);
        } else {
            self.priority_actions_queue.push_front(id);
        }
    }

    fn rotate_auto_mobbing(
        &mut self,
        context: &Context,
        detector: &impl Detector,
        player: &mut PlayerState,
        bound: opencv::core::Rect_<i32>,
        key: KeyBinding,
        key_count: u32,
        key_wait_before_millis: u64,
        key_wait_after_millis: u64,
    ) {
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
        player.set_normal_action(PlayerAction::AutoMob(PlayerActionAutoMob {
            key,
            count: if key_count == 0 { 1 } else { key_count },
            wait_before_ticks: (key_wait_before_millis / MS_PER_TICK) as u32,
            wait_after_ticks: (key_wait_after_millis / MS_PER_TICK) as u32,
            position: Position {
                x: point.x,
                y: idle.bbox.height - point.y,
                allow_adjusting: false,
            },
        }));
    }

    fn rotate_start_end(&mut self, player: &mut PlayerState) {
        if self.normal_actions.is_empty() {
            return;
        }
        debug_assert!(self.normal_index < self.normal_actions.len());
        let action = self.normal_actions[self.normal_index];
        self.normal_index = (self.normal_index + 1) % self.normal_actions.len();
        player.set_normal_action(action);
    }

    fn rotate_start_to_end_then_reverse(&mut self, player: &mut PlayerState) {
        if self.normal_actions.is_empty() {
            return;
        }
        debug_assert!(self.normal_index < self.normal_actions.len());
        let len = self.normal_actions.len();
        let i = if self.normal_action_backward {
            (len - self.normal_index).saturating_sub(1)
        } else {
            self.normal_index
        };
        if (self.normal_index + 1) == len {
            self.normal_action_backward = !self.normal_action_backward
        }
        self.normal_index = (self.normal_index + 1) % len;
        player.set_normal_action(self.normal_actions[i]);
    }
}

#[inline]
fn priority_action(action: Action, condition: ActionCondition) -> PriorityAction {
    debug_assert_matches!(
        condition,
        ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown
    );
    PriorityAction {
        action: action.into(),
        condition: Box::new(move |context, _, last_queued_time| {
            should_queue_fixed_action(context, last_queued_time, condition)
        }),
        condition_kind: Some(condition),
        queue_to_front: if let Action::Key(ActionKey { queue_to_front, .. }) = action {
            queue_to_front.unwrap_or_default()
        } else {
            false
        },
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn elite_boss_potion_spam_priority_action(key: KeyBinding) -> PriorityAction {
    PriorityAction {
        condition: Box::new(|context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_POTION_QUEUE_MILLIS)
            {
                return false;
            }
            if let Minimap::Idle(idle) = context.minimap {
                return idle.has_elite_boss;
            }
            false
        }),
        condition_kind: None,
        action: PlayerAction::Key(PlayerActionKey {
            key,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Any,
            wait_before_use_ticks: 5,
            wait_after_use_ticks: 0,
        }),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn solve_rune_priority_action() -> PriorityAction {
    PriorityAction {
        condition: Box::new(|context, player, last_queued_time| {
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
        }),
        condition_kind: None,
        action: PlayerAction::SolveRune,
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn buff_priority_action(buff_index: usize, key: KeyBinding) -> PriorityAction {
    PriorityAction {
        condition: Box::new(move |context, _, last_queued_time| {
            if !at_least_millis_passed_since(last_queued_time, COOLDOWN_BETWEEN_QUEUE_MILLIS) {
                return false;
            }
            if !matches!(context.minimap, Minimap::Idle(_)) {
                return false;
            }
            matches!(context.buffs[buff_index], Buff::NoBuff)
        }),
        condition_kind: None,
        action: PlayerAction::Key(PlayerActionKey {
            key,
            count: 1,
            position: None,
            direction: ActionKeyDirection::Any,
            with: ActionKeyWith::Stationary,
            wait_before_use_ticks: 10,
            wait_after_use_ticks: 10,
        }),
        queue_to_front: true,
        ignoring: false,
        last_queued_time: None,
    }
}

#[inline]
fn can_add_priority_action(context: &Context) -> bool {
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
        ActionCondition::Any => unreachable!(),
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
        for _ in 0..2 {
            rotator.normal_actions.push(NORMAL_ACTION.into());
        }

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_action_backward);
        assert_eq!(rotator.normal_index, 1);

        player.abort_actions();

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(rotator.normal_action_backward);
        assert_eq!(rotator.normal_index, 0);
    }

    #[test]
    fn rotator_rotate_action_start_to_end() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::default();
        let detector = MockDetector::new();
        rotator.normal_rotate_mode = RotatorMode::StartToEnd;
        for _ in 0..2 {
            rotator.normal_actions.push(NORMAL_ACTION.into());
        }

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_action_backward);
        assert_eq!(rotator.normal_index, 1);

        player.abort_actions();

        rotator.rotate_action(&context, &detector, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_action_backward);
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
            condition: Box::new(|context, _, _| matches!(context.minimap, Minimap::Idle(_))),
            condition_kind: None,
            action: PlayerAction::SolveRune,
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
            condition: Box::new(|_, _, _| true),
            condition_kind: None,
            action: NORMAL_ACTION.into(),
            queue_to_front: false,
            ignoring: false,
            last_queued_time: None,
        });
        rotator.priority_actions.insert(3, PriorityAction {
            condition: Box::new(|_, _, _| true),
            condition_kind: None,
            action: NORMAL_ACTION.into(),
            queue_to_front: false,
            ignoring: false,
            last_queued_time: None,
        });

        rotator.rotate_action(&context, &detector, &mut player);
        assert_eq!(rotator.priority_actions_queue.len(), 1);
        assert_eq!(player.priority_action_id(), Some(2));

        // add 1 front priority action
        rotator.priority_actions.insert(4, PriorityAction {
            condition: Box::new(|_, _, _| true),
            condition_kind: None,
            action: PlayerAction::SolveRune,
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
            condition: Box::new(|_, _, _| true),
            condition_kind: None,
            action: PlayerAction::SolveRune,
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
}
