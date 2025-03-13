use log::debug;
use strum::EnumIter;

use crate::{
    context::{Context, Contextual, ControlFlow, Timeout, update_with_timeout},
    detect::Detector,
    player::Player,
};

pub const BUFF_CHECK_EVERY_TICKS: u32 = 215; // around 7 seconds
const BUFF_FAIL_MAX_COUNT: u32 = 3;

#[derive(Debug)]
pub struct BuffState {
    /// The kind of buff
    kind: BuffKind,
    /// Timeout for detecting buff in a fixed interval
    timeout: Timeout,
    /// The count `Buff::HasBuff` has failed to detect
    fail_count: u32,
}

impl BuffState {
    pub fn new(kind: BuffKind) -> Self {
        Self {
            kind,
            timeout: Timeout::default(),
            fail_count: 0,
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
        detector: &mut impl Detector,
        state: &mut BuffState,
    ) -> ControlFlow<Self> {
        let next = if matches!(context.player, Player::CashShopThenExit(_, _, _)) {
            self
        } else {
            update_context(self, detector, state)
        };
        ControlFlow::Next(next)
    }
}

#[inline]
fn detect_offset_ticks_for(kind: BuffKind) -> u32 {
    match kind {
        BuffKind::Rune => 0,
        BuffKind::SayramElixir | BuffKind::AureliaElixir => 1,
        BuffKind::ExpCouponX3 | BuffKind::BonusExpCoupon => 2,
        BuffKind::LegionWealth => 3,
        BuffKind::LegionLuck => 4,
    }
}

#[inline]
fn update_context(contextual: Buff, detector: &mut impl Detector, state: &mut BuffState) -> Buff {
    let offset_ticks = detect_offset_ticks_for(state.kind);
    let (has_buff, timeout) = update_with_timeout(
        state.timeout,
        BUFF_CHECK_EVERY_TICKS + offset_ticks,
        (),
        |_, timeout| {
            (
                Some(match state.kind {
                    BuffKind::Rune => detector.detect_player_rune_buff(),
                    BuffKind::SayramElixir => detector.detect_player_sayram_elixir_buff(),
                    BuffKind::AureliaElixir => detector.detect_player_aurelia_elixir_buff(),
                    BuffKind::ExpCouponX3 => detector.detect_player_exp_coupon_x3_buff(),
                    BuffKind::BonusExpCoupon => detector.detect_player_bonus_exp_coupon_buff(),
                    BuffKind::LegionWealth => detector.detect_player_legion_wealth_buff(),
                    BuffKind::LegionLuck => detector.detect_player_legion_luck_buff(),
                }),
                timeout,
            )
        },
        |_| (None, Timeout::default()),
        |_, timeout| (None, timeout),
    );
    state.timeout = timeout;
    state.fail_count = if let Some(has_buff) = has_buff {
        if matches!(contextual, Buff::HasBuff) && !has_buff {
            state.fail_count + 1
        } else {
            0
        }
    } else {
        state.fail_count
    };
    if has_buff.is_some() {
        debug!(target: "buff", "{contextual:?} {state:?}");
    }
    match (has_buff, contextual) {
        (None, contextual) => contextual,
        (Some(has_buff), Buff::NoBuff) => {
            if has_buff {
                Buff::HasBuff
            } else {
                Buff::NoBuff
            }
        }
        (_, Buff::HasBuff) => {
            if state.fail_count >= BUFF_FAIL_MAX_COUNT {
                Buff::NoBuff
            } else {
                Buff::HasBuff
            }
        }
    }
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
            BuffKind::AureliaElixir => {
                detector
                    .expect_detect_player_aurelia_elixir_buff()
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
            let buff = update_context(buff, &mut detector, &mut state);
            assert_eq!(state.fail_count, 0);
            assert!(matches!(buff, Buff::HasBuff));
        }
    }

    #[test]
    fn buff_has_buff_to_no_buff() {
        for kind in BuffKind::iter() {
            let offset_ticks = detect_offset_ticks_for(kind);
            let mut detector = detector_with_kind(kind, false);
            let mut state = BuffState::new(kind);
            state.fail_count = BUFF_FAIL_MAX_COUNT + offset_ticks - 1;

            let buff = update_context(Buff::HasBuff, &mut detector, &mut state);
            assert_eq!(state.fail_count, BUFF_FAIL_MAX_COUNT + offset_ticks);
            assert_eq!(state.timeout, Timeout {
                started: true,
                ..Timeout::default()
            });
            assert!(matches!(buff, Buff::NoBuff));
        }
    }

    #[test]
    fn buff_interval_check() {
        for kind in BuffKind::iter() {
            let mut detector = detector_with_kind(kind, true);
            let mut state = BuffState::new(kind);
            state.timeout = Timeout {
                current: 0,
                started: true,
            }; // skip initial check

            let mut buff = Buff::NoBuff;
            let offset_ticks = detect_offset_ticks_for(kind);
            for _ in 0..BUFF_CHECK_EVERY_TICKS + offset_ticks {
                buff = update_context(buff, &mut detector, &mut state);
                assert!(matches!(buff, Buff::NoBuff));
            }
            // timing out and restart
            buff = update_context(buff, &mut detector, &mut state);
            assert!(matches!(buff, Buff::NoBuff));
            assert_eq!(state.timeout, Timeout::default());

            buff = update_context(buff, &mut detector, &mut state);
            assert!(matches!(buff, Buff::HasBuff));
            assert_eq!(state.timeout.current, 0);
        }
    }
}
