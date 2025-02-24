use opencv::core::Mat;

use crate::{
    context::{Context, Contextual, ControlFlow},
    detect::{
        detect_player_bonus_exp_coupon_buff, detect_player_exp_coupon_x3_buff,
        detect_player_legion_luck_buff, detect_player_legion_wealth_buff, detect_player_rune_buff,
        detect_player_sayram_elixir_buff,
    },
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

    fn update(self, _: &Context, mat: &Mat, state: &mut BuffState) -> ControlFlow<Self> {
        ControlFlow::Next(update_context(self, mat, state))
    }
}

fn update_context(contextual: Buff, mat: &Mat, state: &mut BuffState) -> Buff {
    const BUFF_CHECK_EVERY_TICKS: u32 = 215; // around 7 seconds

    let next = if state.interval % BUFF_CHECK_EVERY_TICKS == 0 {
        let has_buff = match state.kind {
            BuffKind::Rune => detect_player_rune_buff(mat),
            BuffKind::SayramElixir => detect_player_sayram_elixir_buff(mat),
            BuffKind::ExpCouponX3 => detect_player_exp_coupon_x3_buff(mat),
            BuffKind::BonusExpCoupon => detect_player_bonus_exp_coupon_buff(mat),
            BuffKind::LegionWealth => detect_player_legion_wealth_buff(mat),
            BuffKind::LegionLuck => detect_player_legion_luck_buff(mat),
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
