use platforms::windows::KeyKind;

use super::{
    Player, PlayerState,
    timeout::{Timeout, update_with_timeout},
};
use crate::context::Context;

#[derive(Clone, Copy, Debug)]
pub enum CashShop {
    Entering,
    Entered,
    Exitting,
    Exitted,
    Stalling,
}

// TODO: Improve this?
pub fn update_cash_shop_context(
    context: &Context,
    state: &PlayerState,
    timeout: Timeout,
    cash_shop: CashShop,
    failed_to_detect_player: bool,
) -> Player {
    match cash_shop {
        CashShop::Entering => {
            let _ = context.keys.send(state.config.cash_shop_key);
            let next = if context.detector_unwrap().detect_player_in_cash_shop() {
                CashShop::Entered
            } else {
                CashShop::Entering
            };
            Player::CashShopThenExit(timeout, next)
        }
        CashShop::Entered => {
            update_with_timeout(
                timeout,
                305, // exits after 10 secs
                |timeout| Player::CashShopThenExit(timeout, cash_shop),
                || Player::CashShopThenExit(timeout, CashShop::Exitting),
                |timeout| Player::CashShopThenExit(timeout, cash_shop),
            )
        }
        CashShop::Exitting => {
            let next = if context.detector_unwrap().detect_player_in_cash_shop() {
                CashShop::Exitting
            } else {
                CashShop::Exitted
            };
            let _ = context.keys.send_click_to_focus();
            let _ = context.keys.send(KeyKind::Esc);
            let _ = context.keys.send(KeyKind::Enter);
            Player::CashShopThenExit(timeout, next)
        }
        CashShop::Exitted => {
            if failed_to_detect_player {
                Player::CashShopThenExit(timeout, cash_shop)
            } else {
                Player::CashShopThenExit(Timeout::default(), CashShop::Stalling)
            }
        }
        CashShop::Stalling => {
            update_with_timeout(
                timeout,
                90, // returns after 3 secs
                |timeout| Player::CashShopThenExit(timeout, cash_shop),
                || Player::Idle,
                |timeout| Player::CashShopThenExit(timeout, cash_shop),
            )
        }
    }
}
