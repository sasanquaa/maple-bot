use crate::{
    context::{Context, Contextual, ControlFlow},
    detect::Detector,
};

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

fn update_context(contextual: Buff, detector: &mut impl Detector, state: &mut BuffState) -> Buff {
    const BUFF_CHECK_EVERY_TICKS: u32 = 215; // around 7 seconds

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
