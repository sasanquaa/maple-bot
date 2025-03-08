use std::{
    collections::VecDeque,
    sync::atomic::{AtomicI32, Ordering},
    time::Instant,
};

use log::debug;

use crate::{
    ActionKeyDirection, ActionKeyWith, KeyBinding,
    buff::Buff,
    context::{Context, ERDA_SHOWER_SKILL_POSITION, RUNE_BUFF_POSITION},
    database::{Action, ActionCondition, ActionKey, ActionMove},
    minimap::Minimap,
    player::{Player, PlayerAction, PlayerState},
    skill::Skill,
};

const COOLDOWN_BETWEEN_QUEUE_MILLIS: u128 = 20_000;
const COOLDOWN_BETWEEN_POTION_QUEUE_MILLIS: u128 = 2_000;

type Condition = Box<dyn Fn(&Context, Option<Instant>) -> bool>;

struct PriorityAction {
    id: i32,
    condition: Condition,
    condition_kind: Option<ActionCondition>,
    action: PlayerAction,
    ignoring: bool,
    last_queued_time: Option<Instant>,
}

#[derive(Default)]
pub enum RotatorMode {
    StartToEnd,
    #[default]
    StartToEndThenReverse,
}

#[derive(Default)]
pub struct Rotator {
    normal_actions: Vec<PlayerAction>,
    normal_index: usize,
    normal_action_backward: bool,
    normal_rotate_mode: RotatorMode,
    priority_actions: Vec<PriorityAction>,
    priority_actions_queue: VecDeque<(i32, PlayerAction)>,
}

impl Rotator {
    pub fn build_actions(
        &mut self,
        actions: &[Action],
        buffs: &[(usize, KeyBinding)],
        potion_key: KeyBinding,
    ) {
        debug!(target: "rotator", "preparing actions {actions:?} {buffs:?}");
        self.reset_queue();
        self.normal_actions.clear();
        self.priority_actions.clear();

        // this is literally free postfix increment!
        let id = AtomicI32::new(0);
        for action in actions.iter().copied() {
            match action {
                Action::Move(ActionMove { condition, .. })
                | Action::Key(ActionKey { condition, .. }) => match condition {
                    ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown => {
                        self.priority_actions.push(PriorityAction {
                            id: id.fetch_add(1, Ordering::Relaxed),
                            action: PlayerAction::Fixed(action),
                            condition: Box::new(move |context, last_queued_time| {
                                should_queue_fixed_action(context, last_queued_time, condition)
                            }),
                            condition_kind: Some(condition),
                            ignoring: false,
                            last_queued_time: None,
                        })
                    }
                    ActionCondition::Any => self.normal_actions.push(PlayerAction::Fixed(action)),
                },
            }
        }
        self.priority_actions.push(PriorityAction {
            id: id.fetch_add(1, Ordering::Relaxed),
            condition: Box::new(|context, last_queued_time| {
                if !at_least_millis_passed_since(
                    last_queued_time,
                    COOLDOWN_BETWEEN_POTION_QUEUE_MILLIS,
                ) {
                    return false;
                }
                if let Minimap::Idle(idle) = context.minimap {
                    return idle.has_elite_boss;
                }
                false
            }),
            condition_kind: None,
            action: PlayerAction::Fixed(Action::Key(ActionKey {
                key: potion_key,
                position: None,
                condition: ActionCondition::Any,
                direction: ActionKeyDirection::Any,
                with: ActionKeyWith::Any,
                wait_before_use_ticks: 5,
                wait_after_use_ticks: 0,
                queue_to_front: Some(true),
            })),
            ignoring: false,
            last_queued_time: None,
        });
        self.priority_actions.push(PriorityAction {
            id: id.fetch_add(1, Ordering::Relaxed),
            condition: Box::new(|context, last_queued_time| {
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
            ignoring: false,
            last_queued_time: None,
        });
        for (i, key) in buffs.iter().copied() {
            self.priority_actions.push(PriorityAction {
                id: id.fetch_add(1, Ordering::Relaxed),
                condition: Box::new(move |context, last_queued_time| {
                    if !at_least_millis_passed_since(
                        last_queued_time,
                        COOLDOWN_BETWEEN_QUEUE_MILLIS,
                    ) {
                        return false;
                    }
                    if !matches!(context.minimap, Minimap::Idle(_)) {
                        return false;
                    }
                    matches!(context.buffs[i], Buff::NoBuff)
                }),
                condition_kind: None,
                action: PlayerAction::Fixed(Action::Key(ActionKey {
                    key,
                    position: None,
                    condition: ActionCondition::Any,
                    direction: ActionKeyDirection::Any,
                    with: ActionKeyWith::Stationary,
                    wait_before_use_ticks: 10,
                    wait_after_use_ticks: 10,
                    queue_to_front: Some(true),
                })),
                ignoring: false,
                last_queued_time: None,
            });
        }
    }

    #[inline]
    pub fn rotator_mode(&mut self, mode: RotatorMode) {
        self.normal_rotate_mode = mode;
        self.reset_queue();
    }

    #[inline]
    pub fn reset_queue(&mut self) {
        self.normal_action_backward = false;
        self.normal_index = 0;
        self.priority_actions_queue.clear();
    }

    #[inline]
    fn has_erda_action_in_queue(&self) -> bool {
        self.priority_actions_queue.iter().any(|(_, action)| {
            matches!(
                action,
                PlayerAction::Fixed(
                    Action::Move(ActionMove {
                        condition: ActionCondition::ErdaShowerOffCooldown,
                        ..
                    }) | Action::Key(ActionKey {
                        condition: ActionCondition::ErdaShowerOffCooldown,
                        ..
                    }),
                )
            )
        })
    }

    pub fn rotate_action(&mut self, context: &Context, player: &mut PlayerState) {
        // what a mess
        let has_erda_action = self.has_erda_action_in_queue();
        for action in self.priority_actions.iter_mut() {
            action.ignoring = match action.condition_kind {
                Some(condition) => match condition {
                    ActionCondition::EveryMillis(_) => self
                        .priority_actions_queue
                        .iter()
                        .any(|(id, _)| *id == action.id),
                    ActionCondition::ErdaShowerOffCooldown => has_erda_action,
                    ActionCondition::Any => unreachable!(),
                },
                None => false,
            };
            if action.ignoring {
                action.last_queued_time = Some(Instant::now());
                continue;
            }
            if (action.condition)(context, action.last_queued_time) {
                let queue_to_front = match action.action {
                    PlayerAction::Fixed(Action::Key(ActionKey { queue_to_front, .. })) => {
                        queue_to_front.unwrap_or_default()
                    }
                    _ => false,
                };
                action.last_queued_time = Some(Instant::now());
                if queue_to_front {
                    self.priority_actions_queue
                        .push_front((action.id, action.action));
                } else {
                    self.priority_actions_queue
                        .push_back((action.id, action.action));
                }
            }
        }
        if !self.priority_actions_queue.is_empty() {
            let (front_id, front_action) = self.priority_actions_queue.pop_front().unwrap();
            let has_queue_to_front = match front_action {
                PlayerAction::Fixed(Action::Key(ActionKey { queue_to_front, .. })) => {
                    queue_to_front.unwrap_or_default()
                }
                _ => false,
            };
            if has_queue_to_front
                && !player.has_solve_rune_or_queue_front_action()
                && !matches!(context.player, Player::UseKey(_) | Player::Stalling(_, _))
            {
                if let Some(action) = player.replace_priority_action(front_id, front_action) {
                    self.priority_actions_queue.push_front(action);
                }
            } else if !player.has_priority_action() {
                player.set_priority_action(front_id, front_action);
            } else {
                self.priority_actions_queue
                    .push_front((front_id, front_action));
            }
        }
        if player.has_priority_action() {
            return;
        }
        if !player.has_normal_action() && !self.normal_actions.is_empty() {
            match self.normal_rotate_mode {
                RotatorMode::StartToEnd => {
                    let action = self.normal_actions[self.normal_index];
                    self.normal_index = (self.normal_index + 1) % self.normal_actions.len();
                    player.set_normal_action(action);
                }
                RotatorMode::StartToEndThenReverse => {
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
        }
    }
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
        && !matches!(context.skills[ERDA_SHOWER_SKILL_POSITION], Skill::Idle)
    {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use opencv::core::Point;

    use super::*;
    use crate::{Position, minimap::MinimapIdle};
    use std::time::{Duration, Instant};

    const NORMAL_ACTION: Action = Action::Move(ActionMove {
        position: Position {
            x: 0,
            y: 0,
            allow_adjusting: false,
        },
        condition: ActionCondition::Any,
        wait_after_move_ticks: 0,
    });
    const PRIORITY_ACTION: Action = Action::Move(ActionMove {
        position: Position {
            x: 0,
            y: 0,
            allow_adjusting: false,
        },
        condition: ActionCondition::ErdaShowerOffCooldown,
        wait_after_move_ticks: 0,
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

        context.skills[ERDA_SHOWER_SKILL_POSITION] = Skill::Idle;
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

        rotator.build_actions(&actions, &buffs, KeyBinding::A);
        assert_eq!(rotator.priority_actions.len(), 7);
        assert_eq!(rotator.normal_actions.len(), 2);
    }

    #[test]
    fn rotator_rotate_action_start_to_end_then_reverse() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::default();
        rotator.rotator_mode(RotatorMode::StartToEndThenReverse);
        for _ in 0..2 {
            rotator
                .normal_actions
                .push(PlayerAction::Fixed(NORMAL_ACTION));
        }

        rotator.rotate_action(&context, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_action_backward);
        assert_eq!(rotator.normal_index, 1);

        player.abort_actions();

        rotator.rotate_action(&context, &mut player);
        assert!(player.has_normal_action());
        assert!(rotator.normal_action_backward);
        assert_eq!(rotator.normal_index, 0);
    }

    #[test]
    fn rotator_rotate_action_start_to_end() {
        let mut rotator = Rotator::default();
        let mut player = PlayerState::default();
        let context = Context::default();
        rotator.rotator_mode(RotatorMode::StartToEnd);
        for _ in 0..2 {
            rotator
                .normal_actions
                .push(PlayerAction::Fixed(NORMAL_ACTION));
        }

        rotator.rotate_action(&context, &mut player);
        assert!(player.has_normal_action());
        assert!(!rotator.normal_action_backward);
        assert_eq!(rotator.normal_index, 1);

        player.abort_actions();

        rotator.rotate_action(&context, &mut player);
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
        let mut context = Context {
            minimap: Minimap::Idle(minimap),
            ..Context::default()
        };
        context.buffs[RUNE_BUFF_POSITION] = Buff::NoBuff;
        rotator.priority_actions.push(PriorityAction {
            id: 0,
            condition: Box::new(|context, _| matches!(context.minimap, Minimap::Idle(_))),
            condition_kind: None,
            action: PlayerAction::SolveRune,
            ignoring: false,
            last_queued_time: None,
        });

        rotator.rotate_action(&context, &mut player);
        assert_eq!(rotator.priority_actions_queue.len(), 0);
        assert!(player.has_priority_action());
    }
}
