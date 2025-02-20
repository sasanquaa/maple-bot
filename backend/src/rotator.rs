use std::{collections::VecDeque, time::Instant};

use log::debug;

use crate::{
    context::{Context, ERDA_SHOWER_SKILL_POSITION},
    database::{Action, ActionCondition, ActionKey, ActionMove},
    minimap::Minimap,
    player::{PlayerAction, PlayerState},
    skill::Skill,
};

#[derive(Debug)]
struct PriorityAction {
    condition: ActionCondition,
    action: PlayerAction,
    last_queued_time: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct Rotator {
    normal_actions: Vec<PlayerAction>,
    normal_index: usize,
    normal_action_backward: bool,
    priority_actions: Vec<PriorityAction>,
    priority_actions_queue: VecDeque<PlayerAction>,
}

impl Rotator {
    pub fn build_actions(&mut self, actions: &[Action]) {
        debug!(target: "rotator", "preparing actions {actions:?}");
        self.reset();
        self.normal_actions.clear();
        self.priority_actions.clear();

        for action in actions.iter().cloned() {
            match action {
                Action::Move(ActionMove { condition, .. })
                | Action::Key(ActionKey { condition, .. }) => match condition {
                    ActionCondition::EveryMillis(_) | ActionCondition::ErdaShowerOffCooldown => {
                        self.priority_actions.push(PriorityAction {
                            condition,
                            action: PlayerAction::Fixed(action),
                            last_queued_time: None,
                        })
                    }
                    ActionCondition::Any => self.normal_actions.push(PlayerAction::Fixed(action)),
                },
            }
        }
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
        if let Minimap::Idle(idle) = context.minimap {
            if let Some(rune) = idle.rune
                && !player.has_rune_buff
            {
                self.priority_actions_queue
                    .push_back(PlayerAction::SolveRune(rune));
            }
        }
        if !self.priority_actions.is_empty() {
            for action in self.priority_actions.iter_mut() {
                if try_to_queue(context, action) {
                    self.priority_actions_queue.push_back(action.action);
                }
            }
        }
        if !player.has_normal_action() && !self.normal_actions.is_empty() {
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

fn try_to_queue(context: &Context, action: &mut PriorityAction) -> bool {
    const ERDA_SHOWER_COOLDOWN_BETWEEN_QUEUE_MILLIS: u128 = 10_000;

    let millis_should_passed = match action.condition {
        ActionCondition::EveryMillis(millis) => millis as u128,
        ActionCondition::ErdaShowerOffCooldown => ERDA_SHOWER_COOLDOWN_BETWEEN_QUEUE_MILLIS,
        ActionCondition::Any => unreachable!(),
    };
    let now = Instant::now();
    if action
        .last_queued_time
        .map(|instant| now.duration_since(instant).as_millis() < millis_should_passed as u128)
        .unwrap_or(false)
    {
        return false;
    }
    if matches!(action.condition, ActionCondition::ErdaShowerOffCooldown)
        && !matches!(context.skills[ERDA_SHOWER_SKILL_POSITION], Skill::Idle)
    {
        return false;
    }
    action.last_queued_time = Some(Instant::now());
    true
}
