use std::{
    mem,
    ops::{Index, IndexMut},
};

use anyhow::Result;
use strum::EnumIter;

use crate::{
    context::{Context, Contextual, ControlFlow},
    detect::Detector,
    player::Player,
    task::{Task, Update, update_task_repeatable},
};

const BUFF_FAIL_MAX_COUNT: u32 = 5;

#[derive(Debug)]
pub struct BuffState {
    /// The kind of buff
    kind: BuffKind,
    /// Task for detecting buff
    task: Option<Task<Result<bool>>>,
    /// The count `Buff::HasBuff` has failed to detect
    fail_count: u32,
    max_fail_count: u32,
}

impl BuffState {
    pub fn new(kind: BuffKind) -> Self {
        Self {
            kind,
            task: None,
            fail_count: 0,
            max_fail_count: if matches!(kind, BuffKind::Rune) {
                1
            } else {
                BUFF_FAIL_MAX_COUNT
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Buff {
    NoBuff,
    HasBuff,
}

#[derive(Clone, Copy, Debug, EnumIter)]
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

    fn update(
        self,
        context: &Context,
        detector: &impl Detector,
        state: &mut BuffState,
    ) -> ControlFlow<Self> {
        let next = if matches!(context.player, Player::CashShopThenExit(_, _)) {
            self
        } else {
            update_context(self, detector, state)
        };
        ControlFlow::Next(next)
    }
}

#[inline]
fn update_context(contextual: Buff, detector: &impl Detector, state: &mut BuffState) -> Buff {
    let detector = detector.clone();
    let kind = state.kind;
    let Update::Complete(Ok(has_buff)) = update_task_repeatable(5000, &mut state.task, move || {
        Ok(match kind {
            BuffKind::Rune => detector.detect_player_rune_buff(),
            BuffKind::SayramElixir => detector.detect_player_sayram_elixir_buff(),
            BuffKind::AureliaElixir => detector.detect_player_aurelia_elixir_buff(),
            BuffKind::ExpCouponX3 => detector.detect_player_exp_coupon_x3_buff(),
            BuffKind::BonusExpCoupon => detector.detect_player_bonus_exp_coupon_buff(),
            BuffKind::LegionWealth => detector.detect_player_legion_wealth_buff(),
            BuffKind::LegionLuck => detector.detect_player_legion_luck_buff(),
            BuffKind::WealthAcquisitionPotion => {
                detector.detect_player_wealth_acquisition_potion_buff()
            }
            BuffKind::ExpAccumulationPotion => {
                detector.detect_player_exp_accumulation_potion_buff()
            }
            BuffKind::ExtremeRedPotion => detector.detect_player_extreme_red_potion_buff(),
            BuffKind::ExtremeBluePotion => detector.detect_player_extreme_blue_potion_buff(),
            BuffKind::ExtremeGreenPotion => detector.detect_player_extreme_green_potion_buff(),
            BuffKind::ExtremeGoldPotion => detector.detect_player_extreme_gold_potion_buff(),
        })
    }) else {
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

    use strum::IntoEnumIterator;
    use tokio::time;

    use super::*;
    use crate::detect::MockDetector;

    fn detector_with_kind(kind: BuffKind, result: bool) -> MockDetector {
        let mut detector = MockDetector::new();
        detector
            .expect_clone()
            .returning(move || detector_with_kind(kind, result));
        match kind {
            BuffKind::Rune => {
                detector
                    .expect_detect_player_rune_buff()
                    .return_const(result);
            }
            BuffKind::SayramElixir => {
                detector
                    .expect_detect_player_sayram_elixir_buff()
                    .return_const(result);
            }
            BuffKind::AureliaElixir => {
                detector
                    .expect_detect_player_aurelia_elixir_buff()
                    .return_const(result);
            }
            BuffKind::ExpCouponX3 => {
                detector
                    .expect_detect_player_exp_coupon_x3_buff()
                    .return_const(result);
            }
            BuffKind::BonusExpCoupon => {
                detector
                    .expect_detect_player_bonus_exp_coupon_buff()
                    .return_const(result);
            }
            BuffKind::LegionWealth => {
                detector
                    .expect_detect_player_legion_wealth_buff()
                    .return_const(result);
            }
            BuffKind::LegionLuck => {
                detector
                    .expect_detect_player_legion_luck_buff()
                    .return_const(result);
            }
            BuffKind::WealthAcquisitionPotion => {
                detector
                    .expect_detect_player_wealth_acquisition_potion_buff()
                    .return_const(result);
            }
            BuffKind::ExpAccumulationPotion => {
                detector
                    .expect_detect_player_exp_accumulation_potion_buff()
                    .return_const(result);
            }
            BuffKind::ExtremeRedPotion => {
                detector
                    .expect_detect_player_extreme_red_potion_buff()
                    .return_const(result);
            }
            BuffKind::ExtremeBluePotion => {
                detector
                    .expect_detect_player_extreme_blue_potion_buff()
                    .return_const(result);
            }
            BuffKind::ExtremeGreenPotion => {
                detector
                    .expect_detect_player_extreme_green_potion_buff()
                    .return_const(result);
            }
            BuffKind::ExtremeGoldPotion => {
                detector
                    .expect_detect_player_extreme_gold_potion_buff()
                    .return_const(result);
            }
        }
        detector
    }

    async fn advance_task(
        contextual: Buff,
        detector: &impl Detector,
        state: &mut BuffState,
    ) -> Buff {
        let mut buff = update_context(contextual, detector, state);
        while !state.task.as_ref().unwrap().completed() {
            buff = update_context(buff, detector, state);
            time::advance(Duration::from_millis(1000)).await;
        }
        buff
    }

    #[tokio::test(start_paused = true)]
    async fn buff_no_buff_to_has_buff() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, true);
            let mut state = BuffState::new(kind);

            let buff = advance_task(Buff::NoBuff, &detector, &mut state).await;
            let buff = update_context(buff, &detector, &mut state);
            assert_eq!(state.fail_count, 0);
            assert_matches!(buff, Buff::HasBuff);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn buff_has_buff_to_no_buff() {
        for kind in BuffKind::iter() {
            let detector = detector_with_kind(kind, false);
            let mut state = BuffState::new(kind);
            state.max_fail_count = BUFF_FAIL_MAX_COUNT;
            state.fail_count = state.max_fail_count - 1;

            let buff = advance_task(Buff::HasBuff, &detector, &mut state).await;
            assert_eq!(state.fail_count, state.max_fail_count);
            assert_matches!(buff, Buff::NoBuff);
        }
    }
}
