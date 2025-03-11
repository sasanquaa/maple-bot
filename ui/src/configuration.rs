use std::str::FromStr;

use backend::{
    Configuration as ConfigurationData, IntoEnumIterator, KeyBinding, KeyBindingConfiguration,
    RotationMode, query_configs, upsert_config,
};
use dioxus::prelude::*;
use tokio::task::spawn_blocking;

use crate::{
    icons::XMark,
    input::MillisInput,
    key::KeyInput,
    select::{Select, TextSelect},
};

const ROPE_LIFT: &str = "Rope Lift";
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
    configs: SyncSignal<Vec<ConfigurationData>>,
    config: SyncSignal<Option<ConfigurationData>>,
) -> Element {
    let config_names =
        use_memo(move || configs().iter().map(|config| config.name.clone()).collect());
    let config_view = use_memo(move || {
        config().unwrap_or(ConfigurationData {
            feed_pet_key: KeyBindingConfiguration {
                millis: 120000,
                ..KeyBindingConfiguration::default()
            },
            potion_key: KeyBindingConfiguration {
                millis: 120000,
                ..KeyBindingConfiguration::default()
            },
            ..ConfigurationData::default()
        })
    });
    let mut active = use_signal(|| None);
    let on_config = use_callback(move |new_config: ConfigurationData| {
        if config.peek().is_some() {
            let id = new_config.id;
            if id.is_none() {
                config.set(None);
            } else {
                config.set(Some(new_config.clone()));
                *configs
                    .write()
                    .iter_mut()
                    .find(|config| config.id == id)
                    .unwrap() = new_config.clone();
            }
            spawn(async move {
                spawn_blocking(move || {
                    let mut new_config = new_config;
                    upsert_config(&mut new_config).unwrap();
                    if id.is_none() {
                        config.set(Some(new_config.clone()));
                        configs.write().push(new_config.clone());
                    }
                })
                .await
                .unwrap();
            });
        }
    });
    let disabled = use_memo(move || config().is_none());
    let data_disabled = disabled.peek().then_some(true);

    use_future(move || async move {
        if config.peek().is_none() {
            let result = spawn_blocking(|| query_configs().unwrap()).await.unwrap();
            config.set(result.first().cloned());
            configs.set(result);
        }
    });

    rsx! {
        div { class: "flex flex-col",
            TextSelect {
                on_create: move |created: String| {
                    on_config(ConfigurationData {
                        name: created,
                        ..ConfigurationData::default()
                    });
                },
                disabled: disabled(),
                on_select: move |selected| {
                    config
                        .set(configs.peek().iter().find(|config| config.name == selected).cloned());
                },
                options: config_names(),
                selected: config_view().name,
            }
            h2 {
                class: "text-sm font-medium text-gray-700 mb-2 data-[disabled]:text-gray-400",
                "data-disabled": data_disabled,
                "Key Bindings"
            }
            div { class: "space-y-1",
                KeyBindingConfigurationInput {
                    label: ROPE_LIFT,
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(ROPE_LIFT)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(ROPE_LIFT));
                    },
                    can_disable: false,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            ropelift_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().ropelift_key),
                }
                KeyBindingConfigurationInput {
                    label: UP_JUMP,
                    input_disabled: disabled(),
                    is_optional: true,
                    is_active: matches!(active(), Some(UP_JUMP)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(UP_JUMP));
                    },
                    can_disable: false,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(INTERACT)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(INTERACT));
                    },
                    can_disable: false,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(CASH_SHOP)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(CASH_SHOP));
                    },
                    can_disable: false,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(FEED_PET)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(FEED_PET));
                    },
                    can_disable: true,
                    has_millis: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            feed_pet_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().feed_pet_key),
                }
                KeyBindingConfigurationInput {
                    label: POTION,
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(POTION)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(POTION));
                    },
                    can_disable: true,
                    has_millis: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            potion_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().potion_key),
                }
                KeyBindingConfigurationInput {
                    label: SAYRAM_ELIXIR,
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(SAYRAM_ELIXIR)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(SAYRAM_ELIXIR));
                    },
                    can_disable: true,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(AURELIA_ELIXIR)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(AURELIA_ELIXIR));
                    },
                    can_disable: true,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(EXP_X3)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(EXP_X3));
                    },
                    can_disable: true,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(BONUS_EXP)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(BONUS_EXP));
                    },
                    can_disable: true,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(LEGION_WEALTH)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(LEGION_WEALTH));
                    },
                    can_disable: true,
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
                    input_disabled: disabled(),
                    is_optional: false,
                    is_active: matches!(active(), Some(LEGION_LUCK)),
                    on_active: move |value: bool| {
                        active.set(value.then_some(LEGION_LUCK));
                    },
                    can_disable: true,
                    on_input: move |key: Option<KeyBindingConfiguration>| {
                        on_config(ConfigurationData {
                            legion_luck_key: key.unwrap(),
                            ..config_view.peek().clone()
                        });
                    },
                    value: Some(config_view().legion_luck_key),
                }
            }
            h2 {
                class: "text-sm font-medium text-gray-700 mt-4 mb-2 data-[disabled]:text-gray-400",
                "data-disabled": data_disabled,
                "Other"
                span { class: "font-normal text-xs text-gray-400 ml-1",
                    "(Only for Any condition actions)"
                }
            }
            RotationModeInput {
                on_input: move |mode| {
                    on_config(ConfigurationData {
                        rotation_mode: mode,
                        ..config_view.peek().clone()
                    });
                },
                disabled: disabled(),
                value: config_view().rotation_mode,
            }
        }
    }
}

#[component]
fn RotationModeInput(
    on_input: EventHandler<RotationMode>,
    disabled: bool,
    value: RotationMode,
) -> Element {
    let options = RotationMode::iter()
        .map(|variant| (variant.to_string(), variant.to_string()))
        .collect::<Vec<_>>();
    let selected = value.to_string();

    rsx! {
        Select {
            label: "Rotation Mode",
            disabled,
            div_class: "flex items-center space-x-4",
            label_class: "text-xs text-gray-700 flex-1 inline-block data-[disabled]:text-gray-400",
            select_class: "w-44 text-xs text-gray-700 text-ellipsis rounded outline-none disabled:cursor-not-allowed disabled:text-gray-400",
            options,
            on_select: move |variant: String| {
                on_input(RotationMode::from_str(variant.as_str()).unwrap());
            },
            selected,
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct KeyBindingConfigurationInputProps {
    label: String,
    input_disabled: bool,
    is_optional: bool,
    is_active: bool,
    on_active: EventHandler<bool>,
    can_disable: bool,
    #[props(default = false)]
    has_millis: bool,
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
    let on_key_input = use_callback(move |key: Option<KeyBinding>| {
        (props.on_input)(key.map(|key| KeyBindingConfiguration {
            key,
            ..props.value.unwrap_or_default()
        }));
    });
    let on_millis_input = use_callback(move |millis: u64| {
        (props.on_input)(
            props
                .value
                .map(|config| KeyBindingConfiguration { millis, ..config }),
        );
    });
    let input_width = if props.can_disable { "w-24" } else { "w-44" };

    rsx! {
        div { class: "flex flex-col space-y-3 py-2 border-b border-gray-100",
            div { class: "flex items-center space-x-4",
                div { class: "flex-1",
                    span {
                        class: "text-xs text-gray-700 data-[disabled]:text-gray-400",
                        "data-disabled": props.input_disabled.then_some(true),
                        {props.label}
                        if props.is_optional {
                            span { class: "text-xs text-gray-400 ml-1", "(Optional)" }
                        }
                    }
                }
                div { class: "flex items-center space-x-2",
                    div { class: "relative",
                        KeyInput {
                            class: "border rounded border-gray-300 h-7 {input_width} disabled:cursor-not-allowed disabled:border-gray-200 disabled:text-gray-400",
                            disabled: props.input_disabled,
                            is_active: props.is_active,
                            on_active: props.on_active,
                            on_input: move |key| {
                                on_key_input(Some(key));
                            },
                            value: props.value.map(|key| key.key),
                        }
                        if props.is_optional && !props.is_active && props.value.is_some() {
                            button {
                                class: "absolute right-0 top-0 flex items-center h-full w-4",
                                onclick: move |_| {
                                    on_key_input(None);
                                },
                                XMark { class: "w-2 h-2 text-red-400 fill-current" }
                            }
                        }
                    }
                    if props.can_disable {
                        button {
                            r#type: "button",
                            disabled: props.input_disabled || props.value.is_none(),
                            class: {
                                let color = if is_enabled { "button-primary" } else { "button-danger" };
                                format!("w-18 h-7 {color}")
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
            if props.has_millis {
                MillisInput {
                    label: "Every Milliseconds",
                    div_class: "flex items-center space-x-4 text-xs text-gray-700",
                    label_class: "flex-1",
                    input_class: "w-44 rounded border border-gray-300 px-1 text-gray-700 h-6 outline-none",
                    on_input: move |value| {
                        on_millis_input(value);
                    },
                    value: props.value.map(|key| key.millis).unwrap_or_default(),
                }
            }
        }
    }
}
