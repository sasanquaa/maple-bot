use std::{
    cmp::{max, min},
    fmt::Display,
    ops::DerefMut,
    str::FromStr,
};

use backend::{
    Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove,
    IntoEnumIterator, LinkKeyBinding, Minimap, ParseError, Position, upsert_map,
};
use dioxus::prelude::*;
use rand::distr::{Alphanumeric, SampleString};
use tokio::task::spawn_blocking;

use crate::{
    icons::{PositionIcon, XIcon},
    input::{
        Checkbox, KeyBindingInput, MillisInput, NumberInputI32, NumberInputU32, use_auto_numeric,
    },
    platform::Platforms,
    rotation::Rotations,
    select::{EnumSelect, TextSelect},
    tab::Tab,
};

const DIV_CLASS: &str = "flex h-6 items-center space-x-2";
const LABEL_CLASS: &str = "flex-1 text-xs text-gray-700 inline-block data-[disabled]:text-gray-400";
const INPUT_CLASS: &str = "w-22 h-full border border-gray-300 rounded text-xs text-ellipsis outline-none disabled:text-gray-400 disabled:cursor-not-allowed";

#[component]
pub fn Actions(
    minimap: Signal<Option<Minimap>>,
    preset: Signal<Option<String>>,
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
) -> Element {
    const TAB_PRESET: &str = "Preset";
    const TAB_ROTATION_MODE: &str = "Rotation Mode";
    const TAB_PLATFORMS: &str = "Platforms";

    let mut editing_action = use_signal::<Option<(Action, usize)>>(|| None);
    let value_action = use_signal(|| Action::Move(ActionMove::default()));
    let mut active_tab = use_signal(|| TAB_PRESET.to_string());

    let save_minimap = move |mut minimap: Minimap| {
        spawn(async move {
            spawn_blocking(move || {
                upsert_map(&mut minimap).unwrap();
            })
            .await
            .unwrap();
        });
    };
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
                    ActionPresetTab {
                        minimap,
                        preset,
                        copy_position,
                        value_action,
                        editing_action,
                        save_minimap,
                    }
                },
                TAB_ROTATION_MODE => rsx! {
                    Rotations {
                        disabled: minimap().is_none(),
                        on_input: on_rotation_mode,
                        value: minimap().map(|minimap| minimap.rotation_mode).unwrap_or_default(),
                    }
                },
                TAB_PLATFORMS => rsx! {
                    Platforms { minimap, on_save: save_minimap, copy_position }
                },
                _ => unreachable!(),
            }
        }
    }
}

#[component]
fn ActionPresetTab(
    minimap: Signal<Option<Minimap>>,
    preset: Signal<Option<String>>,
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
    value_action: Signal<Action>,
    editing_action: Signal<Option<(Action, usize)>>,
    save_minimap: EventHandler<Minimap>,
) -> Element {
    fn is_linked_condition_action(action: Action) -> bool {
        match action {
            Action::Move(ActionMove { condition, .. })
            | Action::Key(ActionKey { condition, .. }) => {
                matches!(condition, ActionCondition::Linked)
            }
        }
    }

    fn is_linked_action(actions: &[Action], index: usize) -> bool {
        if index + 1 < actions.len() {
            return is_linked_condition_action(actions[index + 1]);
        }
        false
    }

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
            let actions = minimap.actions.get_mut(&preset).unwrap();
            let is_linked_action =
                is_linked_action(actions, index) && !is_linked_condition_action(actions[index]);
            actions.remove(index);
            if is_linked_action {
                let action = actions.get_mut(index).unwrap();
                match action {
                    Action::Move(ActionMove { condition, .. })
                    | Action::Key(ActionKey { condition, .. }) => {
                        *condition = ActionCondition::Any;
                    }
                }
            }

            let editing = *editing_action.peek();
            if let Some((action, i)) = editing {
                if index == i {
                    editing_action.set(None);
                } else if index < i {
                    let new_i = i.saturating_sub(1);
                    if new_i == index {
                        editing_action.set(Some((*actions.get(index).unwrap(), new_i)));
                    } else {
                        editing_action.set(Some((action, new_i)));
                    }
                }
            }
            save_minimap(minimap.clone());
        }
    });
    let on_change = use_callback(move |(a, b, swapping)| {
        editing_action.set(None); // FIXME
        // let editing = *editing_action.peek();
        // if let Some((action, index)) = editing {
        //     if index == a {
        //         editing_action.set(Some((action, b)));
        //     } else if swapping && index == b {
        //         editing_action.set(Some((action, a)));
        //     }
        // }
        if let Some((minimap, preset)) = minimap.write().as_mut().zip(preset.peek().clone()) {
            let actions = minimap.actions.get_mut(&preset).unwrap();
            if swapping {
                let tmp = a;
                let a = min(tmp, b);
                let b = max(tmp, b);
                let is_a_linked_action = is_linked_action(actions, a);
                let is_b_linked_action = is_linked_action(actions, b);
                let mut a_actions = vec![];
                let mut b_actions = vec![];
                b_actions.push(actions.remove(b));
                if is_b_linked_action {
                    while b < actions.len() && is_linked_condition_action(actions[b]) {
                        b_actions.push(actions.remove(b));
                    }
                }
                a_actions.push(actions.remove(a));
                if is_a_linked_action {
                    while a < actions.len() && is_linked_condition_action(actions[a]) {
                        a_actions.push(actions.remove(a));
                    }
                }
                let a_offset = b - (a + a_actions.len());
                let a_insert = a_offset + a + b_actions.len();
                for action in b_actions.into_iter().rev() {
                    actions.insert(a, action);
                }
                for action in a_actions.into_iter().rev() {
                    actions.insert(a_insert, action);
                }
            } else {
                let is_a_linked_action = is_linked_action(actions, a);
                let mut a_actions = vec![actions.remove(a)];
                if is_a_linked_action {
                    while a < actions.len() && is_linked_condition_action(actions[a]) {
                        a_actions.push(actions.remove(a));
                    }
                }
                for action in a_actions.into_iter().rev() {
                    actions.insert(b, action);
                }
            }
            save_minimap(minimap.clone());
        }
    });
    let exclude_linked =
        use_memo(move || matches!(editing_action(), Some((_, 0))) || actions().is_empty());

    use_effect(move || {
        if actions().is_empty() {
            value_action.set(Action::Move(ActionMove::default()));
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
                                    exclude_linked: exclude_linked(),
                                }
                            },
                            Action::Key(_) => rsx! {
                                ActionKeyInput {
                                    copy_position,
                                    on_input: move |action| {
                                        on_edit(action);
                                    },
                                    disabled: preset().is_none(),
                                    value: value_action(),
                                    exclude_linked: exclude_linked(),
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
                        on_remove(index);
                    },
                    on_change: move |(a, b, swapping)| {
                        on_change((a, b, swapping));
                    },
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
        div { class: "flex-1 flex flex-col space-y-1 px-1 overflow-y-auto scrollbar rounded",
            if actions.is_empty() {
                div { class: "flex items-center justify-center text-sm text-gray-500 h-full",
                    "No actions"
                }
            } else {
                for (i , action) in actions.into_iter().enumerate() {
                    ActionItem {
                        dragging: dragging(),
                        draggable: match action {
                            Action::Move(ActionMove { condition, .. })
                            | Action::Key(ActionKey { condition, .. }) => {
                                !matches!(condition, ActionCondition::Linked)
                            }
                        },
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
    draggable: bool,
    on_click: EventHandler<()>,
    on_remove: EventHandler<()>,
    on_drag: EventHandler<usize>,
    on_drop: EventHandler<(usize, bool)>,
) -> Element {
    const KEY: &str = "font-mono w-1/2 text-xs";
    const VALUE: &str = "font-mono text-xs flex-1 overflow-hidden text-ellipsis";
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
            link_key,
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
            if let Some(link_key) = link_key {
                div { class: DIV,
                    span { class: KEY, "Link Key" }
                    span { class: VALUE, {link_key.to_string()} }
                }
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
    let cursor = if draggable { "cursur-move" } else { "" };

    rsx! {
        div {
            class: "relative p-1 bg-white rounded shadow-sm {cursor} border-l-2 {border_color}",
            draggable,
            ondragenter: move |e| {
                e.prevent_default();
            },
            ondragover: move |e| {
                e.prevent_default();
            },
            ondragstart: move |e| {
                if !draggable {
                    e.prevent_default();
                } else {
                    on_drag(index);
                }
            },
            ondrop: move |e| {
                if !draggable {
                    e.prevent_default();
                } else {
                    on_drop((index, true));
                }
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
            if draggable && dragging {
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
        ActionCheckbox {
            label: "Adjust position",
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
    exclude_linked: bool,
) -> Element {
    let Action::Move(value) = value else {
        unreachable!()
    };
    let ActionMove {
        position,
        condition,
        wait_after_move_millis,
    } = value;

    rsx! {
        div { class: "flex flex-col space-y-3",
            PositionInput {
                copy_position,
                on_input: move |position| {
                    on_input(Action::Move(ActionMove { position, ..value }));
                },
                disabled,
                value: position,
            }
            ActionConditionInput {
                on_input: move |condition| {
                    on_input(Action::Move(ActionMove { condition, ..value }));
                },
                disabled,
                value: condition,
                exclude_linked,
            }
            ActionMillisInput {
                label: "Wait after action",
                disabled,
                on_input: move |wait_after_move_millis| {
                    on_input(
                        Action::Move(ActionMove {
                            wait_after_move_millis,
                            ..value
                        }),
                    );
                },
                value: wait_after_move_millis,
            }
        }
    }
}

#[component]
fn ActionKeyInput(
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
    on_input: EventHandler<Action>,
    disabled: bool,
    value: Action,
    exclude_linked: bool,
) -> Element {
    let Action::Key(value) = value else {
        unreachable!()
    };
    let ActionKey {
        key,
        link_key,
        count,
        position,
        condition,
        direction,
        with,
        wait_before_use_millis,
        wait_after_use_millis,
        queue_to_front,
    } = value;

    use_effect(use_reactive!(|condition| {
        on_input(Action::Key(ActionKey {
            queue_to_front: (!matches!(condition, ActionCondition::Any))
                .then_some(queue_to_front.unwrap_or_default()),
            ..value
        }));
    }));

    rsx! {
        div { class: "flex flex-col space-y-3",
            ActionCheckbox {
                label: "Position",
                disabled,
                on_input: move |checked: bool| {
                    on_input(
                        Action::Key(ActionKey {
                            position: checked.then_some(Position::default()),
                            ..value
                        }),
                    );
                },
                value: position.is_some(),
            }
            if let Some(position) = position {
                PositionInput {
                    copy_position,
                    on_input: move |position| {
                        on_input(
                            Action::Key(ActionKey {
                                position: Some(position),
                                ..value
                            }),
                        );
                    },
                    disabled,
                    value: position,
                }
            }
            KeyBindingInput {
                label: "Key",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: INPUT_CLASS,
                disabled,
                on_input: move |key| {
                    on_input(Action::Key(ActionKey { key, ..value }));
                },
                value: key,
            }
            NumberInputU32 {
                label: "Count",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "{INPUT_CLASS} p-1",
                disabled,
                on_input: move |count| {
                    on_input(Action::Key(ActionKey { count, ..value }));
                },
                value: count,
            }
            ActionCheckbox {
                label: "Has link key",
                disabled,
                on_input: move |checked: bool| {
                    on_input(
                        Action::Key(ActionKey {
                            link_key: checked.then_some(LinkKeyBinding::default()),
                            ..value
                        }),
                    );
                },
                value: link_key.is_some(),
            }
            if let Some(link_key) = link_key {
                ActionEnumSelect::<LinkKeyBinding> {
                    label: "Link key type",
                    on_input: move |link_key| {
                        on_input(
                            Action::Key(ActionKey {
                                link_key: Some(link_key),
                                ..value
                            }),
                        );
                    },
                    disabled,
                    value: link_key,
                }
                KeyBindingInput {
                    label: "Link key",
                    label_class: LABEL_CLASS,
                    div_class: DIV_CLASS,
                    input_class: INPUT_CLASS,
                    disabled,
                    on_input: move |key| {
                        on_input(
                            Action::Key(ActionKey {
                                link_key: Some(link_key.with_key(key)),
                                ..value
                            }),
                        );
                    },
                    value: link_key.key(),
                }
            }
            ActionConditionInput {
                on_input: move |condition| {
                    on_input(Action::Key(ActionKey { condition, ..value }));
                },
                disabled,
                value: condition,
                exclude_linked,
            }
            if let Some(queue_to_front) = queue_to_front {
                ActionCheckbox {
                    label: "Queue to front",
                    disabled,
                    on_input: move |checked: bool| {
                        on_input(
                            Action::Key(ActionKey {
                                queue_to_front: Some(checked),
                                ..value
                            }),
                        );
                    },
                    value: queue_to_front,
                }
            }
            ActionEnumSelect::<ActionKeyDirection> {
                label: "Direction",
                on_input: move |direction| {
                    on_input(Action::Key(ActionKey { direction, ..value }));
                },
                disabled,
                value: direction,
            }
            ActionEnumSelect::<ActionKeyWith> {
                label: "With",
                on_input: move |with| {
                    on_input(Action::Key(ActionKey { with, ..value }));
                },
                disabled,
                value: with,
            }
            ActionMillisInput {
                label: "Wait before action",
                on_input: move |wait_before_use_millis| {
                    on_input(
                        Action::Key(ActionKey {
                            wait_before_use_millis,
                            ..value
                        }),
                    );
                },
                disabled,
                value: wait_before_use_millis,
            }
            ActionMillisInput {
                label: "Wait after action",
                on_input: move |wait_after_use_millis| {
                    on_input(
                        Action::Key(ActionKey {
                            wait_after_use_millis,
                            ..value
                        }),
                    );
                },
                disabled,
                value: wait_after_use_millis,
            }
        }
    }
}

#[component]
fn ActionConditionInput(
    on_input: EventHandler<ActionCondition>,
    disabled: bool,
    value: ActionCondition,
    exclude_linked: bool,
) -> Element {
    rsx! {
        ActionEnumSelect {
            label: "Condition",
            on_input,
            disabled,
            value,
            excludes: if exclude_linked { vec![ActionCondition::Linked] } else { vec![] },
        }
        if let ActionCondition::EveryMillis(millis) = value {
            ActionMillisInput {
                label: "Milliseconds",
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
fn ActionCheckbox(
    label: String,
    disabled: bool,
    on_input: EventHandler<bool>,
    value: bool,
) -> Element {
    rsx! {
        Checkbox {
            label,
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: "w-22 flex items-center",
            disabled,
            on_input,
            value,
        }
    }
}

#[component]
fn ActionMillisInput(
    label: String,
    disabled: bool,
    on_input: EventHandler<u64>,
    value: u64,
) -> Element {
    rsx! {
        MillisInput {
            label,
            label_class: LABEL_CLASS,
            div_class: DIV_CLASS,
            input_class: "{INPUT_CLASS} p-1",
            disabled,
            on_input,
            value,
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
    #[props(default = Vec::new())] excludes: Vec<T>,
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
            excludes,
        }
    }
}
