use opencv::core::Point;

use super::Moving;
use crate::player::Player;

/// The axis to which the change in position should be detected.
#[derive(Clone, Copy)]
pub enum ChangeAxis {
    /// Detects a change in x direction
    Horizontal,
    /// Detects a change in y direction
    Vertical,
    /// Detects a change in both directions
    Both,
}

/// A struct that stores the current tick before timing out
///
/// Most contextual states can be timed out as there is no guaranteed
/// an action will be performed or a state can be transitioned. So timeout is used to retry
/// such action/state and to avoid looping in a single state forever. Or
/// for some contextual states to perform an action only after timing out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Timeout {
    /// The current timeout tick.
    ///
    /// The timeout tick can be reset to 0 in the context of movement.
    pub current: u32,
    /// The total number of passed ticks.
    ///
    /// Useful when [`Self::current`] can be reset. And currently only used for delaying
    /// up-jumping and stopping down key early in falling
    pub total: u32,
    /// Inidcates whether the timeout has started
    pub started: bool,
}

/// Updates the [`Timeout`] current tick
///
/// This is basic building block for contextual states that can
/// be timed out.
#[inline]
pub fn update_with_timeout<T>(
    timeout: Timeout,
    max_timeout: u32,
    on_started: impl FnOnce(Timeout) -> T,
    on_timeout: impl FnOnce() -> T,
    on_update: impl FnOnce(Timeout) -> T,
) -> T {
    debug_assert!(max_timeout > 0, "max_timeout must be positive");
    debug_assert!(
        timeout.started || timeout == Timeout::default(),
        "started timeout in non-default state"
    );
    debug_assert!(
        timeout.current <= max_timeout,
        "current timeout tick larger than max_timeout"
    );

    match timeout {
        Timeout { started: false, .. } => on_started(Timeout {
            started: true,
            ..timeout
        }),
        Timeout { current, .. } if current >= max_timeout => on_timeout(),
        timeout => on_update(Timeout {
            current: timeout.current + 1,
            total: timeout.total + 1,
            ..timeout
        }),
    }
}

/// Updates movement-related contextual states
///
/// This function helps resetting the [`Timeout`] when the player's position changed
/// based on [`ChangeAxis`]. Upon timing out, it returns to [`Player::Moving`].
#[inline]
pub fn update_moving_axis_context(
    moving: Moving,
    cur_pos: Point,
    max_timeout: u32,
    on_started: impl FnOnce(Moving) -> Player,
    on_timeout: Option<impl FnOnce()>,
    on_update: impl FnOnce(Moving) -> Player,
    axis: ChangeAxis,
) -> Player {
    #[inline]
    fn update_moving_axis_timeout(
        prev_pos: Point,
        cur_pos: Point,
        timeout: Timeout,
        max_timeout: u32,
        axis: ChangeAxis,
    ) -> Timeout {
        if timeout.current >= max_timeout {
            return timeout;
        }
        let moved = match axis {
            ChangeAxis::Horizontal => cur_pos.x != prev_pos.x,
            ChangeAxis::Vertical => cur_pos.y != prev_pos.y,
            ChangeAxis::Both => cur_pos.x != prev_pos.x || cur_pos.y != prev_pos.y,
        };
        Timeout {
            current: if moved { 0 } else { timeout.current },
            ..timeout
        }
    }

    update_with_timeout(
        update_moving_axis_timeout(moving.pos, cur_pos, moving.timeout, max_timeout, axis),
        max_timeout,
        |timeout| on_started(moving.pos(cur_pos).timeout(timeout)),
        || {
            if let Some(callback) = on_timeout {
                callback();
            }
            Player::Moving(moving.dest, moving.exact, moving.intermediates)
        },
        |timeout| on_update(moving.pos(cur_pos).timeout(timeout)),
    )
}
