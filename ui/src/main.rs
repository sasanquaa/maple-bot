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
        tao::{platform::windows::WindowBuilderExtWindows, window::WindowSizeConstraints},
        wry::dpi::{PhysicalSize, PixelUnit, Size},
    },
    prelude::*,
};
use icons::{PositionIcon, XIcon};
use input::{
    Checkbox, KeyBindingInput, MillisInput, NumberInputI32, NumberInputU32, use_auto_numeric,
};
use minimap::Minimap;
use platforms::Platforms;
use rand::distr::{Alphanumeric, SampleString};
use rotation::Rotations;
use select::{EnumSelect, TextSelect};
use tokio::task::spawn_blocking;
use tracing_log::LogTracer;

mod configuration;
mod icons;
mod input;
mod key;
mod minimap;
mod platforms;
mod rotation;
mod select;

const DIV_CLASS: &str = "flex h-6 items-center space-x-2";
const LABEL_CLASS: &str = "flex-1 text-xs text-gray-700 inline-block data-[disabled]:text-gray-400";
const INPUT_CLASS: &str = "w-22 h-full border border-gray-300 rounded text-xs text-ellipsis outline-none disabled:text-gray-400 disabled:cursor-not-allowed";
const TAILWIND_CSS: Asset = asset!("public/tailwind.css");
const AUTO_NUMERIC_JS: Asset = asset!("assets/autoNumeric.min.js");

// TODO: Fix spaghetti UI
fn main() {
    LogTracer::init().unwrap();
    start_update_loop();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(448, 820)))
        .with_inner_size_constraints(WindowSizeConstraints::new(
            Some(PixelUnit::Physical(448.into())),
            Some(PixelUnit::Physical(820.into())),
            None,
            None,
        ))
        .with_resizable(true)
        .with_drag_and_drop(false)
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
    let last_preset = use_signal::<Option<(i64, String)>>(|| None);
    let configs = use_signal_sync(Vec::<ConfigurationData>::new);
    let config = use_signal_sync::<Option<ConfigurationData>>(|| None);
    let copy_position = use_signal::<Option<(i32, i32)>>(|| None);
    let mut active_tab = use_signal(|| TAB_CONFIGURATION.to_string());
    let mut script_loaded = use_signal(|| false);

    // Thanks dioxus
    use_future(move || async move {
        let mut eval = document::eval(
            r#"
            const scriptInterval = setInterval(async () => {
                try {
                    AutoNumeric;
                    await dioxus.send(true);
                    clearInterval(scriptInterval);
                } catch(_) { }
            }, 10);
        "#,
        );
        eval.recv::<bool>().await.unwrap();
        script_loaded.set(true);
    });

    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: AUTO_NUMERIC_JS }
        if script_loaded() {
            div { class: "flex flex-col max-w-xl h-screen mx-auto space-y-2",
                Minimap {
                    minimap,
                    preset,
                    last_preset,
                    copy_position,
                    config,
                }
                Tab {
                    tabs: vec![TAB_CONFIGURATION.to_string(), TAB_ACTIONS.to_string()],
                    class: "py-2 px-4 font-medium text-sm focus:outline-none",
                    selected_class: "bg-white text-gray-800",
                    unselected_class: "hover:text-gray-700 text-gray-400 bg-gray-100",
                    on_tab: move |tab| {
                        active_tab.set(tab);
                    },
                    tab: active_tab(),
                }
                match active_tab().as_str() {
                    TAB_CONFIGURATION => rsx! {
                        div { class: "px-2 pb-2 pt-2 overflow-y-auto scrollbar h-full",
                            Configuration { configs, config }
                        }
                    },
                    TAB_ACTIONS => rsx! {
                        ActionInput { minimap, preset, copy_position }
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
    #[props(default = String::new())]
    div_class: String,
    class: String,
    selected_class: String,
    unselected_class: String,
    on_tab: EventHandler<String>,
    tab: String,
}

#[component]
fn Tab(
    TabProps {
        tabs,
        div_class,
        class,
        selected_class,
        unselected_class,
        on_tab,
        tab,
    }: TabProps,
) -> Element {
    rsx! {
        div { class: "flex {div_class}",
            for t in tabs {
                button {
                    class: {
                        let conditional_class = if t == tab {
                            selected_class.clone()
                        } else {
                            unselected_class.clone()
                        };
                        format!("{conditional_class} {class}")
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
    disabled: bool,
    actions: Vec<Action>,
    on_click: EventHandler<(Action, usize)>,
    on_remove: EventHandler<usize>,
    on_change: EventHandler<(usize, usize, bool)>,
) -> Element {
    let mut drag_index = use_signal(|| None);
    let dragging = use_memo(move || drag_index().is_some());

    use_effect(use_reactive!(|disabled| {
        if disabled {
            drag_index.set(None);
        }
    }));

    rsx! {
        div { class: "flex-1 flex flex-col space-y-1 px-1 overflow-y-auto scrollbar rounded border-l border-gray-300",
            if actions.is_empty() {
                div { class: "flex items-center justify-center text-sm text-gray-500 h-full",
                    "No actions"
                }
            } else {
                for (i , action) in actions.into_iter().enumerate() {
                    ActionItem {
                        dragging: dragging(),
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
                        on_drop: move |(i, swapping)| {
                            if let Some(drag_i) = drag_index.take() {
                                if drag_i != i {
                                    on_change((drag_i, i, swapping));
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
    dragging: bool,
    on_click: EventHandler<()>,
    on_remove: EventHandler<()>,
    on_drag: EventHandler<usize>,
    on_drop: EventHandler<(usize, bool)>,
) -> Element {
    const KEY: &str = "font-mono w-1/2 text-xs";
    const VALUE: &str = "font-mono text-xs w-16 overflow-hidden text-ellipsis";
    const DIV: &str = "flex items-center space-x-1";

    #[component]
    fn ActionMoveItem(action: ActionMove) -> Element {
        let ActionMove {
            position:
                Position {
                    x,
                    y,
                    allow_adjusting,
                },
            condition,
            wait_after_move_millis,
        } = action;
        let wait_after_millis_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));

        use_auto_numeric(
            wait_after_millis_id,
            wait_after_move_millis.to_string(),
            None,
            u64::MAX.to_string(),
            "ms".to_string(),
        );

        rsx! {
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
                span { id: wait_after_millis_id(), class: VALUE }
            }
        }
    }

    #[component]
    fn ActionKeyItem(action: ActionKey) -> Element {
        let ActionKey {
            key,
            count,
            position,
            condition,
            direction,
            with,
            wait_before_use_millis,
            wait_after_use_millis,
            queue_to_front,
        } = action;
        let wait_before_use_millis_id =
            use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
        let wait_after_use_millis_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));

        use_auto_numeric(
            wait_before_use_millis_id,
            wait_before_use_millis.to_string(),
            None,
            u64::MAX.to_string(),
            "ms".to_string(),
        );
        use_auto_numeric(
            wait_after_use_millis_id,
            wait_after_use_millis.to_string(),
            None,
            u64::MAX.to_string(),
            "ms".to_string(),
        );

        rsx! {
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
                span { class: KEY, "Count" }
                span { class: VALUE, {count.to_string()} }
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
                span { id: wait_before_use_millis_id(), class: VALUE }
            }
            div { class: DIV,
                span { class: KEY, "Wait after" }
                span { id: wait_after_use_millis_id(), class: VALUE }
            }
            if let Some(queue_to_front) = queue_to_front {
                div { class: DIV,
                    span { class: KEY, "Queue to front" }
                    span { class: VALUE, {queue_to_front.to_string()} }
                }
            }
        }
    }

    let border_color = match action {
        Action::Move(_) => "border-blue-300",
        Action::Key(_) => "border-gray-300",
    };

    rsx! {
        div {
            class: "relative p-1 bg-white rounded shadow-sm cursor-move border-l-2 {border_color}",
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
                on_drop((index, true));
            },
            onclick: move |_| {
                on_click(());
            },
            div { class: "flex flex-col text-xs text-gray-700",
                match action {
                    Action::Move(action) => rsx! {
                        ActionMoveItem { action }
                    },
                    Action::Key(action) => rsx! {
                        ActionKeyItem { action }
                    },
                }
            }
            if dragging {
                div {
                    class: "absolute left-0 top-0 w-full h-1.5 bg-gray-300",
                    ondrop: move |e| {
                        e.stop_propagation();
                        on_drop((index, false));
                    },
                }
            }
            div { class: "absolute right-3 top-1",
                button {
                    onclick: move |e| {
                        e.stop_propagation();
                        on_remove(());
                    },
                    XIcon { class: "w-[10px] h-[10px] text-red-400 fill-current" }
                }
            }
        }
    }
}

#[component]
fn ActionInput(
    minimap: Signal<Option<MinimapData>>,
    preset: Signal<Option<String>>,
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
) -> Element {
    const TAB_PRESET: &str = "Preset";
    const TAB_ROTATION_MODE: &str = "Rotation Mode";
    const TAB_PLATFORMS: &str = "Platforms";

    let mut editing_action = use_signal::<Option<(Action, usize)>>(|| None);
    let mut value_action = use_signal(|| Action::Move(ActionMove::default()));
    let mut active_tab = use_signal(|| TAB_PRESET.to_string());

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
    let on_change = use_callback(move |(a, b, swapping)| {
        if let Some((minimap, preset)) = minimap.write().as_mut().zip(preset.peek().clone()) {
            let actions = minimap.actions.get_mut(&preset).unwrap();
            if swapping {
                actions.swap(a, b);
            } else {
                let action = actions.remove(a);
                actions.insert(b, action);
            }
            save_minimap(minimap.clone());
        }
    });
    let on_rotation_mode = use_callback(move |rotation_mode| {
        if let Some(minimap) = minimap.write().as_mut() {
            minimap.rotation_mode = rotation_mode;
            save_minimap(minimap.clone());
        }
    });

    use_effect(move || {
        if preset().is_none() {
            editing_action.set(None);
        }
    });

    rsx! {
        Tab {
            tabs: vec![
                TAB_PRESET.to_string(),
                TAB_ROTATION_MODE.to_string(),
                TAB_PLATFORMS.to_string(),
            ],
            div_class: "px-2 pt-2 pb-2 mb-2",
            class: "text-xs px-2 pb-2 focus:outline-none",
            selected_class: "text-gray-800 border-b",
            unselected_class: "hover:text-gray-700 text-gray-400",
            on_tab: move |tab| {
                active_tab.set(tab);
            },
            tab: active_tab(),
        }
        div { class: "px-2 pb-2 overflow-y-auto scrollbar h-full",
            match active_tab().as_str() {
                TAB_PRESET => rsx! {
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
                        div { class: "flex space-x-2 overflow-y-auto flex-1",
                            div { class: "w-1/2 overflow-y-auto scrollbar pr-2",
                                div { class: "flex flex-col space-y-2.5",
                                    ActionEnumSelect {
                                        label: "Type",
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
                                                copy_position,
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
                            }
                            ActionItemList {
                                disabled: preset().is_none(),
                                actions: actions(),
                                on_click: move |(action, index)| {
                                    editing_action.set(Some((action, index)));
                                    on_edit(action);
                                },
                                on_remove: move |index| {
                                    let editing = *editing_action.peek();
                                    if let Some((action, i)) = editing {
                                        if index == i {
                                            editing_action.set(None);
                                        } else if index < i {
                                            editing_action.set(Some((action, i.saturating_sub(1))));
                                        }
                                    }
                                    on_remove(index);
                                },
                                on_change: move |(a, b, swapping)| {
                                    let editing = *editing_action.peek();
                                    if let Some((action, index)) = editing {
                                        if index == a {
                                            editing_action.set(Some((action, b)));
                                        } else if swapping && index == b {
                                            editing_action.set(Some((action, a)));
                                        }
                                    }
                                    on_change((a, b, swapping));
                                },
                            }
                        }
                    }
                },
                TAB_ROTATION_MODE => rsx! {
                    Rotations {
                        disabled: minimap().is_none(),
                        on_input: move |value| {
                            on_rotation_mode(value);
                        },
                        value: minimap().map(|minimap| minimap.rotation_mode).unwrap_or_default(),
                    }
                },
                TAB_PLATFORMS => rsx! {
                    Platforms {
                        minimap,
                        on_save: move |minimap| {
                            save_minimap(minimap);
                        },
                        copy_position
                    }
                },
                _ => unreachable!(),
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
fn PositionNumberInput(
    label: String,
    on_icon_click: EventHandler,
    on_input: EventHandler<i32>,
    disabled: bool,
    value: i32,
) -> Element {
    let mut is_hovering = use_signal(|| false);

    rsx! {
        div {
            class: "relative",
            onmouseover: move |_| {
                is_hovering.set(true);
            },
            onmouseout: move |_| {
                is_hovering.set(false);
            },
            NumberInputI32 {
                label,
                div_class: DIV_CLASS,
                label_class: LABEL_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                disabled,
                on_input: move |value| {
                    on_input(value);
                },
                value,
            }
            button {
                class: {
                    let hidden = if is_hovering() && !disabled { "visible" } else { "invisible" };
                    let hover = if disabled { "" } else { "hover:visible" };
                    format!("absolute right-1 top-0 flex items-center h-full w-4 {hover} {hidden}")
                },
                disabled,
                onclick: move |e| {
                    e.stop_propagation();
                    on_icon_click(());
                },
                PositionIcon { class: "w-3 h-3 text-blue-500 fill-current" }
            }
        }
    }
}

#[component]
fn PositionInput(
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
    on_input: EventHandler<Position>,
    disabled: bool,
    value: Position,
) -> Element {
    let Position {
        x,
        y,
        allow_adjusting,
    } = value;

    rsx! {
        PositionNumberInput {
            label: "X",
            disabled,
            on_icon_click: move |_| {
                if let Some((x, _)) = *copy_position.peek() {
                    on_input(Position { x, ..value });
                }
            },
            on_input: move |x| {
                on_input(Position { x, ..value });
            },
            value: x,
        }
        PositionNumberInput {
            label: "Y",
            disabled,
            on_icon_click: move |_| {
                if let Some((_, y)) = *copy_position.peek() {
                    on_input(Position { y, ..value });
                }
            },
            on_input: move |y| {
                on_input(Position { y, ..value });
            },
            value: y,
        }
        Checkbox {
            label: "Adjust position",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: "w-22 flex items-center",
            disabled,
            on_input: move |allow_adjusting| {
                on_input(Position {
                    allow_adjusting,
                    ..value
                });
            },
            value: allow_adjusting,
        }
    }
}

#[component]
fn ActionMoveInput(
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
    on_input: EventHandler<Action>,
    disabled: bool,
    value: Action,
) -> Element {
    let Action::Move(value) = value else {
        unreachable!()
    };
    let ActionMove {
        position,
        condition,
        wait_after_move_millis,
    } = value;
    let submit = use_callback(move |action_move: ActionMove| on_input(Action::Move(action_move)));
    let set_position = use_callback(move |position| submit(ActionMove { position, ..value }));
    let set_condition = use_callback(move |condition| submit(ActionMove { condition, ..value }));
    let set_wait_after_move_millis = use_callback(move |wait_after_move_millis| {
        submit(ActionMove {
            wait_after_move_millis,
            ..value
        })
    });

    rsx! {
        div { class: "flex flex-col space-y-3",
            PositionInput {
                copy_position,
                on_input: move |position| {
                    set_position(position);
                },
                disabled,
                value: position,
            }
            ActionConditionInput {
                on_input: move |condition| {
                    set_condition(condition);
                },
                disabled,
                value: condition,
            }
            MillisInput {
                label: "Wait after action",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                disabled,
                on_input: move |value| {
                    set_wait_after_move_millis(value);
                },
                value: wait_after_move_millis,
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
        count,
        position,
        condition,
        direction,
        with,
        wait_before_use_millis,
        wait_after_use_millis,
        queue_to_front,
    } = value;
    let submit =
        use_callback(move |action_key: ActionKey| (props.on_input)(Action::Key(action_key)));
    let set_key = use_callback(move |key| submit(ActionKey { key, ..value }));
    let set_count = use_callback(move |count| submit(ActionKey { count, ..value }));
    let set_position = use_callback(move |position| submit(ActionKey { position, ..value }));
    let set_condition = use_callback(move |condition| submit(ActionKey { condition, ..value }));
    let set_direction = use_callback(move |direction| submit(ActionKey { direction, ..value }));
    let set_with = use_callback(move |with| submit(ActionKey { with, ..value }));
    let set_wait_before_use_millis = use_callback(move |wait_before_use_millis| {
        submit(ActionKey {
            wait_before_use_millis,
            ..value
        })
    });
    let set_wait_after_use_millis = use_callback(move |wait_after_use_millis| {
        submit(ActionKey {
            wait_after_use_millis,
            ..value
        })
    });
    let set_queue_to_front = use_callback(move |queue_to_front| {
        submit(ActionKey {
            queue_to_front,
            ..value
        })
    });

    use_effect(use_reactive!(|condition| {
        if matches!(condition, ActionCondition::Any) {
            set_queue_to_front(None);
        } else {
            set_queue_to_front(queue_to_front.or(Some(false)));
        }
    }));

    rsx! {
        div { class: "flex flex-col space-y-3",
            Checkbox {
                label: "Position",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "w-22 flex items-center",
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
            NumberInputU32 {
                label: "Count",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: INPUT_CLASS,
                disabled: props.disabled,
                on_input: move |key| {
                    set_count(key);
                },
                value: count,
            }
            ActionConditionInput {
                on_input: move |condition| {
                    set_condition(condition);
                },
                disabled: props.disabled,
                value: condition,
            }
            if let Some(queue_to_front) = queue_to_front {
                Checkbox {
                    label: "Queue to front",
                    label_class: LABEL_CLASS,
                    div_class: DIV_CLASS,
                    input_class: "w-22 flex items-center",
                    disabled: props.disabled,
                    on_input: move |checked: bool| {
                        set_queue_to_front(Some(checked));
                    },
                    value: queue_to_front,
                }
            }
            ActionEnumSelect::<ActionKeyDirection> {
                label: "Direction",
                on_input: move |direction| {
                    set_direction(direction);
                },
                disabled: props.disabled,
                value: direction,
            }
            ActionEnumSelect::<ActionKeyWith> {
                label: "With",
                on_input: move |with| {
                    set_with(with);
                },
                disabled: props.disabled,
                value: with,
            }
            MillisInput {
                label: "Wait before action",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                on_input: move |value| {
                    set_wait_before_use_millis(value);
                },
                disabled: props.disabled,
                value: wait_before_use_millis,
            }
            MillisInput {
                label: "Wait after action",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                on_input: move |value| {
                    set_wait_after_use_millis(value);
                },
                disabled: props.disabled,
                value: wait_after_use_millis,
            }
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
        ActionEnumSelect {
            label: "Condition",
            on_input,
            disabled,
            value,
        }
        if let ActionCondition::EveryMillis(millis) = value {
            MillisInput {
                label: "Milliseconds",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                disabled,
                on_input: move |millis| {
                    on_input(ActionCondition::EveryMillis(millis));
                },
                value: millis,
            }
        }
    }
}

#[component]
fn ActionEnumSelect<
    T: 'static + Clone + Copy + PartialEq + Display + FromStr<Err = ParseError> + IntoEnumIterator,
>(
    label: String,
    disabled: bool,
    on_input: EventHandler<T>,
    value: T,
) -> Element {
    rsx! {
        EnumSelect {
            label,
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            select_class: INPUT_CLASS,
            disabled,
            on_select: move |selected: T| {
                on_input(selected);
            },
            selected: value,
        }
    }
}
