#[cfg(test)]
use strum::EnumIter;

use crate::{
    context::{Context, Contextual, ControlFlow},
    detect::Detector,
};

const BUFF_CHECK_EVERY_TICKS: u32 = 215; // around 7 seconds

#[derive(Debug)]
pub struct BuffState {
    kind: BuffKind,
    interval: u32,
}

impl BuffState {
    pub fn new(kind: BuffKind) -> Self {
        Self { kind, interval: 0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Buff {
    NoBuff,
    HasBuff,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(test, derive(EnumIter))]
pub enum BuffKind {
    Rune,
    SayramElixir,
    ExpCouponX3,
    BonusExpCoupon,
    LegionWealth,
    LegionLuck,
}

impl Contextual for Buff {
    type Persistent = BuffState;

    fn update(
        self,
        _: &Context,
        detector: &mut impl Detector,
        state: &mut BuffState,
    ) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, detector, state))
    }
}

#[inline]
fn update_context(contextual: Buff, detector: &mut impl Detector, state: &mut BuffState) -> Buff {
    let next = if state.interval % BUFF_CHECK_EVERY_TICKS == 0 {
        let has_buff = match state.kind {
            BuffKind::Rune => detector.detect_player_rune_buff(),
            BuffKind::SayramElixir => detector.detect_player_sayram_elixir_buff(),
            BuffKind::ExpCouponX3 => detector.detect_player_exp_coupon_x3_buff(),
            BuffKind::BonusExpCoupon => detector.detect_player_bonus_exp_coupon_buff(),
            BuffKind::LegionWealth => detector.detect_player_legion_wealth_buff(),
            BuffKind::LegionLuck => detector.detect_player_legion_luck_buff(),
        };
        if has_buff {
            Buff::HasBuff
        } else {
            Buff::NoBuff
        }
    } else {
        contextual
    };
    state.interval = (state.interval + 1) % BUFF_CHECK_EVERY_TICKS;
    next
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;
    use crate::detect::MockDetector;

    fn detector_with_kind(kind: BuffKind, result: bool) -> MockDetector {
        let mut detector = MockDetector::new();
        match kind {
            BuffKind::Rune => {
                detector
                    .expect_detect_player_rune_buff()
                    .times(1)
                    .return_const(result);
            }
            BuffKind::SayramElixir => {
                detector
                    .expect_detect_player_sayram_elixir_buff()
                    .times(1)
                    .return_const(result);
            }
            BuffKind::ExpCouponX3 => {
                detector
                    .expect_detect_player_exp_coupon_x3_buff()
                    .times(1)
                    .return_const(result);
            }
            BuffKind::BonusExpCoupon => {
                detector
                    .expect_detect_player_bonus_exp_coupon_buff()
                    .times(1)
                    .return_const(result);
            }
            BuffKind::LegionWealth => {
                detector
                    .expect_detect_player_legion_wealth_buff()
                    .times(1)
                    .return_const(result);
            }
            BuffKind::LegionLuck => {
                detector
                    .expect_detect_player_legion_luck_buff()
                    .times(1)
                    .return_const(result);
            }
        }
        detector
    }

    #[test]
    fn buff_no_buff_to_has_buff() {
        for kind in BuffKind::iter() {
            let mut detector = detector_with_kind(kind, true);
            let mut state = BuffState::new(kind);

            let buff = update_context(Buff::NoBuff, &mut detector, &mut state);
            assert!(matches!(buff, Buff::HasBuff));
        }
    }

    #[test]
    fn buff_has_buff_to_no_buff() {
        for kind in BuffKind::iter() {
            let mut detector = detector_with_kind(kind, false);
            let mut state = BuffState::new(kind);

            let buff = update_context(Buff::HasBuff, &mut detector, &mut state);
            assert!(matches!(buff, Buff::NoBuff));
        }
    }

    #[test]
    fn buff_interval_check() {
        for kind in BuffKind::iter() {
            let mut detector = detector_with_kind(kind, true);
            let mut state = BuffState::new(kind);
            state.interval = 1; // skip initial check

            let mut buff = Buff::NoBuff;
            for _ in 0..BUFF_CHECK_EVERY_TICKS - 1 {
                buff = update_context(buff, &mut detector, &mut state);
                assert!(matches!(buff, Buff::NoBuff));
            }

            buff = update_context(buff, &mut detector, &mut state);
            assert!(matches!(buff, Buff::HasBuff));
            assert_eq!(state.interval, 1);
        }
    }
}
