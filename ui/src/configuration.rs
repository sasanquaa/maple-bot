use std::str::FromStr;

use crate::option::Option;
use backend::{
    IntoEnumIterator, KeyBinding, KeyBindingConfiguration, RotationMode, RotationModeDiscriminants,
    query_config, refresh_configuration, upsert_config,
};
use dioxus::prelude::*;

use crate::key::KeyInput;

const ROPE_LIFT: &str = "Rope Lift";
const UP_JUMP: &str = "Up Jump";
const INTERACT: &str = "Interact";
const CASH_SHOP: &str = "Cash Shop";
const FEED_PET: &str = "Feed Pet";
const POTION: &str = "Potion";
const SAYRAM_ELIXIR: &str = "Sayram's Elixir";
const EXP_X3: &str = "3x EXP Coupon";
const BONUS_EXP: &str = "50% Bonus EXP Coupon";
const LEGION_WEALTH: &str = "Legion's Wealth";
const LEGION_LUCK: &str = "Legion's Luck";

#[component]
pub fn Configuration() -> Element {
    let mut config = use_signal(|| query_config().unwrap());
    let mut active = use_signal(|| None);

    use_effect(move || {
        upsert_config(&mut config()).unwrap();
        spawn(async move {
            refresh_configuration().await;
        });
    });

    rsx! {
        div { class: "flex flex-col",
            h2 { class: "text-sm font-medium text-gray-700 mb-2", "Key Bindings" }
            div { class: "space-y-1",
                KeyBindingConfigurationInput {
                    label: ROPE_LIFT,
                    is_optional: false,
                    is_active: matches!(active(), Some(ROPE_LIFT)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(ROPE_LIFT));
                    },
                    can_disable: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().ropelift_key = key.unwrap();
                    },
                    value: Some(config().ropelift_key),
                }
                KeyBindingConfigurationInput {
                    label: UP_JUMP,
                    is_optional: true,
                    is_active: matches!(active(), Some(UP_JUMP)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(UP_JUMP));
                    },
                    can_disable: false,
                    on_input: move |key| {
                        config.write().up_jump_key = key;
                    },
                    value: config().up_jump_key,
                }
                KeyBindingConfigurationInput {
                    label: INTERACT,
                    is_optional: false,
                    is_active: matches!(active(), Some(INTERACT)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(INTERACT));
                    },
                    can_disable: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().interact_key = key.unwrap();
                    },
                    value: Some(config().interact_key),
                }
                KeyBindingConfigurationInput {
                    label: CASH_SHOP,
                    is_optional: false,
                    is_active: matches!(active(), Some(CASH_SHOP)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(CASH_SHOP));
                    },
                    can_disable: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().cash_shop_key = key.unwrap();
                    },
                    value: Some(config().cash_shop_key),
                }
                KeyBindingConfigurationInput {
                    label: FEED_PET,
                    is_optional: false,
                    is_active: matches!(active(), Some(FEED_PET)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(FEED_PET));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().feed_pet_key = key.unwrap();
                    },
                    value: Some(config().feed_pet_key),
                }
                KeyBindingConfigurationInput {
                    label: POTION,
                    is_optional: false,
                    is_active: matches!(active(), Some(POTION)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(POTION));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().potion_key = key.unwrap();
                    },
                    value: Some(config().potion_key),
                }
                KeyBindingConfigurationInput {
                    label: SAYRAM_ELIXIR,
                    is_optional: false,
                    is_active: matches!(active(), Some(SAYRAM_ELIXIR)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(SAYRAM_ELIXIR));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().sayram_elixir_key = key.unwrap();
                    },
                    value: Some(config().sayram_elixir_key),
                }
                KeyBindingConfigurationInput {
                    label: EXP_X3,
                    is_optional: false,
                    is_active: matches!(active(), Some(EXP_X3)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(EXP_X3));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().exp_x3_key = key.unwrap();
                    },
                    value: Some(config().exp_x3_key),
                }
                KeyBindingConfigurationInput {
                    label: BONUS_EXP,
                    is_optional: false,
                    is_active: matches!(active(), Some(BONUS_EXP)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(BONUS_EXP));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().bonus_exp_key = key.unwrap();
                    },
                    value: Some(config().bonus_exp_key),
                }
                KeyBindingConfigurationInput {
                    label: LEGION_WEALTH,
                    is_optional: false,
                    is_active: matches!(active(), Some(LEGION_WEALTH)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(LEGION_WEALTH));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().legion_wealth_key = key.unwrap();
                    },
                    value: Some(config().legion_wealth_key),
                }
                KeyBindingConfigurationInput {
                    label: LEGION_LUCK,
                    is_optional: false,
                    is_active: matches!(active(), Some(LEGION_LUCK)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(LEGION_LUCK));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        config.write().legion_luck_key = key.unwrap();
                    },
                    value: Some(config().legion_luck_key),
                }
            }
            h2 { class: "text-sm font-medium text-gray-700 mt-4 mb-2", "Other" }
            RotationModeInput {
                on_input: move |mode| {
                    config.write().rotation_mode = mode;
                },
                value: config().rotation_mode,
            }
        }
    }
}

#[component]
fn RotationModeInput(on_input: EventHandler<RotationMode>, value: RotationMode) -> Element {
    let options = RotationModeDiscriminants::iter()
        .map(|disc| (disc, disc.to_string()))
        .collect::<Vec<_>>();
    let selected = RotationModeDiscriminants::from(value);

    rsx! {
        Option {
            label: "Rotation Mode",
            div_class: "flex items-center space-x-4",
            label_class: "text-xs text-gray-700 flex-1 inline-block",
            select_class: "w-44 text-xs text-gray-700 text-ellipsis rounded outline-none",
            options,
            on_select: move |disc: RotationModeDiscriminants| {
                on_input(RotationMode::from_str(&disc.to_string()).unwrap());
            },
            selected,
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct KeyBindingConfigurationInputProps {
    label: String,
    is_optional: bool,
    is_active: bool,
    on_active: EventHandler<bool>,
    can_disable: bool,
    on_input: EventHandler<Option<KeyBindingConfiguration>>,
    value: Option<KeyBindingConfiguration>,
}

#[component]
fn KeyBindingConfigurationInput(props: KeyBindingConfigurationInputProps) -> Element {
    debug_assert!(props.is_optional || props.value.is_some());

    let is_enabled = props.value.map(|key| key.enabled).unwrap_or(true);
    let on_enabled_input = use_callback(move |enabled: bool| {
        (props.on_input)(
            props
                .value
                .map(|config| KeyBindingConfiguration { enabled, ..config }),
        );
    });
    let on_key_input = use_callback(move |key: KeyBinding| {
        (props.on_input)(
            props
                .value
                .or(Some(KeyBindingConfiguration::default()))
                .map(|config| KeyBindingConfiguration { key, ..config }),
        );
    });

    rsx! {
        div { class: "flex items-center space-x-4 py-2 border-b border-gray-100",
            div { class: "flex-1",
                span { class: "text-xs text-gray-700",
                    {props.label}
                    if props.is_optional {
                        span { class: "text-xs text-gray-400 ml-1", "(Optional)" }
                    }
                }
            }
            div { class: "flex items-center space-x-2",
                KeyInput {
                    class: format!(
                        "border rounded border-gray-300 h-7 {}",
                        if props.can_disable { "w-24" } else { "w-44" },
                    ),
                    is_active: props.is_active,
                    on_active: props.on_active,
                    on_input: move |key| {
                        if let Some(key) = key {
                            on_key_input(key);
                        }
                    },
                    value: props.value.map(|key| key.key),
                }
                if props.can_disable {
                    button {
                        r#type: "button",
                        disabled: props.value.is_none(),
                        class: {
                            let color = if is_enabled {
                                "enabled:bg-blue-100 enabled:text-blue-700 enabled:hover:bg-blue-200"
                            } else {
                                "enabled:bg-red-100 enabled:text-red-500 enabled:hover:bg-red-200"
                            };
                            let disabled = "disabled:bg-gray-100 disabled:text-gray-500 disabled:cursor-not-allowed";
                            let class = "px-3 py-1.5 rounded w-18 text-xs font-medium";
                            format!("{class} {disabled} {color}")
                        },
                        onclick: move |_| {
                            if let Some(config) = props.value {
                                on_enabled_input(!config.enabled);
                            }
                        },
                        if is_enabled {
                            "Enabled"
                        } else {
                            "Disabled"
                        }
                    }
                }
            }
        }
    }
}
