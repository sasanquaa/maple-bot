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
