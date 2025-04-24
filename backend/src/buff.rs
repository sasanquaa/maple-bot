use std::{
    mem,
    ops::{Index, IndexMut},
};

use anyhow::Result;
use strum::EnumIter;

use crate::{
    Configuration, Settings,
    context::{Context, Contextual, ControlFlow},
    player::Player,
    task::{Task, Update, update_detection_task},
};

const BUFF_FAIL_NORMAL_MAX_COUNT: u32 = 5;
// TODO: Test to see if this is reasonable
const BUFF_FAIL_HIGH_MAX_COUNT: u32 = 7; // Meant for WAP / EAP

#[derive(Debug)]
pub struct BuffState {
    /// The kind of buff
    kind: BuffKind,
    /// Task for detecting buff
    task: Option<Task<Result<bool>>>,
    /// The count [`Buff::HasBuff`] has failed to detect
    fail_count: u32,
    /// The maximum number of time [`Buff::HasBuff`] can fail before transitioning
    /// to [`Buff:NoBuff`]
    max_fail_count: u32,
    /// Whether a buff is enabled
    enabled: bool,
}

impl BuffState {
    pub fn new(kind: BuffKind) -> Self {
        Self {
            kind,
            task: None,
            fail_count: 0,
            max_fail_count: match kind {
                BuffKind::Rune => 1,
                BuffKind::WealthAcquisitionPotion | BuffKind::ExpAccumulationPotion => {
                    BUFF_FAIL_HIGH_MAX_COUNT
                }
                BuffKind::SayramElixir
                | BuffKind::AureliaElixir
                | BuffKind::ExpCouponX3
                | BuffKind::BonusExpCoupon
                | BuffKind::LegionWealth
                | BuffKind::LegionLuck
                | BuffKind::ExtremeRedPotion
                | BuffKind::ExtremeBluePotion
                | BuffKind::ExtremeGreenPotion
                | BuffKind::ExtremeGoldPotion => BUFF_FAIL_NORMAL_MAX_COUNT,
            },
            enabled: true,
        }
    }

    /// Update the enabled state of buff to only detect if enabled
    pub fn update_enabled_state(&mut self, config: &Configuration, settings: &Settings) {
        self.enabled = match self.kind {
            BuffKind::Rune => settings.enable_rune_solving,
            BuffKind::SayramElixir => config.sayram_elixir_key.enabled,
            BuffKind::AureliaElixir => config.aurelia_elixir_key.enabled,
            BuffKind::ExpCouponX3 => config.exp_x3_key.enabled,
            BuffKind::BonusExpCoupon => config.bonus_exp_key.enabled,
            BuffKind::LegionWealth => config.legion_wealth_key.enabled,
            BuffKind::LegionLuck => config.legion_luck_key.enabled,
            BuffKind::WealthAcquisitionPotion => config.wealth_acquisition_potion_key.enabled,
            BuffKind::ExpAccumulationPotion => config.exp_accumulation_potion_key.enabled,
            BuffKind::ExtremeRedPotion => config.extreme_red_potion_key.enabled,
            BuffKind::ExtremeBluePotion => config.extreme_blue_potion_key.enabled,
            BuffKind::ExtremeGreenPotion => config.extreme_green_potion_key.enabled,
            BuffKind::ExtremeGoldPotion => config.extreme_gold_potion_key.enabled,
        };
        if !self.enabled {
            self.fail_count = 0;
            self.task = None;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Buff {
    NoBuff,
    HasBuff,
}

#[derive(Clone, Copy, Debug, EnumIter)]
#[cfg_attr(test, derive(PartialEq))]
#[repr(usize)]
pub enum BuffKind {
    /// NOTE: Upon failing to solving rune, there is a cooldown
    /// that looks exactly like the normal rune buff
    Rune,
    SayramElixir,
    AureliaElixir,
    ExpCouponX3,
    BonusExpCoupon,
    LegionWealth,
    LegionLuck,
    WealthAcquisitionPotion,
    ExpAccumulationPotion,
    ExtremeRedPotion,
    ExtremeBluePotion,
    ExtremeGreenPotion,
    ExtremeGoldPotion,
}

impl BuffKind {
    pub const COUNT: usize = mem::variant_count::<BuffKind>();
}

impl Index<BuffKind> for [Buff; BuffKind::COUNT] {
    type Output = Buff;

    fn index(&self, index: BuffKind) -> &Self::Output {
        self.get(index as usize).unwrap()
    }
}

impl IndexMut<BuffKind> for [Buff; BuffKind::COUNT] {
    fn index_mut(&mut self, index: BuffKind) -> &mut Self::Output {
        self.get_mut(index as usize).unwrap()
    }
}

impl Contextual for Buff {
    type Persistent = BuffState;

    fn update(self, context: &Context, state: &mut BuffState) -> ControlFlow<Self> {
        if !state.enabled {
            return ControlFlow::Next(Buff::NoBuff);
        }
        let next = if matches!(context.player, Player::CashShopThenExit(_, _)) {
            self
        } else {
            update_context(self, context, state)
        };
        ControlFlow::Next(next)
    }
}

#[inline]
fn update_context(contextual: Buff, context: &Context, state: &mut BuffState) -> Buff {
    let kind = state.kind;
    let Update::Ok(has_buff) =
        update_detection_task(context, 5000, &mut state.task, move |detector| {
            Ok(detector.detect_player_buff(kind))
        })
    else {
        return contextual;
    };
    state.fail_count = if matches!(contextual, Buff::HasBuff) && !has_buff {
        state.fail_count + 1
    } else {
        0
    };
    match (has_buff, contextual) {
        (true, Buff::NoBuff) => Buff::HasBuff,
        (false, Buff::NoBuff) => Buff::NoBuff,
        (_, Buff::HasBuff) => {
            if state.fail_count >= state.max_fail_count {
                Buff::NoBuff
            } else {
                Buff::HasBuff
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{assert_matches::assert_matches, time::Duration};

    use mockall::predicate::eq;
    use strum::IntoEnumIterator;
    use tokio::time::advance;

    use super::*;
    use crate::detect::MockDetector;

    fn detector_with_kind(kind: BuffKind, result: bool) -> MockDetector {
        let mut detector = MockDetector::new();
        detector
            .expect_detect_player_buff()
            .with(eq(kind))
            .return_const(result);
        detector
            .expect_clone()
            .returning(move || detector_with_kind(kind, result));
        detector
    }

    async fn advance_task(contextual: Buff, context: &Context, state: &mut BuffState) -> Buff {
        let mut buff = update_context(contextual, context, state);
        while !state.task.as_ref().unwrap().completed() {
            buff = update_context(buff, context, state);
            advance(Duration::from_millis(1000)).await;
        }
        buff
    }

    #[tokio::test(start_paused = true)]
    async fn buff_no_buff_to_has_buff() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, true);
            let context = Context::new(None, Some(detector));
            let mut state = BuffState::new(kind);

            let buff = advance_task(Buff::NoBuff, &context, &mut state).await;
            let buff = update_context(buff, &context, &mut state);
            assert_eq!(state.fail_count, 0);
            assert_matches!(buff, Buff::HasBuff);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn buff_has_buff_to_no_buff() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, false);
            let context = Context::new(None, Some(detector));
            let mut state = BuffState::new(kind);
            state.max_fail_count = BUFF_FAIL_NORMAL_MAX_COUNT;
            state.fail_count = state.max_fail_count - 1;

            let buff = advance_task(Buff::HasBuff, &context, &mut state).await;
            assert_eq!(state.fail_count, state.max_fail_count);
            assert_matches!(buff, Buff::NoBuff);
        }
    }
}
