use backend::{
    HotKeys, KeyBindingConfiguration, MAX_PLATFORMS_COUNT, Minimap, Platform, key_receiver,
};
use dioxus::prelude::*;

use crate::{
    icons::PositionIcon,
    input::{Checkbox, NumberInputI32},
};

const DIV_CLASS: &str = "flex h-6 items-center space-x-2";

#[component]
pub fn Platforms(
    minimap: ReadOnlySignal<Option<Minimap>>,
    hot_keys: ReadOnlySignal<Option<HotKeys>>,
    on_save: EventHandler<Minimap>,
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
) -> Element {
    let mut editing = use_signal(Platform::default);
    let add_platform_disabled = use_memo(move || {
        let minimap = minimap();
        minimap.is_none() || minimap.unwrap().platforms.len() >= MAX_PLATFORMS_COUNT
    });

    use_future(move || async move {
        let mut key_receiver = key_receiver().await;
        loop {
            let received_key = key_receiver.recv().await.unwrap();
            if minimap.peek().is_none() {
                continue;
            }
            if let Some((hot_keys, pos)) = hot_keys.peek().clone().zip(*copy_position.peek()) {
                let KeyBindingConfiguration { key, enabled } = hot_keys.platform_start_key;
                if enabled && key == received_key {
                    editing.with_mut(|platform| {
                        platform.x_start = pos.0;
                        platform.y = pos.1;
                    });
                    continue;
                }

                let KeyBindingConfiguration { key, enabled } = hot_keys.platform_end_key;
                if enabled && key == received_key {
                    editing.with_mut(|platform| {
                        platform.x_end = pos.0;
                        platform.y = pos.1;
                    });
                    continue;
                }

                let KeyBindingConfiguration { key, enabled } = hot_keys.platform_add_key;
                if enabled && key == received_key {
                    if let Some(mut minimap) = minimap.peek().clone() {
                        minimap.platforms.push(*editing.peek());
                        on_save(minimap);
                    }
                    continue;
                }
            }
        }
    });

    rsx! {
        div { class: "flex flex-col space-y-2",
            PlatformCheckbox {
                label: "Rune Pathing Enabled",
                disabled: minimap().is_none(),
                on_input: move |platforms_pathing| {
                    if let Some(mut minimap) = minimap.peek().clone() {
                        minimap.rune_platforms_pathing = platforms_pathing;
                        on_save(minimap);
                    }
                },
                value: minimap().map(|data| data.rune_platforms_pathing).unwrap_or_default(),
            }
            PlatformCheckbox {
                label: "Rune Pathing Up Jump Only",
                disabled: minimap().is_none(),
                on_input: move |up_jump_only| {
                    if let Some(mut minimap) = minimap.peek().clone() {
                        minimap.rune_platforms_pathing_up_jump_only = up_jump_only;
                        on_save(minimap);
                    }
                },
                value: {
                    minimap()
                        .map(|data| data.rune_platforms_pathing_up_jump_only)
                        .unwrap_or_default()
                },
            }
            PlatformCheckbox {
                label: "Auto Mobbing Pathing Enabled",
                disabled: minimap().is_none(),
                on_input: move |platforms_pathing| {
                    if let Some(mut minimap) = minimap.peek().clone() {
                        minimap.auto_mob_platforms_pathing = platforms_pathing;
                        on_save(minimap);
                    }
                },
                value: minimap().map(|data| data.auto_mob_platforms_pathing).unwrap_or_default(),
            }
            PlatformCheckbox {
                label: "Auto Mobbing Pathing Up Jump Only",
                disabled: minimap().is_none(),
                on_input: move |up_jump_only| {
                    if let Some(mut minimap) = minimap.peek().clone() {
                        minimap.auto_mob_platforms_pathing_up_jump_only = up_jump_only;
                        on_save(minimap);
                    }
                },
                value: minimap()
                    .map(|data| data.auto_mob_platforms_pathing_up_jump_only)
                    .unwrap_or_default(),
            }
            PlatformCheckbox {
                label: "Auto Mobbing Bound By Platforms",
                disabled: minimap().is_none(),
                on_input: move |bound| {
                    if let Some(mut minimap) = minimap.peek().clone() {
                        minimap.auto_mob_platforms_bound = bound;
                        on_save(minimap);
                    }
                },
                value: minimap().map(|data| data.auto_mob_platforms_bound).unwrap_or_default(),
            }
            div { class: "flex items-center justify-between text-xs text-gray-700 border-b border-gray-300 mt-3 mb-2 data-[disabled]:text-gray-400",
                p { class: "w-26", "X Start" }
                p { class: "w-26", "X End" }
                p { class: "w-26", "Y" }
                div { class: "w-18" }
            }
            if let Some(Minimap { platforms, .. }) = minimap() {
                for (i , platform) in platforms.into_iter().enumerate() {
                    PlatformInput {
                        copy_position,
                        label: "Delete",
                        delete: true,
                        disabled: minimap().is_none(),
                        on_click: move |_| {
                            if let Some(mut minimap) = minimap.peek().clone() {
                                minimap.platforms.remove(i);
                                on_save(minimap);
                            }
                        },
                        on_input: move |value| {
                            if let Some(mut minimap) = minimap.peek().clone() {
                                *minimap.platforms.get_mut(i).unwrap() = value;
                                on_save(minimap);
                            }
                        },
                        value: platform,
                    }
                }
            }
            PlatformInput {
                copy_position,
                label: "Add",
                delete: false,
                disabled: add_platform_disabled(),
                on_click: move |_| {
                    if let Some(mut minimap) = minimap.peek().clone() {
                        minimap.platforms.push(*editing.peek());
                        on_save(minimap);
                    }
                },
                on_input: move |value| {
                    editing.set(value);
                },
                value: editing(),
            }
        }
    }
}

#[component]
fn PlatformCheckbox(
    label: String,
    disabled: bool,
    on_input: EventHandler<bool>,
    value: bool,
) -> Element {
    const CHECKBOX_LABEL_CLASS: &str =
        "w-64 text-xs text-gray-700 inline-block data-[disabled]:text-gray-400";
    const CHECKBOX_INPUT_CLASS: &str = "flex item-centers";

    rsx! {
        Checkbox {
            label,
            label_class: CHECKBOX_LABEL_CLASS,
            div_class: DIV_CLASS,
            input_class: CHECKBOX_INPUT_CLASS,
            disabled,
            on_input,
            value,
        }
    }
}

#[component]
fn PlatformInput(
    copy_position: ReadOnlySignal<Option<(i32, i32)>>,
    label: String,
    delete: bool,
    disabled: bool,
    on_click: EventHandler,
    on_input: EventHandler<Platform>,
    value: Platform,
) -> Element {
    let Platform { x_start, x_end, y } = value;

    rsx! {
        div { class: "flex items-center justify-between text-xs text-gray-700",
            PlatformNumberInput {
                disabled,
                on_icon_click: move |_| {
                    if let Some((x_start, _)) = *copy_position.peek() {
                        on_input(Platform { x_start, ..value });
                    }
                },
                on_input: move |x_start| {
                    on_input(Platform { x_start, ..value });
                },
                value: x_start,
            }
            PlatformNumberInput {
                disabled,
                on_icon_click: move |_| {
                    if let Some((x_end, _)) = *copy_position.peek() {
                        on_input(Platform { x_end, ..value });
                    }
                },
                on_input: move |x_end| {
                    on_input(Platform { x_end, ..value });
                },
                value: x_end,
            }
            PlatformNumberInput {
                disabled,
                on_icon_click: move |_| {
                    if let Some((_, y)) = *copy_position.peek() {
                        on_input(Platform { y, ..value });
                    }
                },
                on_input: move |y| {
                    on_input(Platform { y, ..value });
                },
                value: y,
            }
            button {
                class: {
                    let class = if delete { "button-danger" } else { "button-primary" };
                    format!("{class} h-6 w-18")
                },
                disabled,
                onclick: move |_| {
                    on_click(());
                },
                {label}
            }
        }
    }
}

#[component]
fn PlatformNumberInput(
    disabled: bool,
    on_icon_click: EventHandler,
    on_input: EventHandler<i32>,
    value: i32,
) -> Element {
    const INPUT_CLASS: &str = "w-26 h-6 px-1.5 border border-gray-300 rounded text-xs text-ellipsis outline-none disabled:text-gray-400 disabled:cursor-not-allowed";

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
                label: "",
                label_class: "hidden",
                input_class: INPUT_CLASS,
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
                onclick: move |e| {
                    e.stop_propagation();
                    on_icon_click(());
                },
                PositionIcon { class: "w-3 h-3 text-blue-500 fill-current" }
            }
        }
    }
}
