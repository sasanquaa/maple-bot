use platforms::windows::KeyKind;

use super::{Player, PlayerState, actions::PlayerAction};
use crate::{
    context::Context,
    player::{
        MAX_RUNE_FAILED_COUNT, on_action_state_mut,
        timeout::{Timeout, update_with_timeout},
        update_rune_fail_count_state,
    },
    task::{Update, update_detection_task},
};

#[derive(Clone, Copy, Default, Debug)]
pub struct SolvingRune {
    timeout: Timeout,
    keys: Option<[KeyKind; 4]>,
    key_index: usize,
}

/// Updates the [`Player::SolvingRune`] contextual state
///
/// Though this state can only be transitioned via [`Player::Moving`]
/// with [`PlayerAction::SolveRune`], it is not required. This state does:
/// - On timeout start, sends the interact key
/// - On timeout update, detects the rune and sends the keys
/// - On timeout end or rune is solved before timing out, transitions to `Player::Idle`
pub fn update_solving_rune_context(
    context: &Context,
    state: &mut PlayerState,
    solving_rune: SolvingRune,
) -> Player {
    const TIMEOUT: u32 = 155;
    const PRESS_KEY_INTERVAL: u32 = 8;

    debug_assert!(state.rune_validate_timeout.is_none());
    debug_assert!(state.rune_failed_count < MAX_RUNE_FAILED_COUNT);
    debug_assert!(!state.rune_cash_shop);
    let next = update_with_timeout(
        solving_rune.timeout,
        TIMEOUT,
        |timeout| {
            let _ = context.keys.send(state.config.interact_key);
            Player::SolvingRune(SolvingRune {
                timeout,
                ..solving_rune
            })
        },
        || {
            // likely a spinning rune if the bot can't detect and timeout
            Player::Idle
        },
        |timeout| {
            if solving_rune.keys.is_none() {
                let Update::Ok(keys) =
                    update_detection_task(context, 500, &mut state.rune_task, move |detector| {
                        detector.detect_rune_arrows()
                    })
                else {
                    return Player::SolvingRune(SolvingRune {
                        timeout,
                        ..solving_rune
                    });
                };
                return Player::SolvingRune(SolvingRune {
                    // reset current timeout for pressing keys
                    timeout: Timeout {
                        current: 1, // starts at 1 instead of 0 to avoid immediate key press
                        total: 1,
                        started: true,
                    },
                    keys: Some(keys),
                    ..solving_rune
                });
            }
            if timeout.current % PRESS_KEY_INTERVAL != 0 {
                return Player::SolvingRune(SolvingRune {
                    timeout,
                    ..solving_rune
                });
            }
            debug_assert!(solving_rune.key_index != 0 || timeout.current == PRESS_KEY_INTERVAL);
            debug_assert!(
                solving_rune
                    .keys
                    .is_some_and(|keys| solving_rune.key_index < keys.len())
            );
            let keys = solving_rune.keys.unwrap();
            let key_index = solving_rune.key_index;
            let _ = context.keys.send(keys[key_index]);
            let key_index = solving_rune.key_index + 1;
            if key_index >= keys.len() {
                Player::Idle
            } else {
                Player::SolvingRune(SolvingRune {
                    timeout,
                    key_index,
                    ..solving_rune
                })
            }
        },
    );

    on_action_state_mut(
        state,
        |state, action| match action {
            PlayerAction::SolveRune => {
                let is_terminal = matches!(next, Player::Idle);
                if is_terminal {
                    if solving_rune.keys.is_some() {
                        state.rune_validate_timeout = Some(Timeout::default());
                    } else {
                        update_rune_fail_count_state(state);
                    }
                }
                Some((next, is_terminal))
            }
            PlayerAction::AutoMob(_) | PlayerAction::Key(_) | PlayerAction::Move(_) => {
                unreachable!()
            }
        },
        || next,
    )
}
