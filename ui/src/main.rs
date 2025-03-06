#![feature(variant_count)]
#![feature(map_try_insert)]

use std::fmt::Display;
use std::ops::DerefMut;
use std::str::FromStr;
use std::string::ToString;

use backend::{
    Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove,
    Configuration as ConfigurationData, IntoEnumIterator, Minimap as MinimapData, ParseError,
    Position, start_update_loop, upsert_map,
};
use configuration::Configuration;
use dioxus::{
    desktop::{
        WindowBuilder,
        tao::platform::windows::WindowBuilderExtWindows,
        wry::dpi::{PhysicalSize, Size},
    },
    prelude::*,
};
use icons::XMark;
use input::{Checkbox, KeyBindingInput, NumberInputI32, NumberInputU32, NumberInputU64};
use minimap::Minimap;
use select::{Select, TextSelect};
use tokio::task::spawn_blocking;
use tracing_log::LogTracer;

mod configuration;
mod icons;
mod input;
mod key;
mod minimap;
mod select;

const DIV_CLASS: &str = "flex h-6 items-center space-x-2";
const LABEL_CLASS: &str = "flex-1 text-xs text-gray-700 inline-block data-[disabled]:text-gray-400";
const INPUT_CLASS: &str = "w-22 h-full border border-gray-300 rounded text-xs text-ellipsis outline-none disabled:text-gray-400 disabled:cursor-not-allowed";
const TAILWIND_CSS: Asset = asset!("public/tailwind.css");

fn main() {
    LogTracer::init().unwrap();
    start_update_loop();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(448, 820)))
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

    let minimap = use_signal::<Option<MinimapData>>(|| None);
    let preset = use_signal::<Option<String>>(|| None);
    let configs = use_signal_sync(Vec::<ConfigurationData>::new);
    let config = use_signal_sync::<Option<ConfigurationData>>(|| None);
    let mut active_tab = use_signal(|| TAB_CONFIGURATION.to_string());

    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div { class: "flex flex-col w-md h-screen space-y-2",
            Minimap { minimap, preset }
            Tab {
                tabs: vec![TAB_CONFIGURATION.to_string(), TAB_ACTIONS.to_string()],
                on_tab: move |tab| {
                    active_tab.set(tab);
                },
                tab: active_tab(),
            }
            div { class: "px-4 pb-4 pt-2 overflow-y-auto scrollbar h-full",
                match active_tab().as_str() {
                    TAB_CONFIGURATION => rsx! {
                        Configuration { configs, config }
                    },
                    TAB_ACTIONS => rsx! {
                        ActionInput { minimap, preset }
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct TabProps {
    tabs: Vec<String>,
    on_tab: EventHandler<String>,
    tab: String,
}

#[component]
fn Tab(TabProps { tabs, on_tab, tab }: TabProps) -> Element {
    rsx! {
        div { class: "flex",
            for t in tabs {
                button {
                    class: {
                        let class = if t == tab {
                            "bg-white text-gray-800"
                        } else {
                            "hover:text-gray-700 text-gray-400 bg-gray-100"
                        };
                        format!("{class} py-2 px-4 font-medium text-sm focus:outline-none")
                    },
                    onclick: move |_| {
                        on_tab(t.clone());
                    },
                    {t.clone()}
                }
            }
        }
    }
}

#[component]
fn ActionItemList(
    actions: Vec<Action>,
    on_click: EventHandler<(Action, usize)>,
    on_remove: EventHandler<usize>,
    on_swap: EventHandler<(usize, usize)>,
) -> Element {
    let mut drag_index = use_signal(|| None);

    rsx! {
        div { class: "flex-1 flex flex-col space-y-1 pl-1 overflow-y-auto scrollbar rounded border-l border-gray-300",
            if actions.is_empty() {
                div { class: "flex items-center justify-center text-sm text-gray-500 h-full",
                    "No actions"
                }
            } else {
                for (i , action) in actions.into_iter().enumerate() {
                    ActionItem {
                        index: i,
                        action,
                        on_click: move |_| {
                            on_click((action, i));
                        },
                        on_remove: move |_| {
                            on_remove(i);
                        },
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
}

#[component]
fn ActionItem(
    index: usize,
    action: Action,
    on_click: EventHandler<()>,
    on_remove: EventHandler<()>,
    on_drag: EventHandler<usize>,
    on_drop: EventHandler<usize>,
) -> Element {
    const KEY: &str = "font-medium w-1/2 text-xs";
    const VALUE: &str = "font-mono text-xs w-16 overflow-hidden text-ellipsis";
    const DIV: &str = "flex items-center space-x-1";

    let border_color = match action {
        Action::Move(_) => "border-blue-300",
        Action::Key(_) => "border-gray-300",
    };

    rsx! {
        div {
            class: "relative p-1 bg-white rounded shadow-sm cursor-move border-l-1 {border_color}",
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
            onclick: move |_| {
                on_click(());
            },
            div { class: "text-xs text-gray-700 space-y-2",
                match action {
                    Action::Move(
                        ActionMove {
                            position: Position { x, y, allow_adjusting },
                            condition,
                            wait_after_move_ticks,
                        },
                    ) => rsx! {
                        div { class: DIV,
                            span { class: KEY, "Position" }
                            span { class: VALUE, "{x}, {y}" }
                        }
                        div { class: DIV,
                            span { class: KEY, "Adjust" }
                            span { class: VALUE, "{allow_adjusting}" }
                        }
                        div { class: DIV,
                            span { class: KEY, "Condition" }
                            span { class: VALUE, {condition.to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "Wait after" }
                            span { class: VALUE, {wait_after_move_ticks.to_string()} }
                        }
                    },
                    Action::Key(
                        ActionKey {
                            key,
                            position,
                            condition,
                            direction,
                            with,
                            wait_before_use_ticks,
                            wait_after_use_ticks,
                        },
                    ) => rsx! {
                        if let Some(Position { x, y, allow_adjusting }) = position {
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
                            span { class: VALUE, {key.to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "Condition" }
                            span { class: VALUE, {condition.to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "Direction" }
                            span { class: VALUE, {direction.to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "With" }
                            span { class: VALUE, {with.to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "Wait before" }
                            span { class: VALUE, {wait_before_use_ticks.to_string()} }
                        }
                        div { class: DIV,
                            span { class: KEY, "Wait after" }
                            span { class: VALUE, {wait_after_use_ticks.to_string()} }
                        }
                    },
                }
            }
            div { class: "flex flex-col absolute right-3 top-1",
                button {
                    onclick: move |e| {
                        e.stop_propagation();
                        on_remove(());
                    },
                    XMark { class: "w-[10px] h-[10px] text-red-400 fill-current" }
                }
            }
        }
    }
}

#[component]
fn ActionInput(minimap: Signal<Option<MinimapData>>, preset: Signal<Option<String>>) -> Element {
    let mut editing_action = use_signal::<Option<(Action, usize)>>(|| None);
    let mut value_action = use_signal(|| Action::Move(ActionMove::default()));

    let save_minimap = move |mut minimap: MinimapData| {
        spawn(async move {
            spawn_blocking(move || {
                upsert_map(&mut minimap).unwrap();
            })
            .await
            .unwrap();
        });
    };
    let presets = use_memo::<Vec<String>>(move || {
        minimap()
            .map(|minimap| minimap.actions.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let actions = use_memo::<Vec<Action>>(move || {
        minimap()
            .zip(preset())
            .and_then(|(minimap, preset)| minimap.actions.get(&preset).cloned())
            .unwrap_or_default()
    });
    let on_edit = use_callback(move |action| {
        value_action.set(action);
    });
    let on_save = use_callback(move |index| {
        if let Some((minimap, preset)) = minimap.write().as_mut().zip(preset.peek().clone()) {
            let actions = minimap.actions.get_mut(&preset).unwrap();
            if let Some(index) = index {
                *actions.get_mut(index).unwrap() = *value_action.peek();
            } else {
                actions.push(*value_action.peek());
            }
            save_minimap(minimap.clone());
        }
    });
    let on_remove = use_callback(move |index| {
        if let Some((minimap, preset)) = minimap.write().as_mut().zip(preset.peek().clone()) {
            minimap.actions.get_mut(&preset).unwrap().remove(index);
            save_minimap(minimap.clone());
        }
    });
    let on_swap = use_callback(move |(a, b)| {
        if let Some((minimap, preset)) = minimap.write().as_mut().zip(preset.peek().clone()) {
            minimap.actions.get_mut(&preset).unwrap().swap(a, b);
            save_minimap(minimap.clone());
        }
    });

    use_effect(move || {
        if preset().is_none() {
            editing_action.set(None);
        }
    });

    rsx! {
        div { class: "flex flex-col h-full",
            TextSelect {
                on_create: move |created: String| {
                    if let Some(minimap) = minimap.write().deref_mut() {
                        let actions_inserted = minimap
                            .actions
                            .try_insert(created.clone(), vec![])
                            .is_ok();
                        if actions_inserted {
                            save_minimap(minimap.clone());
                        }
                        preset.set(Some(created));
                    }
                },
                disabled: minimap().is_none(),
                on_select: move |selected| {
                    preset.set(Some(selected));
                },
                options: presets(),
                selected: preset(),
            }
            div { class: "flex space-x-4 overflow-y-auto flex-1",
                div { class: "w-1/2 flex flex-col space-y-3",
                    ActionTypeInput {
                        on_input: move |action: Action| {
                            if let Some((editing_action, _)) = *editing_action.peek() {
                                if editing_action.to_string() == action.to_string() {
                                    on_edit(editing_action);
                                    return;
                                }
                            }
                            on_edit(action);
                        },
                        disabled: preset().is_none(),
                        value: value_action(),
                    }
                    match value_action() {
                        Action::Move(_) => rsx! {
                            ActionMoveInput {
                                on_input: move |action| {
                                    on_edit(action);
                                },
                                disabled: preset().is_none(),
                                value: value_action(),
                            }
                        },
                        Action::Key(_) => rsx! {
                            ActionKeyInput {
                                on_input: move |action| {
                                    on_edit(action);
                                },
                                disabled: preset().is_none(),
                                value: value_action(),
                            }
                        },
                    }
                    if editing_action().is_none() {
                        button {
                            class: "w-full button-primary h-6",
                            disabled: preset().is_none(),
                            onclick: move |_| {
                                on_save(None);
                            },
                            "Add action"
                        }
                    } else {
                        div { class: "grid grid-cols-2 gap-x-2",
                            button {
                                class: "button-primary h-6",
                                onclick: move |_| {
                                    on_save(editing_action.replace(None).map(|tuple| tuple.1));
                                },
                                "Save"
                            }
                            button {
                                class: "button-secondary h-6",
                                onclick: move |_| {
                                    editing_action.set(None);
                                },
                                "Cancel"
                            }
                        }
                    }
                }
                ActionItemList {
                    actions: actions(),
                    on_click: move |(action, index)| {
                        editing_action.set(Some((action, index)));
                        on_edit(action);
                    },
                    on_remove: move |index| {
                        let editing = *editing_action.peek();
                        if let Some((_, i)) = editing {
                            if index == i {
                                editing_action.set(None);
                            }
                        }
                        on_remove(index);
                    },
                    on_swap: move |(a, b)| {
                        let editing = *editing_action.peek();
                        if let Some((action, index)) = editing {
                            if index == a {
                                editing_action.set(Some((action, b)));
                            } else if index == b {
                                editing_action.set(Some((action, a)));
                            }
                        }
                        on_swap((a, b));
                    },
                }
            }
        }
    }
}

#[derive(Clone, Copy, Props, PartialEq)]
struct InputConfigProps<T: 'static + Clone + PartialEq> {
    on_input: EventHandler<T>,
    disabled: bool,
    value: T,
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

    rsx! {
        NumberInputI32 {
            label: "X",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: "{INPUT_CLASS} p-1",
            disabled: props.disabled,
            on_input: move |value| {
                set_x(value);
            },
            value: x,
        }
        NumberInputI32 {
            label: "Y",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: "{INPUT_CLASS} p-1",
            disabled: props.disabled,
            on_input: move |value| {
                set_y(value);
            },
            value: y,
        }
        Checkbox {
            label: "Adjust position",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: "appearance-none h-4 w-4 border border-gray-300 rounded checked:bg-gray-400",
            disabled: props.disabled,
            on_input: move |checked| {
                set_allow_adjusting(checked);
            },
            value: allow_adjusting,
        }
    }
}

#[component]
fn ActionMoveInput(props: InputConfigProps<Action>) -> Element {
    let Action::Move(value) = props.value else {
        unreachable!()
    };
    let ActionMove {
        position,
        condition,
        wait_after_move_ticks,
    } = value;
    let submit =
        use_callback(move |action_move: ActionMove| (props.on_input)(Action::Move(action_move)));
    let set_position = use_callback(move |position| submit(ActionMove { position, ..value }));
    let set_condition = use_callback(move |condition| submit(ActionMove { condition, ..value }));
    let set_wait_after_move_ticks = use_callback(move |wait_after_move_ticks| {
        submit(ActionMove {
            wait_after_move_ticks,
            ..value
        })
    });

    rsx! {
        div { class: "flex flex-col space-y-3",
            PositionInput {
                on_input: move |position| {
                    set_position(position);
                },
                disabled: props.disabled,
                value: position,
            }
            ActionConditionInput {
                on_input: move |condition| {
                    set_condition(condition);
                },
                disabled: props.disabled,
                value: condition,
            }
            NumberInputU32 {
                label: "Wait after action",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                disabled: props.disabled,
                on_input: move |value| {
                    set_wait_after_move_ticks(value);
                },
                value: wait_after_move_ticks,
            }
        }
    }
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

    rsx! {
        div { class: "flex flex-col space-y-3",
            Checkbox {
                label: "Position",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "appearance-none h-4 w-4 border border-gray-300 rounded checked:bg-gray-400",
                disabled: props.disabled,
                on_input: move |checked: bool| {
                    set_position(checked.then_some(Position::default()));
                },
                value: position.is_some(),
            }
            if let Some(pos) = position {
                PositionInput {
                    on_input: move |position| {
                        set_position(Some(position));
                    },
                    disabled: props.disabled,
                    value: pos,
                }
            }
            KeyBindingInput {
                label: "Key",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: INPUT_CLASS,
                disabled: props.disabled,
                on_input: move |key| {
                    set_key(key);
                },
                value: key,
            }
            ActionConditionInput {
                on_input: move |condition| {
                    set_condition(condition);
                },
                disabled: props.disabled,
                value: condition,
            }
            ActionKeyDirectionInput {
                on_input: move |direction| {
                    set_direction(direction);
                },
                disabled: props.disabled,
                value: direction,
            }
            ActionKeyWithInput {
                on_input: move |with| {
                    set_with(with);
                },
                disabled: props.disabled,
                value: with,
            }
            NumberInputU32 {
                label: "Wait before action",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                on_input: move |value| {
                    set_wait_before_use_ticks(value);
                },
                disabled: props.disabled,
                value: wait_before_use_ticks,
            }
            NumberInputU32 {
                label: "Wait after action",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                on_input: move |value| {
                    set_wait_after_use_ticks(value);
                },
                disabled: props.disabled,
                value: wait_after_use_ticks,
            }
        }
    }
}

#[component]
fn ActionTypeInput(
    InputConfigProps {
        on_input,
        disabled,
        value,
    }: InputConfigProps<Action>,
) -> Element {
    rsx! {
        ActionEnumInput {
            label: "Type",
            on_input,
            disabled,
            value,
        }
    }
}

#[component]
fn ActionKeyDirectionInput(
    InputConfigProps {
        on_input,
        disabled,
        value,
    }: InputConfigProps<ActionKeyDirection>,
) -> Element {
    rsx! {
        ActionEnumInput {
            label: "Direction",
            on_input,
            disabled,
            value,
        }
    }
}

#[component]
fn ActionKeyWithInput(
    InputConfigProps {
        on_input,
        disabled,
        value,
    }: InputConfigProps<ActionKeyWith>,
) -> Element {
    rsx! {
        ActionEnumInput {
            label: "With",
            on_input,
            disabled,
            value,
        }
    }
}

#[component]
fn ActionConditionInput(
    InputConfigProps {
        on_input,
        disabled,
        value,
    }: InputConfigProps<ActionCondition>,
) -> Element {
    rsx! {
        ActionEnumInput {
            label: "Condition",
            on_input,
            disabled,
            value,
        }
        if let ActionCondition::EveryMillis(millis) = value {
            NumberInputU64 {
                label: "Milliseconds",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                on_input: move |millis| {
                    on_input(ActionCondition::EveryMillis(millis));
                },
                value: millis,
            }
        }
    }
}

#[component]
fn ActionEnumInput<
    T: 'static + Clone + Copy + PartialEq + Display + FromStr<Err = ParseError> + IntoEnumIterator,
>(
    label: String,
    disabled: bool,
    on_input: EventHandler<T>,
    value: T,
) -> Element {
    let options = T::iter()
        .map(|variant| (variant.to_string(), variant.to_string()))
        .collect::<Vec<_>>();
    let selected = value.to_string();

    rsx! {
        Select {
            label,
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            select_class: INPUT_CLASS,
            disabled,
            options,
            on_select: move |selected: String| {
                on_input(T::from_str(selected.as_str()).unwrap());
            },
            selected,
        }
    }
}
