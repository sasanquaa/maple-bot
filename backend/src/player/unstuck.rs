use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{PlayerState, timeout::Timeout};
use crate::{
    context::Context,
    minimap::Minimap,
    player::{MOVE_TIMEOUT, Player, timeout::update_with_timeout},
    task::{Update, update_detection_task},
};

/// A threshold to consider spamming falling action
///
/// This is when the player is inside the top edge of minimap. At least for higher level maps, this
/// seems rare but one possible map is The Forest Of Earth in Arcana.
const Y_IGNORE_THRESHOLD: i32 = 18;

/// Random threshold to choose unstucking direction
const X_TO_RIGHT_THRESHOLD: i32 = 10;

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
    gamba_mode: bool,
) -> Player {
    let Minimap::Idle(idle) = context.minimap else {
        return Player::Detecting;
    };

    if !timeout.started && !gamba_mode && has_settings.is_none() {
        let Update::Ok(has_settings) =
            update_detection_task(context, 0, &mut state.unstuck_task, move |detector| {
                Ok(detector.detect_esc_settings())
            })
        else {
            return Player::Unstucking(timeout, has_settings, gamba_mode);
        };
        return Player::Unstucking(timeout, Some(has_settings), gamba_mode);
    }

    let pos = state
        .last_known_pos
        .map(|pos| Point::new(pos.x, idle.bbox.height - pos.y));
    let gamba_mode = gamba_mode || pos.is_none();

    update_with_timeout(
        timeout,
        MOVE_TIMEOUT,
        |timeout| {
            if has_settings.unwrap_or_default() || gamba_mode {
                let _ = context.keys.send(KeyKind::Esc);
            }
            let to_right = match (gamba_mode, pos) {
                (true, _) => rand::random_bool(0.5),
                (_, Some(Point { y, .. })) if y <= Y_IGNORE_THRESHOLD => {
                    return Player::Unstucking(timeout, has_settings, gamba_mode);
                }
                (_, Some(Point { x, .. })) => x <= X_TO_RIGHT_THRESHOLD,
                (_, None) => unreachable!(),
            };
            if to_right {
                let _ = context.keys.send_down(KeyKind::Right);
            } else {
                let _ = context.keys.send_down(KeyKind::Left);
            }
            Player::Unstucking(timeout, has_settings, gamba_mode)
        },
        || {
            let _ = context.keys.send_up(KeyKind::Down);
            let _ = context.keys.send_up(KeyKind::Right);
            let _ = context.keys.send_up(KeyKind::Left);
            Player::Detecting
        },
        |timeout| {
            let send_space = match (gamba_mode, pos) {
                (true, _) => true,
                (_, Some(pos)) if pos.y > Y_IGNORE_THRESHOLD => true,
                _ => false,
            };
            if send_space {
                let _ = context.keys.send(state.config.jump_key);
            }
            Player::Unstucking(timeout, has_settings, gamba_mode)
        },
    )
}
