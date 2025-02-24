#![feature(variant_count)]
#![feature(map_try_insert)]

use std::str::FromStr;
use std::string::ToString;

use backend::{
    Action, ActionCondition, ActionConditionDiscriminants, ActionDiscriminants, ActionKey,
    ActionKeyDirection, ActionKeyDirectionDiscriminants, ActionKeyWith, ActionKeyWithDiscriminants,
    ActionMove, IntoEnumIterator, KeyBinding, KeyBindingDiscriminants, Minimap as MinimapData,
    Position, RotationMode, RotationModeDiscriminants, minimap_data, minimap_frame,
    player_position, prepare_actions, query_config, redetect_minimap, refresh_configuration,
    refresh_minimap_data, rotate_actions, start_update_loop, upsert_config, upsert_map,
};
use components::Checkbox;
use components::{
    Divider, NumberInput, TextInput,
    button::{OneButton, TwoButtons},
    options::Options,
};
use dioxus::{
    desktop::{
        WindowBuilder,
        wry::dpi::{PhysicalSize, Size},
    },
    document::EvalError,
    prelude::*,
};
use tracing_log::LogTracer;

mod components;

const TAILWIND_CSS: Asset = asset!("public/tailwind.css");
const DEFAULT_POSITION: Position = Position {
    x: 0,
    y: 0,
    allow_adjusting: true,
};
const DEFAULT_MOVE_ACTION: Action = Action::Move(ActionMove {
    position: DEFAULT_POSITION,
    condition: ActionCondition::Any,
    wait_after_move_ticks: 0,
});
const DEFAULT_KEY_ACTION: Action = Action::Key(ActionKey {
    key: KeyBinding::A,
    position: None,
    condition: ActionCondition::Any,
    direction: ActionKeyDirection::Any,
    with: ActionKeyWith::Any,
    wait_before_use_ticks: 0,
    wait_after_use_ticks: 0,
});

// 許してくれよ！UIなんてよくわからん
// 使えば十分よ！๑-﹏-๑

fn main() {
    LogTracer::init().unwrap();
    start_update_loop();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(1200, 780)))
        .with_resizable(false)
        .with_maximizable(false)
        .with_title("Maple Bot")
        .with_always_on_top(cfg!(debug_assertions));
    let cfg = dioxus::desktop::Config::default()
        .with_menu(None)
        .with_window(window);
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "flex justify-center", Minimap {} }
    }
}

#[component]
fn Minimap() -> Element {
    let mut halted = use_signal(|| true);
    let mut position = use_signal::<Option<(i32, i32)>>(|| None);
    let mut minimap = use_signal::<Option<MinimapData>>(|| None);
    let mut preset = use_signal::<Option<String>>(move || {
        if let Some(minimap) = &minimap() {
            minimap.actions.keys().next().cloned()
        } else {
            None
        }
    });
    let preset_insert_positions = use_memo::<Vec<(usize, String)>>(move || {
        if let Some(minimap) = &minimap() {
            if let Some(preset) = &preset() {
                let mut vec = minimap
                    .actions
                    .get(preset)
                    .unwrap_or(&Vec::new())
                    .iter()
                    .enumerate()
                    .map(|(i, _)| (i, i.to_string()))
                    .collect::<Vec<(usize, String)>>();
                vec.push((vec.len(), vec.len().to_string()));
                return vec;
            }
        }
        vec![]
    });
    let presets = use_memo::<Option<Vec<(String, String)>>>(move || {
        minimap().map(|minimap| {
            minimap
                .actions
                .keys()
                .cloned()
                .map(|key| (key.clone(), key))
                .collect()
        })
    });

    let mut editing = use_signal::<Option<usize>>(|| None);
    let mut editing_preset = use_signal::<String>(String::new);
    let mut editing_action = use_signal::<Action>(|| DEFAULT_MOVE_ACTION);
    let mut editing_insert_position = use_signal::<usize>(|| 0);
    let mut editing_action_last = use_signal::<ActionDiscriminants>(|| ActionDiscriminants::Move);
    let editing_action_set = use_callback(move |action: Action| {
        let action_disc = ActionDiscriminants::from(action);
        if let Some(i) = *editing.peek() {
            let minimap = minimap.peek();
            let minimap = minimap.as_ref().unwrap();
            let existing_action = minimap
                .actions
                .get(preset.peek().as_ref().unwrap())
                .unwrap()
                .get(i)
                .unwrap();
            if action_disc != *editing_action_last.peek()
                && action_disc == ActionDiscriminants::from(existing_action)
            {
                let is_default = match action {
                    Action::Move(_) => action == DEFAULT_MOVE_ACTION,
                    Action::Key(_) => action == DEFAULT_KEY_ACTION,
                };
                if is_default {
                    editing_action_last.set(action_disc);
                    editing_action.set(*existing_action);
                    return;
                }
            }
        }
        editing_action_last.set(action_disc);
        editing_action.set(action);
    });

    let reset = use_callback(move |()| {
        if position.peek().is_some() {
            position.set(None);
        }
        if minimap.peek().is_some() {
            minimap.set(None);
        }
        if preset.peek().is_some() {
            preset.set(None);
        }
        if editing.peek().is_some() {
            editing.set(None);
        }
    });

    use_effect(move || {
        let i = preset_insert_positions()
            .last()
            .map(|(i, _)| *i)
            .unwrap_or(0);
        editing_insert_position.set(i);
    });
    use_effect(move || {
        if let Some(preset) = preset() {
            spawn(async move {
                prepare_actions(preset).await;
            });
        }
    });
    use_effect(move || {
        if let Some(minimap) = &mut minimap() {
            upsert_map(minimap).unwrap();
            if preset.peek().is_none() {
                preset.set(minimap.actions.keys().next().cloned());
            }
            spawn(async move {
                refresh_minimap_data().await;
                if let Some(preset) = preset.peek().clone() {
                    prepare_actions(preset).await;
                }
            });
        }
    });
    use_future(move || async move {
        let mut canvas = document::eval(include_str!("js/minimap.js"));
        loop {
            let result = minimap_frame().await;
            let Ok(frame) = result else {
                reset(());
                continue;
            };
            if minimap.peek().is_none() {
                minimap.set(minimap_data().await.ok());
            }
            position.set(player_position().await.ok());
            let Err(error) = canvas.send(frame) else {
                continue;
            };
            if matches!(error, EvalError::Finished) {
                // probably: https://github.com/DioxusLabs/dioxus/issues/2979
                canvas = document::eval(include_str!("js/minimap.js"));
            }
        }
    });

    rsx! {
        div { class: "grid grid-cols-3 gap-x-[32px] p-[16px]",
            div { class: "grid grid-flow-row auto-rows-max gap-[8px] w-[350px] place-items-center",
                p { class: "font-main",
                    if let Some(minimap) = &minimap() {
                        "{minimap.name}"
                    } else {
                        "Detecting..."
                    }
                }
                if let Some((x, y)) = position() {
                    p { class: "font-main", "{x}, {y}" }
                }
                div { class: "flex w-full relative",
                    canvas { class: "w-full", id: "canvas-minimap" }
                    canvas {
                        id: "canvas-minimap-magnifier",
                        class: "absolute hidden",
                    }
                }
                if minimap().is_some() {
                    OneButton {
                        on_ok: move || async move {
                            reset(());
                            redetect_minimap(false).await;
                        },
                        "Redetect"
                    }
                    {
                        let value = halted();
                        let name = if value { "Start actions" } else { "Stop actions" };
                        rsx! {
                            OneButton {
                                on_ok: move || async move {
                                    halted.set(!value);
                                    rotate_actions(!value).await;
                                },
                                {name}
                            }
                        }
                    }
                    OneButton {
                        on_ok: move || async move {
                            reset(());
                            redetect_minimap(true).await;
                        },
                        "Delete map (for redetecting)"
                    }
                    Configuration {}
                    Divider {}
                    TextInput {
                        label: "Preset name",
                        on_input: move |value| {
                            editing_preset.set(value);
                        },
                        value: editing_preset(),
                    }
                    OneButton {
                        on_ok: move || {
                            let name = editing_preset.peek().to_owned();
                            if !name.is_empty() {
                                let _ = minimap
                                    .write()
                                    .as_mut()
                                    .unwrap()
                                    .actions
                                    .try_insert(name.clone(), vec![]);
                                preset.set(Some(name));
                                editing.set(None);
                            }
                        },
                        "Create preset"
                    }
                }
            }
            if preset().is_some() {
                div { class: "grid grid-flow-row auto-rows-max gap-[8px] w-[350px] place-items-center",
                    if let Some(presets) = presets() {
                        Options {
                            label: "Presets",
                            options: presets,
                            on_select: move |v| {
                                preset.set(Some(v));
                                editing.set(None);
                            },
                            selected: preset.peek().clone().unwrap(),
                        }
                    }
                    if let Some(index) = editing() {
                        p { class: "font-main", "Editing {index}" }
                    }
                    Actions {
                        label: "Action",
                        on_option: move |action| {
                            editing_action_set(action);
                        },
                        selected: editing_action(),
                    }
                    match editing_action() {
                        Action::Move { .. } => {
                            rsx! {
                                ActionMoveEdit {
                                    on_submit: move |action| {
                                        editing_action_set(action);
                                    },
                                    value: editing_action(),
                                }
                            }
                        }
                        Action::Key { .. } => {
                            rsx! {
                                ActionKeyEdit {
                                    on_submit: move |action| {
                                        editing_action_set(action);
                                    },
                                    value: editing_action(),
                                }
                            }
                        }
                    }
                    if let Some(i) = editing() {
                        OneButton {
                            on_ok: move || {
                                editing.set(None);
                                minimap
                                    .write()
                                    .as_mut()
                                    .unwrap()
                                    .actions
                                    .get_mut(preset.peek().as_ref().unwrap())
                                    .unwrap()
                                    .remove(i);
                            },
                            "Delete"
                        }
                        TwoButtons {
                            on_ok: move || {
                                editing.set(None);
                                *minimap
                                    .write()
                                    .as_mut()
                                    .unwrap()
                                    .actions
                                    .get_mut(preset.peek().as_ref().unwrap())
                                    .unwrap()
                                    .get_mut(i)
                                    .unwrap() = *editing_action.peek();
                            },
                            ok_body: rsx! { "Save" },
                            on_cancel: move || {
                                editing.set(None);
                            },
                            cancel_body: rsx! { "Cancel" },
                        }
                    } else {
                        Options {
                            label: "Insert position",
                            options: preset_insert_positions(),
                            on_select: move |pos| {
                                editing_insert_position.set(pos);
                            },
                            selected: editing_insert_position(),
                        }
                        OneButton {
                            on_ok: move || {
                                minimap
                                    .write()
                                    .as_mut()
                                    .unwrap()
                                    .actions
                                    .get_mut(preset.peek().as_ref().unwrap())
                                    .unwrap()
                                    .insert(*editing_insert_position.peek(), *editing_action.peek());
                            },
                            "Add action"
                        }
                    }
                }
            }
            if let Some(preset) = preset() {
                if let Some(minimap) = minimap().as_ref() {
                    div { class: "grid grid-flow-row auto-rows-max gap-[8px] w-[350px] place-items-center",
                        {
                            let actions = minimap.actions.get(&preset).unwrap().clone();
                            rsx! {
                                if !actions.is_empty() {
                                    p { class: "font-main", "Click action to edit" }
                                }
                                for (i , action) in actions.into_iter().enumerate() {
                                    div {
                                        class: "w-fit h-fit border border-black font-main",
                                        onclick: move |_| {
                                            editing.set(Some(i));
                                            editing_action.set(action);
                                        },
                                        "{action:?}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct ActionEditProps<T: 'static + PartialEq + Clone> {
    on_submit: EventHandler<T>,
    value: T,
}

#[component]
fn PositionEdit(props: ActionEditProps<Position>) -> Element {
    let Position {
        x,
        y,
        allow_adjusting,
    } = props.value;
    let submit = use_callback(move |position: Position| (props.on_submit)(position));
    let set_x = use_callback(move |x| submit(Position { x, ..props.value }));
    let set_y = use_callback(move |y| submit(Position { y, ..props.value }));
    let set_allow_adjusting = use_callback(move |allow_adjusting| {
        submit(Position {
            allow_adjusting,
            ..props.value
        })
    });

    rsx! {
        NumberInput {
            label: "x",
            on_input: move |value| {
                set_x(value);
            },
            value: x,
        }
        NumberInput {
            label: "y",
            on_input: move |value| {
                set_y(value);
            },
            value: y,
        }
        Checkbox {
            label: "Allow adjusting position",
            on_input: move |value| {
                set_allow_adjusting(value);
            },
            value: allow_adjusting,
        }
    }
}

#[component]
fn ActionMoveEdit(props: ActionEditProps<Action>) -> Element {
    let Action::Move(value) = props.value else {
        unreachable!()
    };
    let ActionMove {
        position,
        condition,
        wait_after_move_ticks,
    } = value;
    let submit =
        use_callback(move |action_move: ActionMove| (props.on_submit)(Action::Move(action_move)));
    let set_position = use_callback(move |position| submit(ActionMove { position, ..value }));
    let set_condition = use_callback(move |condition| submit(ActionMove { condition, ..value }));
    let set_wait_after_move_ticks = use_callback(move |wait_after_move_ticks| {
        submit(ActionMove {
            wait_after_move_ticks,
            ..value
        })
    });

    rsx! {
        PositionEdit {
            on_submit: move |position| {
                set_position(position);
            },
            value: position,
        }
        ActionConditions {
            label: "Condition",
            on_option: move |condition| {
                set_condition(condition);
            },
            selected: condition,
        }
        if let ActionCondition::EveryMillis(millis) = condition {
            NumberInput {
                label: "Every millis",
                on_input: move |millis| {
                    set_condition(ActionCondition::EveryMillis(millis as u64));
                },
                value: millis as i32,
            }
        }
        NumberInput {
            label: "Wait for ticks after move",
            on_input: move |wait_after_move_ticks| {
                set_wait_after_move_ticks(wait_after_move_ticks as u32);
            },
            value: wait_after_move_ticks as i32,
        }
    }
}

#[component]
fn ActionKeyEdit(props: ActionEditProps<Action>) -> Element {
    let Action::Key(value) = props.value else {
        unreachable!()
    };
    let ActionKey {
        key,
        position,
        condition,
        direction,
        with,
        wait_before_use_ticks,
        wait_after_use_ticks,
    } = value;
    let submit =
        use_callback(move |action_key: ActionKey| (props.on_submit)(Action::Key(action_key)));
    let set_key = use_callback(move |key| submit(ActionKey { key, ..value }));
    let set_position = use_callback(move |position| submit(ActionKey { position, ..value }));
    let set_condition = use_callback(move |condition| submit(ActionKey { condition, ..value }));
    let set_direction = use_callback(move |direction| submit(ActionKey { direction, ..value }));
    let set_with = use_callback(move |with| submit(ActionKey { with, ..value }));
    let set_wait_before_use_ticks = use_callback(move |wait_before_use_ticks| {
        submit(ActionKey {
            wait_before_use_ticks,
            ..value
        })
    });
    let set_wait_after_use_ticks = use_callback(move |wait_after_use_ticks| {
        submit(ActionKey {
            wait_after_use_ticks,
            ..value
        })
    });

    rsx! {
        Checkbox {
            label: "Has position",
            on_input: move |checked: bool| {
                set_position(checked.then_some(DEFAULT_POSITION));
            },
            value: position.is_some(),
        }
        if let Some(pos) = position {
            PositionEdit {
                on_submit: move |position| {
                    set_position(Some(position));
                },
                value: pos,
            }
        }
        KeyBindings {
            label: "Key binding",
            on_option: move |key| {
                set_key(key);
            },
            selected: key,
        }
        ActionConditions {
            label: "Condition",
            on_option: move |condition| {
                set_condition(condition);
            },
            selected: condition,
        }
        if let ActionCondition::EveryMillis(millis) = condition {
            NumberInput {
                label: "Condition every millis",
                on_input: move |millis| {
                    set_condition(ActionCondition::EveryMillis(millis as u64));
                },
                value: millis as i32,
            }
        }
        ActionKeyDirections {
            label: "Direction",
            on_option: move |direction| {
                set_direction(direction);
            },
            selected: direction,
        }
        ActionKeyWiths {
            label: "Use with",
            on_option: move |with| {
                set_with(with);
            },
            selected: with,
        }
        NumberInput {
            label: "Wait for ticks before use",
            on_input: move |ticks| {
                set_wait_before_use_ticks(ticks as u32);
            },
            value: wait_before_use_ticks as i32,
        }
        NumberInput {
            label: "Wait for ticks after use",
            on_input: move |ticks| {
                set_wait_after_use_ticks(ticks as u32);
            },
            value: wait_after_use_ticks as i32,
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct ActionConfigProps<T: 'static + Copy + Clone + PartialEq> {
    label: String,
    on_option: EventHandler<T>,
    selected: T,
}

#[component]
fn Actions(props: ActionConfigProps<Action>) -> Element {
    let map_default = |action| match action {
        ActionDiscriminants::Move => DEFAULT_MOVE_ACTION,
        ActionDiscriminants::Key => DEFAULT_KEY_ACTION,
    };
    let options = ActionDiscriminants::iter()
        .map(|condition| (condition, condition.to_string()))
        .collect::<Vec<_>>();
    let selected = ActionDiscriminants::from(props.selected);
    rsx! {
        Options {
            label: props.label,
            options,
            on_select: move |action| {
                (props.on_option)(map_default(action));
            },
            selected,
        }
    }
}

#[component]
fn ActionConditions(props: ActionConfigProps<ActionCondition>) -> Element {
    let map_default = |condition| match condition {
        ActionConditionDiscriminants::Any => ActionCondition::Any,
        ActionConditionDiscriminants::EveryMillis => ActionCondition::EveryMillis(0),
        ActionConditionDiscriminants::ErdaShowerOffCooldown => {
            ActionCondition::ErdaShowerOffCooldown
        }
    };
    let options = ActionConditionDiscriminants::iter()
        .map(|condition| (condition, condition.to_string()))
        .collect::<Vec<_>>();
    let selected = ActionConditionDiscriminants::from(props.selected);

    rsx! {
        Options {
            label: props.label,
            options,
            on_select: move |condition| {
                (props.on_option)(map_default(condition));
            },
            selected,
        }
    }
}

#[component]
fn ActionKeyDirections(props: ActionConfigProps<ActionKeyDirection>) -> Element {
    let options = ActionKeyDirectionDiscriminants::iter()
        .map(|disc| (disc, disc.to_string()))
        .collect::<Vec<_>>();
    let selected = ActionKeyDirectionDiscriminants::from(props.selected);

    rsx! {
        Options {
            label: props.label,
            options,
            on_select: move |disc: ActionKeyDirectionDiscriminants| {
                (props.on_option)(ActionKeyDirection::from_str(&disc.to_string()).unwrap());
            },
            selected,
        }
    }
}

#[component]
fn ActionKeyWiths(props: ActionConfigProps<ActionKeyWith>) -> Element {
    let options = ActionKeyWithDiscriminants::iter()
        .map(|disc| (disc, disc.to_string()))
        .collect::<Vec<_>>();
    let selected = ActionKeyWithDiscriminants::from(props.selected);

    rsx! {
        Options {
            label: props.label,
            options,
            on_select: move |disc: ActionKeyWithDiscriminants| {
                (props.on_option)(ActionKeyWith::from_str(&disc.to_string()).unwrap());
            },
            selected,
        }
    }
}

#[component]
fn KeyBindings(props: ActionConfigProps<KeyBinding>) -> Element {
    let options = KeyBindingDiscriminants::iter()
        .map(|disc| (disc, disc.to_string()))
        .collect::<Vec<_>>();
    let selected = KeyBindingDiscriminants::from(props.selected);

    rsx! {
        Options {
            label: props.label,
            options,
            on_select: move |disc: KeyBindingDiscriminants| {
                (props.on_option)(KeyBinding::from_str(&disc.to_string()).unwrap());
            },
            selected,
        }
    }
}

#[component]
fn RotationModes(props: ActionConfigProps<RotationMode>) -> Element {
    let options = RotationModeDiscriminants::iter()
        .map(|disc| (disc, disc.to_string()))
        .collect::<Vec<_>>();
    let selected = RotationModeDiscriminants::from(props.selected);

    rsx! {
        Options {
            label: props.label,
            options,
            on_select: move |disc: RotationModeDiscriminants| {
                (props.on_option)(RotationMode::from_str(&disc.to_string()).unwrap());
            },
            selected,
        }
    }
}

#[component]
fn Configuration() -> Element {
    let mut config = use_signal(|| query_config().unwrap());
    let interact_key = use_memo(move || config().interact_key);
    let ropelift_key = use_memo(move || config().ropelift_key);
    let up_jump_key = use_memo(move || config().up_jump_key);
    let feed_pet_key = use_memo(move || config().feed_pet_key);
    let potion_key = use_memo(move || config().potion_key);
    let rotation_mode = use_memo(move || config().rotation_mode);
    let exp_x3_key = use_memo(move || config().exp_x3_key);
    let legion_wealth_key = use_memo(move || config().legion_wealth_key);
    let legion_luck_key = use_memo(move || config().legion_luck_key);
    let sayram_elixir_key = use_memo(move || config().sayram_elixir_key);

    use_effect(move || {
        upsert_config(&mut config()).unwrap();
        spawn(async move {
            refresh_configuration().await;
        });
    });

    rsx! {
        KeyBindings {
            label: "Interact key",
            on_option: move |key| {
                config.write().interact_key = key;
            },
            selected: interact_key(),
        }
        Checkbox {
            label: "Has up jump key (Hero, Corsair, Blaster,...)",
            on_input: move |checked: bool| {
                config.write().up_jump_key = checked.then_some(KeyBinding::default());
            },
            value: up_jump_key().is_some(),
        }
        if let Some(key) = up_jump_key() {
            KeyBindings {
                label: "Up jump key",
                on_option: move |key| {
                    config.write().up_jump_key = Some(key);
                },
                selected: key,
            }
        }
        KeyBindings {
            label: "Rope lift key",
            on_option: move |key| {
                config.write().ropelift_key = key;
            },
            selected: ropelift_key(),
        }
        KeyBindings {
            label: "Feed pet key",
            on_option: move |key| {
                config.write().feed_pet_key = key;
            },
            selected: feed_pet_key(),
        }
        KeyBindings {
            label: "Potion key",
            on_option: move |key| {
                config.write().potion_key = key;
            },
            selected: potion_key(),
        }
        RotationModes {
            label: "Rotation mode",
            on_option: move |mode| {
                config.write().rotation_mode = mode;
            },
            selected: rotation_mode(),
        }
        Checkbox {
            label: "Has sayram's elixir",
            on_input: move |checked: bool| {
                config.write().sayram_elixir_key = checked.then_some(KeyBinding::default());
            },
            value: sayram_elixir_key().is_some(),
        }
        if let Some(key) = sayram_elixir_key() {
            KeyBindings {
                label: "Sayram's elixir key",
                on_option: move |key| {
                    config.write().sayram_elixir_key = Some(key);
                },
                selected: key,
            }
        }
        Checkbox {
            label: "Has x3 exp coupon",
            on_input: move |checked: bool| {
                config.write().exp_x3_key = checked.then_some(KeyBinding::default());
            },
            value: exp_x3_key().is_some(),
        }
        if let Some(key) = exp_x3_key() {
            KeyBindings {
                label: "x3 exp coupon key",
                on_option: move |key| {
                    config.write().exp_x3_key = Some(key);
                },
                selected: key,
            }
        }
        Checkbox {
            label: "Has legion wealth",
            on_input: move |checked: bool| {
                config.write().legion_wealth_key = checked.then_some(KeyBinding::default());
            },
            value: legion_wealth_key().is_some(),
        }
        if let Some(key) = legion_wealth_key() {
            KeyBindings {
                label: "Legion wealth key",
                on_option: move |key| {
                    config.write().legion_wealth_key = Some(key);
                },
                selected: key,
            }
        }
        Checkbox {
            label: "Has legion luck",
            on_input: move |checked: bool| {
                config.write().legion_luck_key = checked.then_some(KeyBinding::default());
            },
            value: legion_luck_key().is_some(),
        }
        if let Some(key) = legion_luck_key() {
            KeyBindings {
                label: "Legion luck key",
                on_option: move |key| {
                    config.write().legion_luck_key = Some(key);
                },
                selected: key,
            }
        }
    }
}
