use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{PlayerState, timeout::Timeout};
use crate::{
    context::Context,
    minimap::Minimap,
    player::{MOVE_TIMEOUT, Player, timeout::update_with_timeout},
    task::{Update, update_detection_task},
};

/// Updates the [`Player::Unstucking`] contextual state
///
/// This state can only be transitioned to when [`PlayerState::unstuck_counter`] reached the fixed
/// threshold or when the player moved into the edges of the minimap.
/// If [`PlayerState::unstuck_consecutive_counter`] has not reached the threshold and the player
/// moved into the left/right/top edges of the minimap, it will try to move
/// out as appropriate. It will also try to press ESC key to exit any dialog.
///
/// Each initial transition to [`Player::Unstucking`] increases
/// the [`PlayerState::unstuck_consecutive_counter`] by one. If the threshold is reached, this
/// state will enter GAMBA mode. And by definition, it means `random bullsh*t go`.
pub fn update_unstucking_context(
    context: &Context,
    state: &mut PlayerState,
    timeout: Timeout,
    has_settings: Option<bool>,
) -> Player {
    const Y_IGNORE_THRESHOLD: i32 = 18;
    // what is gamba mode? i am disappointed if you don't know
    const GAMBA_MODE_COUNT: u32 = 3;
    /// Random threshold to choose unstucking direction
    const X_TO_RIGHT_THRESHOLD: i32 = 10;

    let Minimap::Idle(idle) = context.minimap else {
        return Player::Detecting;
    };

    if !timeout.started {
        if state.unstuck_consecutive_counter + 1 < GAMBA_MODE_COUNT && has_settings.is_none() {
            let Update::Ok(has_settings) =
                update_detection_task(context, 0, &mut state.unstuck_task, move |detector| {
                    Ok(detector.detect_esc_settings())
                })
            else {
                return Player::Unstucking(timeout, has_settings);
            };
            return Player::Unstucking(timeout, Some(has_settings));
        }
        debug_assert!(
            state.unstuck_consecutive_counter + 1 >= GAMBA_MODE_COUNT || has_settings.is_some()
        );
        if state.unstuck_consecutive_counter < GAMBA_MODE_COUNT {
            state.unstuck_consecutive_counter += 1;
        }
    }

    let pos = state
        .last_known_pos
        .map(|pos| Point::new(pos.x, idle.bbox.height - pos.y));
    let is_gamba_mode = pos.is_none() || state.unstuck_consecutive_counter >= GAMBA_MODE_COUNT;

    update_with_timeout(
        timeout,
        MOVE_TIMEOUT,
        |timeout| {
            if has_settings.unwrap_or_default() || is_gamba_mode {
                let _ = context.keys.send(KeyKind::Esc);
            }
            let to_right = match (is_gamba_mode, pos) {
                (true, _) => rand::random_bool(0.5),
                (_, Some(Point { y, .. })) if y <= Y_IGNORE_THRESHOLD => {
                    return Player::Unstucking(timeout, has_settings);
                }
                (_, Some(Point { x, .. })) => x <= X_TO_RIGHT_THRESHOLD,
                (_, None) => unreachable!(),
            };
            if to_right {
                let _ = context.keys.send_down(KeyKind::Right);
            } else {
                let _ = context.keys.send_down(KeyKind::Left);
            }
            Player::Unstucking(timeout, has_settings)
        },
        || {
            let _ = context.keys.send_up(KeyKind::Down);
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            Player::Detecting
        },
        |timeout| {
            let send_space = match (is_gamba_mode, pos) {
                (true, _) => true,
                (_, Some(pos)) if pos.y > Y_IGNORE_THRESHOLD => true,
                _ => false,
            };
            if send_space {
                let _ = context.keys.send(state.config.jump_key);
            }
            Player::Unstucking(timeout, has_settings)
        },
    )
}
