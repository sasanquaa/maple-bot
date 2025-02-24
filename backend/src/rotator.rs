use std::{collections::VecDeque, time::Instant};

use log::debug;

use crate::{
    ActionKeyDirection, ActionKeyWith, KeyBinding,
    buff::Buff,
    context::{Context, ERDA_SHOWER_SKILL_POSITION, RUNE_BUFF_POSITION},
    database::{Action, ActionCondition, ActionKey, ActionMove},
    minimap::Minimap,
    player::{PlayerAction, PlayerState},
    skill::Skill,
};

const COOLDOWN_BETWEEN_QUEUE_MILLIS: u128 = 20_000;

type Condition = Box<dyn Fn(&Context, Option<Instant>) -> bool>;

struct PriorityAction {
    condition: Condition,
    action: PlayerAction,
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
    priority_actions_queue: VecDeque<PlayerAction>,
}

impl Rotator {
    pub fn build_actions(&mut self, actions: &[Action], buffs: &[(usize, KeyBinding)]) {
        debug!(target: "rotator", "preparing actions {actions:?} {buffs:?}");
        self.reset();
        self.normal_actions.clear();
        self.priority_actions.clear();

        for action in actions.iter().cloned() {
            match action {
                Action::Move(ActionMove { condition, .. })
                | Action::Key(ActionKey { condition, .. }) => match condition {
                    ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown => {
                        self.priority_actions.push(PriorityAction {
                            condition: Box::new(move |context, last_queued_time| {
                                should_queue_fixed_action(context, last_queued_time, condition)
                            }),
                            action: PlayerAction::Fixed(action),
                            last_queued_time: None,
                        })
                    }
                    ActionCondition::Any => self.normal_actions.push(PlayerAction::Fixed(action)),
                },
            }
        }
        self.priority_actions.push(PriorityAction {
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
            action: PlayerAction::SolveRune,
            last_queued_time: None,
        });
        for (i, key) in buffs.iter().copied() {
            self.priority_actions.push(PriorityAction {
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
                action: PlayerAction::Fixed(Action::Key(ActionKey {
                    key,
                    position: None,
                    condition: ActionCondition::Any,
                    direction: ActionKeyDirection::Any,
                    with: ActionKeyWith::Stationary,
                    wait_before_use_ticks: 10,
                    wait_after_use_ticks: 10,
                })),
                last_queued_time: None,
            });
        }
    }

    pub fn rotator_mode(&mut self, mode: RotatorMode) {
        self.normal_rotate_mode = mode;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.normal_action_backward = false;
        self.normal_index = 0;
        self.priority_actions_queue.clear();
    }

    pub fn rotate_action(&mut self, context: &Context, player: &mut PlayerState) {
        if player.has_priority_action() || !self.priority_actions_queue.is_empty() {
            if !player.has_priority_action() {
                player.set_priority_action(self.priority_actions_queue.pop_front().unwrap());
            }
            return;
        }
        if !self.priority_actions.is_empty() {
            for action in self.priority_actions.iter_mut() {
                if (action.condition)(context, action.last_queued_time) {
                    action.last_queued_time = Some(Instant::now());
                    self.priority_actions_queue.push_back(action.action);
                }
            }
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

fn at_least_millis_passed_since(last_queued_time: Option<Instant>, millis: u128) -> bool {
    last_queued_time
        .map(|instant| Instant::now().duration_since(instant).as_millis() >= millis)
        .unwrap_or(true)
}

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
