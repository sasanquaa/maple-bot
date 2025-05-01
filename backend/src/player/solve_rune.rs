use anyhow::Result;
use platforms::windows::KeyKind;

use super::{Player, PlayerState, actions::PlayerAction};
use crate::{
    context::Context,
    detect::{ArrowsCalibrating, ArrowsState},
    player::{
        on_action_state_mut,
        state::MAX_RUNE_FAILED_COUNT,
        timeout::{Timeout, update_with_timeout},
    },
    task::{Task, Update, update_task},
};

const TIMEOUT: u32 = 185;
const SOLVE_START_TICK: u32 = 30;

const PRESS_KEY_INTERVAL: u32 = 8;

#[derive(Clone, Copy, Default, Debug)]
pub struct SolvingRune {
    timeout: Timeout,
    keys: Option<[KeyKind; 4]>,
    key_index: usize,
    calibrating: ArrowsCalibrating,
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
    debug_assert!(state.rune_validate_timeout.is_none());
    debug_assert!(state.rune_failed_count < MAX_RUNE_FAILED_COUNT);
    debug_assert!(!state.rune_cash_shop);

    let update_timeout = |timeout| {
        Player::SolvingRune(SolvingRune {
            timeout,
            ..solving_rune
        })
    };
    let next = update_with_timeout(
        solving_rune.timeout,
        TIMEOUT,
        |timeout| {
            let _ = context.keys.send(state.config.interact_key);
            update_timeout(timeout)
        },
        || {
            // likely a spinning rune if the bot can't detect and timeout
            Player::Idle
        },
        |timeout| {
            if timeout.total <= SOLVE_START_TICK {
                return update_timeout(timeout);
            }
            if solving_rune.keys.is_none() {
                return calibrate_rune_arrows(context, timeout, &mut state.rune_task, solving_rune)
                    .unwrap_or(update_timeout(timeout));
            }
            if timeout.current % PRESS_KEY_INTERVAL != 0 {
                return update_timeout(timeout);
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
                        state.update_rune_fail_count_state();
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

fn calibrate_rune_arrows(
    context: &Context,
    timeout: Timeout,
    task: &mut Option<Task<Result<ArrowsState>>>,
    solving_rune: SolvingRune,
) -> Option<Player> {
    let state = if solving_rune.calibrating.has_spin_arrows() {
        // When there are spinning arrows, detect immediately on the main thread
        // so that there is no frame skip
        context
            .detector_unwrap()
            .detect_rune_arrows(solving_rune.calibrating)
            .ok()?
    } else {
        calibrate_rune_arrows_async(context, task, solving_rune.calibrating)?
    };

    let next = match state {
        ArrowsState::Calibrating(calibrating) => Player::SolvingRune(SolvingRune {
            timeout,
            calibrating,
            ..solving_rune
        }),
        ArrowsState::Complete(keys) => {
            Player::SolvingRune(SolvingRune {
                // reset current timeout for pressing keys
                timeout: Timeout {
                    current: 1, // starts at 1 instead of 0 to avoid immediate key press
                    ..timeout
                },
                keys: Some(keys),
                ..solving_rune
            })
        }
    };
    Some(next)
}

#[inline]
fn calibrate_rune_arrows_async(
    context: &Context,
    task: &mut Option<Task<Result<ArrowsState>>>,
    calibrating: ArrowsCalibrating,
) -> Option<ArrowsState> {
    match update_task(
        500,
        task,
        || (context.detector_cloned_unwrap(), calibrating),
        move |(detector, calibrating)| detector.detect_rune_arrows(calibrating),
    ) {
        Update::Ok(state) => Some(state),
        Update::Err(_) | Update::Pending => None,
    }
}
