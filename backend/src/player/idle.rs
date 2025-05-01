use log::debug;
use opencv::core::Point;
use platforms::windows::KeyKind;

use super::{
    JUMP_THRESHOLD, Player, PlayerAction, PlayerActionAutoMob, PlayerActionKey, PlayerActionMove,
    PlayerState,
    actions::on_action_state_mut,
    double_jump::DOUBLE_JUMP_THRESHOLD,
    grapple::{GRAPPLING_MAX_THRESHOLD, GRAPPLING_THRESHOLD},
    moving::{Moving, MovingIntermediates},
    use_key::UseKey,
};
use crate::{
    ActionKeyDirection, ActionKeyWith, Position,
    array::Array,
    context::Context,
    minimap::Minimap,
    pathing::{PlatformWithNeighbors, find_points_with},
    player::state::{AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT, AUTO_MOB_REACHABLE_Y_THRESHOLD},
};

/// Updates [`Player::Idle`] contextual state
///
/// This state does not do much on its own except when auto mobbing. It acts as entry
/// to other state when there is an action and helps clearing keys.
pub fn update_idle_context(context: &Context, state: &mut PlayerState) -> Player {
    state.last_destinations = None;
    state.last_movement = None;
    state.stalling_timeout_state = None;
    let _ = context.keys.send_up(KeyKind::Up);
    let _ = context.keys.send_up(KeyKind::Down);
    let _ = context.keys.send_up(KeyKind::Left);
    let _ = context.keys.send_up(KeyKind::Right);

    on_action_state_mut(
        state,
        |state, action| on_player_action(context, state, action),
        || Player::Idle,
    )
}

fn on_player_action(
    context: &Context,
    state: &mut PlayerState,
    action: PlayerAction,
) -> Option<(Player, bool)> {
    let cur_pos = state.last_known_pos.unwrap();
    match action {
        PlayerAction::AutoMob(PlayerActionAutoMob { position, .. }) => Some((
            ensure_reachable_auto_mob_y(context, state, cur_pos, position),
            false,
        )),
        PlayerAction::Move(PlayerActionMove { position, .. }) => {
            let x = get_x_destination(position);
            debug!(target: "player", "handling move: {} {}", x, position.y);
            Some((
                Player::Moving(Point::new(x, position.y), position.allow_adjusting, None),
                false,
            ))
        }
        PlayerAction::Key(PlayerActionKey {
            position: Some(position),
            ..
        }) => {
            let x = get_x_destination(position);
            debug!(target: "player", "handling move: {} {}", x, position.y);
            Some((
                Player::Moving(Point::new(x, position.y), position.allow_adjusting, None),
                false,
            ))
        }
        PlayerAction::Key(PlayerActionKey {
            position: None,
            with: ActionKeyWith::DoubleJump,
            direction,
            ..
        }) => {
            if matches!(direction, ActionKeyDirection::Any)
                || direction == state.last_known_direction
            {
                Some((
                    Player::DoubleJumping(Moving::new(cur_pos, cur_pos, false, None), true, true),
                    false,
                ))
            } else {
                Some((Player::UseKey(UseKey::from_action(action)), false))
            }
        }
        PlayerAction::Key(PlayerActionKey {
            position: None,
            with: ActionKeyWith::Any | ActionKeyWith::Stationary,
            ..
        }) => Some((Player::UseKey(UseKey::from_action(action)), false)),
        PlayerAction::SolveRune => {
            if let Minimap::Idle(idle) = context.minimap
                && let Some(rune) = idle.rune
            {
                if state.config.rune_platforms_pathing {
                    if !state.is_stationary {
                        return Some((Player::Idle, false));
                    }
                    let intermediates = find_points(
                        &idle.platforms,
                        cur_pos,
                        rune,
                        true,
                        state.config.rune_platforms_pathing_up_jump_only,
                    );
                    if let Some(mut intermediates) = intermediates {
                        state.last_destinations = Some(
                            intermediates
                                .inner
                                .into_iter()
                                .map(|(point, _)| point)
                                .collect(),
                        );
                        let (point, exact) = intermediates.next().unwrap();
                        return Some((Player::Moving(point, exact, Some(intermediates)), false));
                    }
                }
                state.last_destinations = Some(vec![rune]);
                return Some((Player::Moving(rune, true, None), false));
            }
            Some((Player::Idle, true))
        }
    }
}

fn get_x_destination(position: Position) -> i32 {
    let x_min = position.x.saturating_sub(position.x_random_range).max(0);
    let x_max = position.x.saturating_add(position.x_random_range + 1);
    rand::random_range(x_min..x_max)
}

fn ensure_reachable_auto_mob_y(
    context: &Context,
    state: &mut PlayerState,
    player_pos: Point,
    mob_pos: Position,
) -> Player {
    if !state.is_stationary {
        return Player::Idle;
    }
    if state.auto_mob_reachable_y_map.is_empty() {
        populate_auto_mob_reachable_y(context, state);
    }
    debug_assert!(!state.auto_mob_reachable_y_map.is_empty());
    let y = state
        .auto_mob_reachable_y_map
        .keys()
        .copied()
        .min_by_key(|y| (mob_pos.y - y).abs())
        .filter(|y| (mob_pos.y - y).abs() <= AUTO_MOB_REACHABLE_Y_THRESHOLD);
    let point = Point::new(mob_pos.x, y.unwrap_or(mob_pos.y));
    let intermediates = if state.config.auto_mob_platforms_pathing {
        match context.minimap {
            Minimap::Idle(idle) => find_points(
                &idle.platforms,
                player_pos,
                point,
                mob_pos.allow_adjusting,
                state.config.auto_mob_platforms_pathing_up_jump_only,
            ),
            _ => unreachable!(),
        }
    } else {
        None
    };
    debug!(target: "player", "auto mob reachable y {:?} {:?}", y, state.auto_mob_reachable_y_map);
    state.auto_mob_reachable_y = y;
    state.last_destinations = intermediates
        .map(|intermediates| {
            intermediates
                .inner
                .into_iter()
                .map(|(point, _)| point)
                .collect::<Vec<_>>()
        })
        .or(Some(vec![point]));
    intermediates
        .map(|mut intermediates| {
            let (point, exact) = intermediates.next().unwrap();
            Player::Moving(point, exact, Some(intermediates))
        })
        .unwrap_or(Player::Moving(point, mob_pos.allow_adjusting, None))
}

fn populate_auto_mob_reachable_y(context: &Context, state: &mut PlayerState) {
    if state.config.auto_mob_platforms_pathing {
        match context.minimap {
            Minimap::Idle(idle) => {
                // Believes in user input lets goo...
                for platform in idle.platforms {
                    state
                        .auto_mob_reachable_y_map
                        .insert(platform.y(), AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT);
                }
            }
            _ => unreachable!(),
        }
    }
    let _ = state.auto_mob_reachable_y_map.try_insert(
        state.last_known_pos.unwrap().y,
        AUTO_MOB_REACHABLE_Y_SOLIDIFY_COUNT - 1,
    );
    debug!(target: "player", "auto mob initial reachable y map {:?}", state.auto_mob_reachable_y_map);
}

#[inline]
fn find_points(
    platforms: &[PlatformWithNeighbors],
    cur_pos: Point,
    dest: Point,
    exact: bool,
    up_jump_only: bool,
) -> Option<MovingIntermediates> {
    let vertical_threshold = if up_jump_only {
        GRAPPLING_THRESHOLD
    } else {
        GRAPPLING_MAX_THRESHOLD
    };
    let vec = find_points_with(
        platforms,
        cur_pos,
        dest,
        DOUBLE_JUMP_THRESHOLD,
        JUMP_THRESHOLD,
        vertical_threshold,
    )?;
    let len = vec.len();
    let array = Array::from_iter(
        vec.into_iter()
            .enumerate()
            .map(|(i, point)| (point, if i == len - 1 { exact } else { false })),
    );
    Some(MovingIntermediates {
        current: 0,
        inner: array,
    })
}
