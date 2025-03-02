#![feature(variant_count)]
#![feature(map_try_insert)]

use std::ops::DerefMut;
use std::str::FromStr;
use std::string::ToString;

use backend::{
    Action, ActionCondition, ActionConditionDiscriminants, ActionDiscriminants, ActionKey,
    ActionKeyDirection, ActionKeyDirectionDiscriminants, ActionKeyWith, ActionKeyWithDiscriminants,
    ActionMove, IntoEnumIterator, KeyBinding, KeyBindingConfiguration, KeyBindingDiscriminants,
    Minimap as MinimapData, Position, RotationMode, RotationModeDiscriminants, minimap_data,
    minimap_frame, player_position, prepare_actions, query_config, redetect_minimap,
    refresh_configuration, refresh_minimap_data, rotate_actions, start_update_loop, upsert_config,
    upsert_map,
};
use checkbox::Checkbox;
use configuration::Configuration;
use dioxus::{
    desktop::{
        WindowBuilder,
        tao::platform::windows::WindowBuilderExtWindows,
        wry::dpi::{PhysicalSize, Size},
    },
    document::EvalError,
    prelude::*,
};
use key::{KeyBindingInput, KeyInput};
use option::Option;
use tab::Tab;
use tracing_log::LogTracer;

mod checkbox;
mod configuration;
mod key;
mod option;
mod tab;

const DIV_CLASS: &str = "flex h-6 items-center space-x-2";
const LABEL_CLASS: &str = "w-20 text-xs text-gray-700 inline-block";
const INPUT_CLASS: &str =
    "w-22 h-full border border-gray-300 rounded text-xs text-ellipsis outline-none";
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
    position: Some(DEFAULT_POSITION),
    condition: ActionCondition::ErdaShowerOffCooldown,
    direction: ActionKeyDirection::Any,
    with: ActionKeyWith::Any,
    wait_before_use_ticks: 0,
    wait_after_use_ticks: 0,
});

fn main() {
    LogTracer::init().unwrap();
    start_update_loop();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(448, 800)))
        .with_resizable(false)
        .with_drag_and_drop(false)
        .with_maximizable(false)
        .with_title("Maple Bot");
    let cfg = dioxus::desktop::Config::default()
        .with_menu(None)
        .with_window(window);
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    const TAB_CONFIGURATION: &str = "Configuration";
    const TAB_ACTIONS: &str = "Actions";

    let mut action_key = use_signal(|| DEFAULT_KEY_ACTION);
    let mut active_tab = use_signal(|| TAB_CONFIGURATION.to_string());

    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "flex flex-col w-md h-screen space-y-4",
            Minimap {}
            Tab {
                tabs: vec![TAB_CONFIGURATION.to_string(), TAB_ACTIONS.to_string()],
                on_tab: move |tab| {
                    active_tab.set(tab);
                },
                tab: active_tab(),
            }
            match active_tab().as_str() {
                TAB_CONFIGURATION => rsx! {
                    div { class: "p-4 overflow-y-auto scrollbar", Configuration {} }
                },
                TAB_ACTIONS => rsx! {
                    div { class: "p-4 flex space-x-4 overflow-y-auto",
                        div { class: "w-1/2 flex flex-col space-y-3",
                            ActionInput {
                                on_input: move |action| {
                                    action_key.set(action);
                                },
                                value: action_key(),
                            }
                            ActionKeyInput {
                                on_input: move |action| {
                                    action_key.set(action);
                                },
                                value: action_key(),
                            }
                        }
                        ActionItemList { actions: vec![DEFAULT_KEY_ACTION; 2], on_swap: move |(a, b)| {} }
                    }
                },
                _ => unreachable!(),
            }
        }
    }
}

#[component]
fn ActionItemList(actions: Vec<Action>, on_swap: EventHandler<(usize, usize)>) -> Element {
    let mut drag_index = use_signal(|| None);

    rsx! {
        div { class: "flex-1 flex flex-col space-y-2 overflow-y-auto scrollbar",
            for (i , action) in actions.into_iter().enumerate() {
                ActionItem {
                    index: i,
                    action,
                    on_drag: move |i| {
                        drag_index.set(Some(i));
                    },
                    on_drop: move |i| {
                        if let Some(drag_i) = drag_index.take() {
                            if drag_i != i {
                                on_swap((drag_i, i));
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn ActionItem(
    index: usize,
    action: Action,
    on_drag: EventHandler<usize>,
    on_drop: EventHandler<usize>,
) -> Element {
    const KEY: &str = "font-medium w-1/2 text-xs";
    const VALUE: &str = "font-mono text-xs w-16 overflow-hidden text-ellipsis";
    const DIV: &str = "flex items-center space-x-1";

    let border_color = match action {
        Action::Move(_) => "border-gray-500",
        Action::Key(_) => "border-gray-800",
    };

    rsx! {
        div {
            class: "p-1 bg-white rounded shadow-sm cursor-move border-l-1 {border_color}",
            draggable: true,
            ondragenter: move |e| {
                e.prevent_default();
            },
            ondragover: move |e| {
                e.prevent_default();
            },
            ondragstart: move |_| {
                on_drag(index);
            },
            ondrop: move |_| {
                on_drop(index);
            },
            match action {
                Action::Move(action_move) => todo!(),
                Action::Key(action) => rsx! {
                    div { class: "text-xs text-gray-700 space-y-2",
                        if let Some(Position { x, y, allow_adjusting }) = action.position {
                            div { class: DIV,
                                span { class: KEY, "Position" }
                                span { class: VALUE, "{x}, {y}" }
                            }
                            div { class: DIV,
                                span { class: KEY, "Adjust" }
                                span { class: VALUE, "{allow_adjusting}" }
                            }
                        }
                        div { class: DIV,
                            span { class: KEY, "Key" }
                            span { class: VALUE, {KeyBindingDiscriminants::from(action.key).to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "Condition" }
                            span { class: VALUE, {ActionConditionDiscriminants::from(action.condition).to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "Direction" }
                            span { class: VALUE, {ActionKeyDirectionDiscriminants::from(action.direction).to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "With" }
                            span { class: VALUE, {ActionKeyWithDiscriminants::from(action.with).to_string()} }
                        }
                    }
                },
            }
        }
    }
}

#[component]
fn Minimap() -> Element {
    let mut position = use_signal::<Option<(i32, i32)>>(|| None);
    let mut minimap = use_signal::<Option<MinimapData>>(|| None);
    let mut preset = use_signal::<Option<String>>(move || {
        if let Some(minimap) = &minimap() {
            minimap.actions.keys().next().cloned()
        } else {
            None
        }
    });

    use_future(move || async move {
        let mut canvas = document::eval(include_str!("js/minimap.js"));
        loop {
            let result = minimap_frame().await;
            let Ok(frame) = result else {
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
        div { class: "flex flex-col items-center justify-center",
            canvas {
                class: "h-36 p-3 border border-gray-300 rounded-md",
                id: "canvas-minimap",
            }
        }
    }
}

// #[component]
// fn Minimap() -> Element {
//     let mut halted = use_signal(|| true);
//     let mut position = use_signal::<Option<(i32, i32)>>(|| None);
//     let mut minimap = use_signal::<Option<MinimapData>>(|| None);
//     let mut preset = use_signal::<Option<String>>(move || {
//         if let Some(minimap) = &minimap() {
//             minimap.actions.keys().next().cloned()
//         } else {
//             None
//         }
//     });
//     let preset_insert_positions = use_memo::<Vec<(usize, String)>>(move || {
//         if let Some(minimap) = &minimap() {
//             if let Some(preset) = &preset() {
//                 let mut vec = minimap
//                     .actions
//                     .get(preset)
//                     .unwrap_or(&Vec::new())
//                     .iter()
//                     .enumerate()
//                     .map(|(i, _)| (i, i.to_string()))
//                     .collect::<Vec<(usize, String)>>();
//                 vec.push((vec.len(), vec.len().to_string()));
//                 return vec;
//             }
//         }
//         vec![]
//     });
//     let presets = use_memo::<Option<Vec<(String, String)>>>(move || {
//         minimap().map(|minimap| {
//             minimap
//                 .actions
//                 .keys()
//                 .cloned()
//                 .map(|key| (key.clone(), key))
//                 .collect()
//         })
//     });

//     let mut editing = use_signal::<Option<usize>>(|| None);
//     let mut editing_preset = use_signal::<String>(String::new);
//     let mut editing_action = use_signal::<Action>(|| DEFAULT_MOVE_ACTION);
//     let mut editing_insert_position = use_signal::<usize>(|| 0);
//     let mut editing_action_last = use_signal::<ActionDiscriminants>(|| ActionDiscriminants::Move);
//     let editing_action_set = use_callback(move |action: Action| {
//         let action_disc = ActionDiscriminants::from(action);
//         if let Some(i) = *editing.peek() {
//             let minimap = minimap.peek();
//             let minimap = minimap.as_ref().unwrap();
//             let existing_action = minimap
//                 .actions
//                 .get(preset.peek().as_ref().unwrap())
//                 .unwrap()
//                 .get(i)
//                 .unwrap();
//             if action_disc != *editing_action_last.peek()
//                 && action_disc == ActionDiscriminants::from(existing_action)
//             {
//                 let is_default = match action {
//                     Action::Move(_) => action == DEFAULT_MOVE_ACTION,
//                     Action::Key(_) => action == DEFAULT_KEY_ACTION,
//                 };
//                 if is_default {
//                     editing_action_last.set(action_disc);
//                     editing_action.set(*existing_action);
//                     return;
//                 }
//             }
//         }
//         editing_action_last.set(action_disc);
//         editing_action.set(action);
//     });

//     let reset = use_callback(move |()| {
//         if position.peek().is_some() {
//             position.set(None);
//         }
//         if minimap.peek().is_some() {
//             minimap.set(None);
//         }
//         if preset.peek().is_some() {
//             preset.set(None);
//         }
//         if editing.peek().is_some() {
//             editing.set(None);
//         }
//     });

//     use_effect(move || {
//         let i = preset_insert_positions()
//             .last()
//             .map(|(i, _)| *i)
//             .unwrap_or(0);
//         editing_insert_position.set(i);
//     });
//     use_effect(move || {
//         if let Some(preset) = preset() {
//             spawn(async move {
//                 prepare_actions(preset).await;
//             });
//         }
//     });
//     use_effect(move || {
//         if let Some(minimap) = &mut minimap() {
//             upsert_map(minimap).unwrap();
//             if preset.peek().is_none() {
//                 preset.set(minimap.actions.keys().next().cloned());
//             }
//             spawn(async move {
//                 refresh_minimap_data().await;
//                 if let Some(preset) = preset.peek().clone() {
//                     prepare_actions(preset).await;
//                 }
//             });
//         }
//     });
//     use_future(move || async move {
//         let mut canvas = document::eval(include_str!("js/minimap.js"));
//         loop {
//             let result = minimap_frame().await;
//             let Ok(frame) = result else {
//                 reset(());
//                 continue;
//             };
//             if minimap.peek().is_none() {
//                 minimap.set(minimap_data().await.ok());
//             }
//             position.set(player_position().await.ok());
//             let Err(error) = canvas.send(frame) else {
//                 continue;
//             };
//             if matches!(error, EvalError::Finished) {
//                 // probably: https://github.com/DioxusLabs/dioxus/issues/2979
//                 canvas = document::eval(include_str!("js/minimap.js"));
//             }
//         }
//     });

//     rsx! {
//         div { class: "grid grid-cols-3 gap-x-[32px] p-[16px]",
//             div { class: "grid grid-flow-row auto-rows-max gap-[8px] w-[350px] place-items-center",
//                 p { class: "font-main",
//                     if let Some(minimap) = &minimap() {
//                         "{minimap.name}"
//                     } else {
//                         "Detecting..."
//                     }
//                 }
//                 if let Some((x, y)) = position() {
//                     p { class: "font-main", "{x}, {y}" }
//                 }
//                 div { class: "flex w-[280px] relative",
//                     canvas { class: "w-full", id: "canvas-minimap" }
//                     canvas {
//                         id: "canvas-minimap-magnifier",
//                         class: "absolute hidden",
//                     }
//                 }
//                 if minimap().is_some() {
//                     OneButton {
//                         on_ok: move || async move {
//                             reset(());
//                             redetect_minimap(false).await;
//                         },
//                         "Redetect"
//                     }
//                     {
//                         let value = halted();
//                         let name = if value { "Start actions" } else { "Stop actions" };
//                         rsx! {
//                             OneButton {
//                                 on_ok: move || async move {
//                                     halted.set(!value);
//                                     rotate_actions(!value).await;
//                                 },
//                                 {name}
//                             }
//                         }
//                     }
//                     OneButton {
//                         on_ok: move || async move {
//                             reset(());
//                             redetect_minimap(true).await;
//                         },
//                         "Delete map (for redetecting)"
//                     }
//                     OneButton {
//                         on_ok: move || {
//                             let position = *position.peek();
//                             if let Some((x, y)) = position {
//                                 match editing_action.write().deref_mut() {
//                                     Action::Move(action_move) => {
//                                         action_move.position.x = x;
//                                         action_move.position.y = y;
//                                     }
//                                     Action::Key(action_key) => {
//                                         let position = action_key.position.get_or_insert(DEFAULT_POSITION);
//                                         position.x = x;
//                                         position.y = y;
//                                     }
//                                 }
//                             }
//                         },
//                         "Copy position to action"
//                     }
//                     Configuration {}
//                 }
//             }
//             div { class: "grid grid-flow-row auto-rows-max gap-[8px] w-[350px] place-items-center",
//                 if minimap().is_some() {
//                     TextInput {
//                         label: "Preset name",
//                         on_input: move |value| {
//                             editing_preset.set(value);
//                         },
//                         value: editing_preset(),
//                     }
//                     OneButton {
//                         on_ok: move || {
//                             let name = editing_preset.peek().to_owned();
//                             if !name.is_empty() {
//                                 let _ = minimap
//                                     .write()
//                                     .as_mut()
//                                     .unwrap()
//                                     .actions
//                                     .try_insert(name.clone(), vec![]);
//                                 preset.set(Some(name));
//                                 editing.set(None);
//                             }
//                         },
//                         "Create preset"
//                     }
//                 }
//                 if preset().is_some() {
//                     Divider {}
//                     if let Some(presets) = presets() {
//                         Options {
//                             label: "Presets",
//                             options: presets,
//                             on_select: move |v| {
//                                 preset.set(Some(v));
//                                 editing.set(None);
//                             },
//                             selected: preset.peek().clone().unwrap(),
//                         }
//                     }
//                     if let Some(index) = editing() {
//                         p { class: "font-main", "Editing {index}" }
//                     }
//                     Actions {
//                         label: "Action",
//                         on_option: move |action| {
//                             editing_action_set(action);
//                         },
//                         selected: editing_action(),
//                     }
//                     match editing_action() {
//                         Action::Move { .. } => {
//                             rsx! {
//                                 ActionMoveEdit {
//                                     on_submit: move |action| {
//                                         editing_action_set(action);
//                                     },
//                                     value: editing_action(),
//                                 }
//                             }
//                         }
//                         Action::Key { .. } => {
//                             rsx! {
//                                 ActionKeyEdit {
//                                     on_submit: move |action| {
//                                         editing_action_set(action);
//                                     },
//                                     value: editing_action(),
//                                 }
//                             }
//                         }
//                     }
//                     if let Some(i) = editing() {
//                         OneButton {
//                             on_ok: move || {
//                                 editing.set(None);
//                                 minimap
//                                     .write()
//                                     .as_mut()
//                                     .unwrap()
//                                     .actions
//                                     .get_mut(preset.peek().as_ref().unwrap())
//                                     .unwrap()
//                                     .remove(i);
//                             },
//                             "Delete"
//                         }
//                         TwoButtons {
//                             on_ok: move || {
//                                 editing.set(None);
//                                 *minimap
//                                     .write()
//                                     .as_mut()
//                                     .unwrap()
//                                     .actions
//                                     .get_mut(preset.peek().as_ref().unwrap())
//                                     .unwrap()
//                                     .get_mut(i)
//                                     .unwrap() = *editing_action.peek();
//                             },
//                             ok_body: rsx! { "Save" },
//                             on_cancel: move || {
//                                 editing.set(None);
//                             },
//                             cancel_body: rsx! { "Cancel" },
//                         }
//                     } else {
//                         Options {
//                             label: "Insert position",
//                             options: preset_insert_positions(),
//                             on_select: move |pos| {
//                                 editing_insert_position.set(pos);
//                             },
//                             selected: editing_insert_position(),
//                         }
//                         OneButton {
//                             on_ok: move || {
//                                 minimap
//                                     .write()
//                                     .as_mut()
//                                     .unwrap()
//                                     .actions
//                                     .get_mut(preset.peek().as_ref().unwrap())
//                                     .unwrap()
//                                     .insert(*editing_insert_position.peek(), *editing_action.peek());
//                             },
//                             "Add action"
//                         }
//                     }
//                 }
//             }
//             if let Some(preset) = preset() {
//                 if let Some(minimap) = minimap().as_ref() {
//                     div { class: "grid grid-flow-row auto-rows-max gap-[8px] w-[350px] h-[780px] place-items-center overflow-y-scroll",
//                         {
//                             let actions = minimap.actions.get(&preset).unwrap().clone();
//                             rsx! {
//                                 if !actions.is_empty() {
//                                     p { class: "font-main", "Click action to edit" }
//                                 }
//                                 for (i , action) in actions.into_iter().enumerate() {
//                                     div {
//                                         class: "w-fit h-fit border border-black font-main",
//                                         onclick: move |_| {
//                                             editing.set(Some(i));
//                                             editing_action.set(action);
//                                         },
//                                         "{i} - {action:?}"
//                                     }
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//     }
// }

// #[derive(PartialEq, Props, Clone)]
// struct ActionEditProps<T: 'static + PartialEq + Clone> {
//     on_submit: EventHandler<T>,
//     value: T,
// }

// #[component]
// fn ActionMoveEdit(props: ActionEditProps<Action>) -> Element {
//     const LABEL_CLASS: &str = "w-16 text-xs text-gray-700 inline-block";
//     const INPUT_CLASS: &str = "h-6 p-1 border border-gray-300 rounded text-xs";

//     let Action::Move(value) = props.value else {
//         unreachable!()
//     };
//     let ActionMove {
//         position,
//         condition,
//         wait_after_move_ticks,
//     } = value;
//     let submit =
//         use_callback(move |action_move: ActionMove| (props.on_submit)(Action::Move(action_move)));
//     let set_position = use_callback(move |position| submit(ActionMove { position, ..value }));
//     let set_condition = use_callback(move |condition| submit(ActionMove { condition, ..value }));
//     let set_wait_after_move_ticks = use_callback(move |wait_after_move_ticks| {
//         submit(ActionMove {
//             wait_after_move_ticks,
//             ..value
//         })
//     });

//     rsx! {
//         div { class: "p-2 border rounded mt-2 flex flex-col space-y-2" }
//     }
// }

#[derive(Clone, Copy, Props, PartialEq)]
struct InputConfigProps<T: 'static + Clone + PartialEq> {
    on_input: EventHandler<T>,
    value: T,
}

#[component]
fn ActionKeyInput(props: InputConfigProps<Action>) -> Element {
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
        use_callback(move |action_key: ActionKey| (props.on_input)(Action::Key(action_key)));
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
    let mut is_active = use_signal(|| false);

    rsx! {
        div { class: "flex flex-col space-y-3",
            Checkbox {
                label: "Position",
                div_class: DIV_CLASS,
                label_class: LABEL_CLASS,
                input_class: "appearance-none h-4 w-4 border border-gray-300 rounded checked:bg-gray-400",
                on_checked: move |checked: bool| {
                    set_position(checked.then_some(DEFAULT_POSITION));
                },
                checked: position.is_some(),
            }
            if let Some(pos) = position {
                PositionInput {
                    on_input: move |position| {
                        set_position(Some(position));
                    },
                    value: pos,
                }
            }
            KeyBindingInput {
                div_class: DIV_CLASS,
                label_class: LABEL_CLASS,
                input_class: INPUT_CLASS,
                is_active: is_active(),
                on_active: move |active| {
                    is_active.set(active);
                },
                on_input: move |key: Option<KeyBinding>| {
                    set_key(key.unwrap());
                },
                value: key,
            }
            ActionConditionInput {
                on_input: move |condition| {
                    set_condition(condition);
                },
                value: condition,
            }
            ActionKeyDirectionInput {
                on_input: move |direction| {
                    set_direction(direction);
                },
                value: direction,
            }
            ActionKeyWithInput {
                on_input: move |with| {
                    set_with(with);
                },
                value: with,
            }
        }
    }
}

#[component]
fn ActionInput(props: InputConfigProps<Action>) -> Element {
    let map_default = |action| match action {
        ActionDiscriminants::Move => DEFAULT_MOVE_ACTION,
        ActionDiscriminants::Key => DEFAULT_KEY_ACTION,
    };
    let options = ActionDiscriminants::iter()
        .map(|condition| (condition, condition.to_string()))
        .collect::<Vec<_>>();
    let selected = ActionDiscriminants::from(props.value);
    rsx! {
        Option {
            label: "Type",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            select_class: INPUT_CLASS,
            options,
            on_select: move |action| {
                (props.on_input)(map_default(action));
            },
            selected,
        }
    }
}

#[component]
fn PositionInput(props: InputConfigProps<Position>) -> Element {
    let Position {
        x,
        y,
        allow_adjusting,
    } = props.value;
    let submit = use_callback(move |position: Position| (props.on_input)(position));
    let set_x = use_callback(move |x| submit(Position { x, ..props.value }));
    let set_y = use_callback(move |y| submit(Position { y, ..props.value }));
    let set_allow_adjusting = use_callback(move |allow_adjusting| {
        submit(Position {
            allow_adjusting,
            ..props.value
        })
    });
    let x_or_y_component = |is_x: bool| {
        rsx! {
            div { class: DIV_CLASS,
                label { class: LABEL_CLASS,
                    if is_x {
                        "X"
                    } else {
                        "Y"
                    }
                }
                input {
                    r#type: "number",
                    class: "{INPUT_CLASS} p-1",
                    min: "0",
                    onchange: move |e| {
                        let prev_value = if is_x { x } else { y };
                        let new_value = e.parsed::<i32>().unwrap_or(prev_value);
                        if is_x {
                            set_x(new_value);
                        } else {
                            set_y(new_value);
                        }
                    },
                    value: if is_x { x } else { y },
                }
            }
        }
    };
    rsx! {
        {x_or_y_component(true)}
        {x_or_y_component(false)}
        Checkbox {
            label: "Adjust position",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: "appearance-none h-4 w-4 border border-gray-300 rounded checked:bg-gray-400",
            on_checked: move |checked| {
                set_allow_adjusting(checked);
            },
            checked: allow_adjusting,
        }
    }
}

#[component]
fn ActionKeyDirectionInput(props: InputConfigProps<ActionKeyDirection>) -> Element {
    let options = ActionKeyDirectionDiscriminants::iter()
        .map(|disc| (disc, disc.to_string()))
        .collect::<Vec<_>>();
    let selected = ActionKeyDirectionDiscriminants::from(props.value);

    rsx! {
        Option {
            label: "Direction",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            select_class: INPUT_CLASS,
            options,
            on_select: move |disc: ActionKeyDirectionDiscriminants| {
                (props.on_input)(ActionKeyDirection::from_str(&disc.to_string()).unwrap());
            },
            selected,
        }
    }
}

#[component]
fn ActionKeyWithInput(props: InputConfigProps<ActionKeyWith>) -> Element {
    let options = ActionKeyWithDiscriminants::iter()
        .map(|disc| (disc, disc.to_string()))
        .collect::<Vec<_>>();
    let selected = ActionKeyWithDiscriminants::from(props.value);

    rsx! {
        Option {
            label: "With",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            select_class: INPUT_CLASS,
            options,
            on_select: move |disc: ActionKeyWithDiscriminants| {
                (props.on_input)(ActionKeyWith::from_str(&disc.to_string()).unwrap());
            },
            selected,
        }
    }
}

#[component]
fn ActionConditionInput(props: InputConfigProps<ActionCondition>) -> Element {
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
    let selected = ActionConditionDiscriminants::from(props.value);

    rsx! {
        Option {
            label: "Condition",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            select_class: INPUT_CLASS,
            options,
            on_select: move |condition| {
                (props.on_input)(map_default(condition));
            },
            selected,
        }
        if let ActionCondition::EveryMillis(millis) = props.value {
            div { class: DIV_CLASS,
                label { class: LABEL_CLASS, "Milliseconds" }
                input {
                    r#type: "number",
                    class: "{INPUT_CLASS} p-1",
                    min: "0",
                    onchange: move |e| {
                        let millis = e.parsed::<u64>().unwrap_or(millis);
                        (props.on_input)(ActionCondition::EveryMillis(millis));
                    },
                    value: millis,
                }
            }
        }
    }
}
