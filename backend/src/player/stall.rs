use log::debug;

use super::{
    Player, PlayerAction, PlayerState,
    actions::on_action_state_mut,
    state::AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT,
    timeout::{Timeout, update_with_timeout},
};

/// Updates the [`Player::Stalling`] contextual state
///
/// This state stalls for the specified number of `max_timeout`. Upon timing out,
/// it will return to [`PlayerState::stalling_timeout_state`] if [`Some`] or
/// [`Player::Idle`] if [`None`]. And [`Player::Idle`] is considered the terminal state if
/// there is an action. [`PlayerState::stalling_timeout_state`] is currently only [`Some`] when
/// it is transitioned via [`Player::UseKey`].
///
/// If this state timeout in auto mob with terminal state, it will perform
/// auto mob reachable `y` solidifying if needed.
pub fn update_stalling_context(
    state: &mut PlayerState,
    timeout: Timeout,
    max_timeout: u32,
) -> Player {
    let update = |timeout| Player::Stalling(timeout, max_timeout);
    let next = update_with_timeout(
        timeout,
        max_timeout,
        update,
        || state.stalling_timeout_state.take().unwrap_or(Player::Idle),
        update,
    );

    on_action_state_mut(
        state,
        |state, action| match action {
            PlayerAction::AutoMob(_) => {
                let is_terminal = matches!(next, Player::Idle);
                if is_terminal && state.auto_mob_reachable_y_require_update() {
                    if !state.is_stationary {
                        return Some((Player::Stalling(Timeout::default(), max_timeout), false));
                    }
                    // state.last_known_pos is explicitly used instead of state.auto_mob_reachable_y
                    // because they might not be the same
                    if let Some(pos) = state.last_known_pos {
                        if state.auto_mob_reachable_y.is_some_and(|y| y != pos.y) {
                            let y = state.auto_mob_reachable_y.unwrap();
                            let count = state.auto_mob_reachable_y_map.get_mut(&y).unwrap();
                            *count = count.saturating_sub(1);
                            if *count == 0 {
                                state.auto_mob_reachable_y_map.remove(&y);
                                state.auto_mob_reachable_y = None;
                            }
                        }
                        let count = state.auto_mob_reachable_y_map.entry(pos.y).or_insert(0);
                        if *count < AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT {
                            *count += 1;
                        }
                        debug_assert!(*count <= AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT);
                        debug!(target: "player", "auto mob additional reachable y {} / {}", pos.y, count);
                    }
                }
                Some((next, is_terminal))
            }
            PlayerAction::Key(_) | PlayerAction::Move(_) | PlayerAction::SolveRune => {
                Some((next, matches!(next, Player::Idle)))
            }
        },
        || next,
    )
}
