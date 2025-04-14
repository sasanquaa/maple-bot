use std::{fmt::Display, str::FromStr};

use backend::{
    Class, Configuration as ConfigurationData, IntoEnumIterator, KeyBindingConfiguration,
    PotionMode, upsert_config,
};
use dioxus::prelude::*;
use tokio::task::spawn_blocking;

use crate::{
    icons::XIcon,
    input::{MillisInput, PercentageInput},
    key::KeyInput,
    select::{EnumSelect, TextSelect},
};

const DIV_CLASS: &str = "flex items-center space-x-4 text-xs text-gray-700";
const LABEL_CLASS: &str = "flex-1";
const INPUT_CLASS: &str = "w-44 rounded border border-gray-300 px-1 text-gray-700 h-6 outline-none";
const ROPE_LIFT: &str = "Rope Lift";
const TELEPORT: &str = "Teleport";
const UP_JUMP: &str = "Up Jump";
const INTERACT: &str = "Interact";
const CASH_SHOP: &str = "Cash Shop";
const FEED_PET: &str = "Feed Pet";
const POTION: &str = "Potion";
const SAYRAM_ELIXIR: &str = "Sayram's Elixir";
const AURELIA_ELIXIR: &str = "Aurelia's Elixir";
const EXP_X3: &str = "3x EXP Coupon";
const BONUS_EXP: &str = "50% Bonus EXP Coupon";
const LEGION_WEALTH: &str = "Legion's Wealth";
const LEGION_LUCK: &str = "Legion's Luck";

#[component]
pub fn Configuration(
    configs: Resource<Vec<ConfigurationData>>,
    config: SyncSignal<Option<ConfigurationData>>,
) -> Element {
    let is_disabled = use_memo(move || config().is_none());
    let active = use_signal(|| None);
    let configs_value = configs.value();
    let config_names = use_memo(move || match configs_value() {
        Some(configs) => configs
            .into_iter()
            .map(|config| config.name.clone())
            .collect(),
        None => vec![],
    });
    let config_view = use_memo(move || config().unwrap_or_default());
    let on_config = use_callback(move |new_config: ConfigurationData| {
        spawn(async move {
            spawn_blocking(move || {
                let mut new_config = new_config;
                upsert_config(&mut new_config).unwrap();
                config.set(Some(new_config.clone()));
            })
            .await
            .unwrap();
            configs.restart();
        });
    });

    use_effect(move || {
        if let Some(configs) = configs_value() {
            debug_assert!(!configs.is_empty());
            if config.peek().is_none() {
                config.set(Some(configs.into_iter().next().unwrap()));
            }
        }
    });

    rsx! {
        div { class: "flex flex-col",
            TextSelect {
                create_text: "+ Create new preset",
                on_create: move |created: String| {
                    on_config(ConfigurationData {
                        name: created,
                        ..ConfigurationData::default()
                    });
                },
                disabled: is_disabled(),
                on_select: move |(i, _)| {
                    config.set(configs_value().unwrap().get(i).cloned());
                },
                options: config_names(),
                selected: config_view().name,
            }
            ConfigHeader {
                text: "Key Bindings",
                disabled: is_disabled(),
                class: "mt-2",
            }
            div { class: "space-y-1",
                KeyBindingConfigurationInput {
                    label: ROPE_LIFT,
                    label_active: active,
                    is_disabled: is_disabled(),
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            ropelift_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().ropelift_key),
                }
                KeyBindingConfigurationInput {
                    label: TELEPORT,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_optional: true,
                    on_input: move |key| {
                        on_config(ConfigurationData {
                            teleport_key: key,
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().teleport_key,
                }
                KeyBindingConfigurationInput {
                    label: UP_JUMP,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_optional: true,
                    on_input: move |key| {
                        on_config(ConfigurationData {
                            up_jump_key: key,
                            ..config_view.peek().clone()
                        });
                    },
                    value: config_view().up_jump_key,
                }
                KeyBindingConfigurationInput {
                    label: INTERACT,
                    label_active: active,
                    is_disabled: is_disabled(),
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            interact_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().interact_key),
                }
                KeyBindingConfigurationInput {
                    label: CASH_SHOP,
                    label_active: active,
                    is_disabled: is_disabled(),
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            cash_shop_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().cash_shop_key),
                }
                KeyBindingConfigurationInput {
                    label: FEED_PET,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            feed_pet_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().feed_pet_key),
                    ConfigMillisInput {
                        label: "Every Milliseconds",
                        disabled: is_disabled(),
                        on_input: move |value| {
                            on_config(ConfigurationData {
                                feed_pet_millis: value,
                                ..config_view.peek().clone()
                            });
                        },
                        value: config_view().feed_pet_millis,
                    }
                }
                KeyBindingConfigurationInput {
                    label: POTION,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            potion_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().potion_key),
                    ConfigEnumSelect::<PotionMode> {
                        label: "Potion Mode",
                        on_select: move |mode| {
                            on_config(ConfigurationData {
                                potion_mode: mode,
                                ..config_view.peek().clone()
                            });
                        },
                        disabled: is_disabled(),
                        selected: config_view().potion_mode,
                    }
                    match config_view().potion_mode {
                        PotionMode::EveryMillis(value) => rsx! {
                            ConfigMillisInput {
                                label: "Every Milliseconds",
                                disabled: is_disabled(),
                                on_input: move |value| {
                                    on_config(ConfigurationData {
                                        potion_mode: PotionMode::EveryMillis(value),
                                        ..config_view.peek().clone()
                                    });
                                },
                                value,
                            }
                        },
                        PotionMode::Percentage(value) => rsx! {
                            PercentageInput {
                                label: "Below Health Percentage",
                                div_class: DIV_CLASS,
                                label_class: LABEL_CLASS,
                                input_class: INPUT_CLASS,
                                disabled: is_disabled(),
                                on_input: move |value| {
                                    on_config(ConfigurationData {
                                        potion_mode: PotionMode::Percentage(value),
                                        ..config_view.peek().clone()
                                    });
                                },
                                value,
                            }
                            ConfigMillisInput {
                                label: "Health Update Milliseconds",
                                disabled: is_disabled(),
                                on_input: move |value| {
                                    on_config(ConfigurationData {
                                        health_update_millis: value,
                                        ..config_view.peek().clone()
                                    });
                                },
                                value: config_view().health_update_millis,
                            }
                        },
                    }
                }
                KeyBindingConfigurationInput {
                    label: SAYRAM_ELIXIR,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            sayram_elixir_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().sayram_elixir_key),
                }
                KeyBindingConfigurationInput {
                    label: AURELIA_ELIXIR,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            aurelia_elixir_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().aurelia_elixir_key),
                }
                KeyBindingConfigurationInput {
                    label: EXP_X3,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            exp_x3_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().exp_x3_key),
                }
                KeyBindingConfigurationInput {
                    label: BONUS_EXP,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            bonus_exp_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().bonus_exp_key),
                }
                KeyBindingConfigurationInput {
                    label: LEGION_WEALTH,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            legion_wealth_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().legion_wealth_key),
                }
                KeyBindingConfigurationInput {
                    label: LEGION_LUCK,
                    label_active: active,
                    is_disabled: is_disabled(),
                    is_toggleable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            legion_luck_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().legion_luck_key),
                }
            }
            ConfigHeader { text: "Others", disabled: is_disabled(), class: "mt-2" }
            p { class: "font-normal italic text-xs text-gray-400 mb-1",
                "Class affects only link key timing except Blaster"
            }
            ConfigEnumSelect::<Class> {
                label: "Class",
                on_select: move |class| {
                    on_config(ConfigurationData {
                        class,
                        ..config_view.peek().clone()
                    });
                },
                disabled: is_disabled(),
                selected: config_view().class,
            }
        }
    }
}

#[component]
fn ConfigMillisInput(
    label: String,
    disabled: bool,
    on_input: EventHandler<u64>,
    value: u64,
) -> Element {
    rsx! {
        MillisInput {
            label: "Every Milliseconds",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input,
            value,
        }
    }
}

#[component]
fn ConfigHeader(
    text: String,
    disabled: bool,
    #[props(default = String::new())] class: String,
) -> Element {
    rsx! {
        h2 {
            class: "text-sm font-medium text-gray-700 mb-2 data-[disabled]:text-gray-400 {class}",
            "data-disabled": disabled.then_some(true),
            "{text}"
        }
    }
}

#[component]
fn ConfigEnumSelect<
    T: 'static + Clone + Copy + PartialEq + Display + FromStr + IntoEnumIterator,
>(
    label: String,
    on_select: EventHandler<T>,
    disabled: bool,
    selected: T,
) -> Element {
    rsx! {
        EnumSelect {
            label,
            disabled,
            div_class: "flex items-center space-x-4",
            label_class: "text-xs text-gray-700 flex-1 inline-block data-[disabled]:text-gray-400",
            select_class: "w-44 text-xs text-gray-700 text-ellipsis rounded outline-none disabled:cursor-not-allowed disabled:text-gray-400",
            on_select: move |variant: T| {
                on_select(variant);
            },
            selected,
        }
    }
}

// FIXME: pub for hot_keys
#[component]
pub fn KeyBindingConfigurationInput(
    label: &'static str,
    label_active: Signal<Option<&'static str>>,
    is_disabled: bool,
    #[props(default = false)] is_optional: bool,
    #[props(default = false)] is_toggleable: bool,
    on_input: EventHandler<Option<KeyBindingConfiguration>>,
    value: Option<KeyBindingConfiguration>,
    children: Element,
) -> Element {
    debug_assert!(is_optional || value.is_some());

    let is_active = use_memo(move || label_active() == Some(label));
    let is_enabled = value.map(|key| key.enabled).unwrap_or(true);
    let input_width = if is_toggleable { "w-24" } else { "w-44" };

    rsx! {
        div { class: "flex flex-col space-y-4 py-3 border-b border-gray-100",
            div { class: "flex items-center space-x-4",
                div { class: "flex-1",
                    span {
                        class: "text-xs text-gray-700 data-[disabled]:text-gray-400",
                        "data-disabled": is_disabled.then_some(true),
                        {label}
                        if is_optional {
                            span { class: "text-xs text-gray-400 ml-1", "(Optional)" }
                        }
                    }
                }
                div { class: "flex items-center space-x-2",
                    div { class: "relative",
                        KeyInput {
                            class: "border rounded border-gray-300 h-7 {input_width} disabled:cursor-not-allowed disabled:border-gray-200 disabled:text-gray-400",
                            disabled: is_disabled,
                            is_active: is_active(),
                            on_active: move |is_active: bool| {
                                label_active.set(is_active.then_some(label));
                            },
                            on_input: move |key| {
                                (on_input)(
                                    Some(KeyBindingConfiguration {
                                        key,
                                        ..value.unwrap_or_default()
                                    }),
                                );
                            },
                            value: value.map(|key| key.key),
                        }
                        if is_optional && !is_active() && value.is_some() {
                            button {
                                class: "absolute right-0 top-0 flex items-center h-full w-4",
                                onclick: move |_| {
                                    (on_input)(None);
                                },
                                XIcon { class: "w-2 h-2 text-red-400 fill-current" }
                            }
                        }
                    }
                    if is_toggleable {
                        button {
                            r#type: "button",
                            disabled: is_disabled || value.is_none(),
                            class: {
                                let color = if is_enabled { "button-primary" } else { "button-danger" };
                                format!("w-18 h-7 {color}")
                            },
                            onclick: move |_| {
                                if let Some(config) = value {
                                    (on_input)(
                                        Some(KeyBindingConfiguration {
                                            enabled: !config.enabled,
                                            ..config
                                        }),
                                    );
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
            {children}
        }
    }
}
