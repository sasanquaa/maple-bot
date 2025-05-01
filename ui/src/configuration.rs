use std::{fmt::Display, str::FromStr};

use backend::{
    ActionConfiguration, Class, Configuration as ConfigurationData, IntoEnumIterator,
    KeyBindingConfiguration, PotionMode,
};
use dioxus::prelude::*;
use rand::distr::{Alphanumeric, SampleString};

use crate::{
    AppMessage,
    icons::{CheckMarkIcon, XIcon},
    input::{Checkbox, KeyBindingInput, MillisInput, PercentageInput, use_auto_numeric},
    key::KeyBindingConfigurationInput,
    select::{EnumSelect, TextSelect},
    tab::Tab,
};

const DIV_CLASS: &str = "flex items-center space-x-4 text-xs text-gray-700";
const LABEL_CLASS: &str = "flex-1";
const INPUT_CLASS: &str = "w-44 rounded border border-gray-300 px-1 text-gray-700 h-6 outline-none";
const ROPE_LIFT: &str = "Rope Lift";
const TELEPORT: &str = "Teleport";
const JUMP: &str = "Jump";
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
const WEALTH_ACQUISITION_POTION: &str = "Wealth Acquisition Potion";
const EXP_ACCUMULATION_POTION: &str = "Exp Accumulation Potion";
const EXTREME_RED_POTION: &str = "Extreme Red Potion";
const EXTREME_BLUE_POTION: &str = "Extreme Blue Potion";
const EXTREME_GREEN_POTION: &str = "Extreme Green Potion";
const EXTREME_GOLD_POTION: &str = "Extreme Gold Potion";
const TAB_GAME: &str = "Game";
const TAB_BUFFS: &str = "Buffs";
const TAB_FIXED_ACTIONS: &str = "Fixed Actions";

#[component]
pub fn Configuration(
    app_coroutine: Coroutine<AppMessage>,
    configs: ReadOnlySignal<Option<Vec<ConfigurationData>>>,
    config: ReadOnlySignal<Option<ConfigurationData>>,
) -> Element {
    let mut active_tab = use_signal(|| TAB_GAME.to_string());
    let is_disabled = use_memo(move || config().is_none());
    let active = use_signal(|| None);
    let config_names = use_memo(move || {
        configs()
            .map(|configs| {
                configs
                    .into_iter()
                    .map(|config| config.name.clone())
                    .collect()
            })
            .unwrap_or_default()
    });
    let config_view = use_memo(move || config().unwrap_or_default());
    let on_config = move |new_config: ConfigurationData| {
        app_coroutine.send(AppMessage::UpdateConfig(new_config, true));
    };

    rsx! {
        Tab {
            tabs: vec![TAB_GAME.to_string(), TAB_BUFFS.to_string(), TAB_FIXED_ACTIONS.to_string()],
            div_class: "px-2 pt-2 pb-1",
            class: "text-xs px-2 pb-2 focus:outline-none",
            selected_class: "text-gray-800 border-b",
            unselected_class: "hover:text-gray-700 text-gray-400",
            on_tab: move |tab| {
                active_tab.set(tab);
            },
            tab: active_tab(),
        }
        div { class: "px-2 flex flex-col overflow-y-auto scrollbar h-full",
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
                    let msg = AppMessage::UpdateConfig(
                        configs.peek().as_ref().unwrap().get(i).cloned().unwrap(),
                        false,
                    );
                    app_coroutine.send(msg);
                },
                options: config_names(),
                selected: config_view().name,
            }
            div { class: "pb-2 overflow-y-auto scrollbar h-full",
                match active_tab().as_str() {
                    TAB_GAME => rsx! {
                        ConfigGameKeyBindings {
                            active,
                            is_disabled,
                            config_view,
                            on_config,
                        }
                    },
                    TAB_BUFFS => rsx! {
                        ConfigBuffKeyBindings {
                            active,
                            is_disabled,
                            config_view,
                            on_config,
                        }
                    },
                    TAB_FIXED_ACTIONS => rsx! {
                        ConfigFixedActions {
                            active,
                            is_disabled,
                            config_view,
                            on_config,
                        }
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
}

#[component]
fn ConfigGameKeyBindings(
    active: Signal<Option<&'static str>>,
    is_disabled: Memo<bool>,
    config_view: Memo<ConfigurationData>,
    on_config: EventHandler<ConfigurationData>,
) -> Element {
    rsx! {
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
                label: JUMP,
                label_active: active,
                is_disabled: is_disabled(),
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_config(ConfigurationData {
                        jump_key: key.unwrap(),
                        ..config_view.peek().clone()
                    });
                },
                value: config_view().jump_key,
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
            div { class: "space-y-2",
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
}

#[component]
fn ConfigBuffKeyBindings(
    active: Signal<Option<&'static str>>,
    is_disabled: Memo<bool>,
    config_view: Memo<ConfigurationData>,
    on_config: EventHandler<ConfigurationData>,
) -> Element {
    rsx! {
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
        KeyBindingConfigurationInput {
            label: WEALTH_ACQUISITION_POTION,
            label_active: active,
            is_disabled: is_disabled(),
            is_toggleable: true,
            on_input: move |key: Option<KeyBindingConfiguration>| {
                on_config(ConfigurationData {
                    wealth_acquisition_potion_key: key.unwrap(),
                    ..config_view.peek().clone()
                });
            },
            value: Some(config_view().wealth_acquisition_potion_key),
        }
        KeyBindingConfigurationInput {
            label: EXP_ACCUMULATION_POTION,
            label_active: active,
            is_disabled: is_disabled(),
            is_toggleable: true,
            on_input: move |key: Option<KeyBindingConfiguration>| {
                on_config(ConfigurationData {
                    exp_accumulation_potion_key: key.unwrap(),
                    ..config_view.peek().clone()
                });
            },
            value: Some(config_view().exp_accumulation_potion_key),
        }
        KeyBindingConfigurationInput {
            label: EXTREME_RED_POTION,
            label_active: active,
            is_disabled: is_disabled(),
            is_toggleable: true,
            on_input: move |key: Option<KeyBindingConfiguration>| {
                on_config(ConfigurationData {
                    extreme_red_potion_key: key.unwrap(),
                    ..config_view.peek().clone()
                });
            },
            value: Some(config_view().extreme_red_potion_key),
        }
        KeyBindingConfigurationInput {
            label: EXTREME_BLUE_POTION,
            label_active: active,
            is_disabled: is_disabled(),
            is_toggleable: true,
            on_input: move |key: Option<KeyBindingConfiguration>| {
                on_config(ConfigurationData {
                    extreme_blue_potion_key: key.unwrap(),
                    ..config_view.peek().clone()
                });
            },
            value: Some(config_view().extreme_blue_potion_key),
        }
        KeyBindingConfigurationInput {
            label: EXTREME_GREEN_POTION,
            label_active: active,
            is_disabled: is_disabled(),
            is_toggleable: true,
            on_input: move |key: Option<KeyBindingConfiguration>| {
                on_config(ConfigurationData {
                    extreme_green_potion_key: key.unwrap(),
                    ..config_view.peek().clone()
                });
            },
            value: Some(config_view().extreme_green_potion_key),
        }
        KeyBindingConfigurationInput {
            label: EXTREME_GOLD_POTION,
            label_active: active,
            is_disabled: is_disabled(),
            is_toggleable: true,
            on_input: move |key: Option<KeyBindingConfiguration>| {
                on_config(ConfigurationData {
                    extreme_gold_potion_key: key.unwrap(),
                    ..config_view.peek().clone()
                });
            },
            value: Some(config_view().extreme_gold_potion_key),
        }
    }
}

#[component]
fn ConfigFixedActions(
    active: Signal<Option<&'static str>>,
    is_disabled: Memo<bool>,
    config_view: Memo<ConfigurationData>,
    on_config: EventHandler<ConfigurationData>,
) -> Element {
    let mut editing_action = use_signal(ActionConfiguration::default);
    let mut editing_action_index = use_signal(|| None);
    let actions_view = use_memo(move || config_view().actions);

    use_effect(move || {
        if let Some(index) = editing_action_index() {
            if let Some(action) = actions_view.peek().get(index) {
                editing_action.set(*action);
            }
        }
    });

    rsx! {
        div { class: "flex flex-col space-y-2",
            ConfigFixedActionInput {
                is_disabled,
                on_input: move |action| {
                    editing_action.set(action);
                },
                value: editing_action(),
            }
            div { class: "mt-2 flex space-x-2",
                if editing_action_index().is_some() {
                    button {
                        class: "w-1/2 button-primary h-6",
                        disabled: is_disabled(),
                        onclick: move |_| {
                            if let Some(index) = editing_action_index.take() {
                                let mut actions = actions_view.peek().clone();
                                *actions.get_mut(index).unwrap() = *editing_action.peek();
                                on_config(ConfigurationData {
                                    actions,
                                    ..config_view()
                                });
                            }
                        },
                        "Save"
                    }
                    button {
                        class: "w-1/2 button-secondary h-6",
                        disabled: is_disabled(),
                        onclick: move |_| {
                            editing_action_index.set(None);
                        },
                        "Cancel"
                    }
                } else {
                    button {
                        class: "flex-1 button-primary h-6",
                        disabled: is_disabled(),
                        onclick: move |_| {
                            let mut actions = actions_view.peek().clone();
                            actions.push(*editing_action.peek());
                            on_config(ConfigurationData {
                                actions,
                                ..config_view()
                            });
                        },
                        "Add action"
                    }
                }
            }
            for (i , action) in actions_view().into_iter().enumerate() {
                ConfigFixedActionCard {
                    index: i,
                    is_disabled,
                    action,
                    on_delete: move |_| {
                        let mut actions = actions_view.peek().clone();
                        actions.remove(i);
                        on_config(ConfigurationData {
                            actions,
                            ..config_view()
                        });
                    },
                    on_toggle: move |enabled| {
                        let mut actions = actions_view.peek().clone();
                        actions.get_mut(i).unwrap().enabled = enabled;
                        on_config(ConfigurationData {
                            actions,
                            ..config_view()
                        });
                    },
                    on_click: move |i| {
                        editing_action_index.set(Some(i));
                    },
                }
            }
        }
    }
}

#[component]
fn ConfigFixedActionCard(
    index: usize,
    is_disabled: Memo<bool>,
    action: ActionConfiguration,
    on_delete: EventHandler,
    on_toggle: EventHandler<bool>,
    on_click: EventHandler<usize>,
) -> Element {
    const KEY: &str = "font-mono w-1/2 text-xs";
    const VALUE: &str = "font-mono text-xs w-24 overflow-hidden text-ellipsis";
    const DIV: &str = "flex items-center space-x-1";

    let ActionConfiguration {
        key,
        every_millis,
        require_stationary,
        wait_before_use_millis,
        wait_after_use_millis,
        enabled,
    } = action;
    let every_millis_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
    let wait_before_use_millis_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
    let wait_after_use_millis_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));

    use_auto_numeric(
        every_millis_id,
        every_millis.to_string(),
        None,
        "0".to_string(),
        u64::MAX.to_string(),
        "ms".to_string(),
    );
    use_auto_numeric(
        wait_before_use_millis_id,
        wait_before_use_millis.to_string(),
        None,
        "0".to_string(),
        u64::MAX.to_string(),
        "ms".to_string(),
    );
    use_auto_numeric(
        wait_after_use_millis_id,
        wait_after_use_millis.to_string(),
        None,
        "0".to_string(),
        u64::MAX.to_string(),
        "ms".to_string(),
    );
    let enabled_text_color = if enabled {
        "text-blue-700"
    } else {
        "text-gray-500"
    };
    let enabled_border_color = if enabled {
        "border-blue-700"
    } else {
        "border-gray-500"
    };

    rsx! {
        div {
            class: "relative flex flex-col p-1 space-y-1 border-l-2 border-gray-300 rounded",
            onclick: move |_| {
                on_click(index);
            },
            div { class: DIV,
                span { class: KEY, "Key" }
                span { class: VALUE, {key.to_string()} }
            }
            div { class: DIV,
                span { class: KEY, "Every milliseconds" }
                span { id: every_millis_id(), class: VALUE, {every_millis.to_string()} }
            }
            div { class: DIV,
                span { class: KEY, "Require stationary" }
                span { class: VALUE, {require_stationary.to_string()} }
            }
            div { class: DIV,
                span { class: KEY, "Wait before" }
                span { id: wait_before_use_millis_id(), class: VALUE,
                    {wait_before_use_millis.to_string()}
                }
            }
            div { class: DIV,
                span { class: KEY, "Wait after" }
                span { id: wait_after_use_millis_id(), class: VALUE,
                    {wait_after_use_millis.to_string()}
                }
            }
            div { class: "absolute right-3 top-1 flex space-x-2 flex-1 justify-center",
                button {
                    class: "w-5 h-5 border {enabled_border_color} p-1",
                    disabled: is_disabled(),
                    onclick: move |e| {
                        e.stop_propagation();
                        on_toggle(!enabled);
                    },
                    CheckMarkIcon { class: "w-full h-full {enabled_text_color} fill-current" }
                }
                button {
                    class: "w-5 h-5 border border-red-500 p-1",
                    disabled: is_disabled(),
                    onclick: move |e| {
                        e.stop_propagation();
                        on_delete(());
                    },
                    XIcon { class: "w-full h-full text-red-400 fill-current" }
                }
            }
        }
    }
}

#[component]
fn ConfigFixedActionInput(
    is_disabled: Memo<bool>,
    on_input: EventHandler<ActionConfiguration>,
    value: ActionConfiguration,
) -> Element {
    rsx! {
        div { class: "flex flex-col space-y-3",
            KeyBindingInput {
                label: "Key",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: INPUT_CLASS,
                disabled: is_disabled(),
                on_input: move |key| {
                    (on_input)(ActionConfiguration {
                        key,
                        ..value
                    })
                },
                value: value.key,
            }
            ConfigMillisInput {
                label: "Every milliseconds",
                disabled: is_disabled(),
                on_input: move |every_millis| {
                    (on_input)(ActionConfiguration {
                        every_millis,
                        ..value
                    })
                },
                value: value.every_millis,
            }
            Checkbox {
                label: "Require stationary",
                label_class: LABEL_CLASS,
                div_class: DIV_CLASS,
                input_class: "w-44",
                disabled: is_disabled(),
                on_input: move |require_stationary| {
                    (on_input)(ActionConfiguration {
                        require_stationary,
                        ..value
                    })
                },
                value: value.require_stationary,
            }
            ConfigMillisInput {
                label: "Wait before action",
                disabled: is_disabled(),
                on_input: move |wait_before_use_millis| {
                    (on_input)(ActionConfiguration {
                        wait_before_use_millis,
                        ..value
                    })
                },
                value: value.wait_before_use_millis,
            }
            ConfigMillisInput {
                label: "Wait after action",
                disabled: is_disabled(),
                on_input: move |wait_after_use_millis| {
                    (on_input)(ActionConfiguration {
                        wait_after_use_millis,
                        ..value
                    })
                },
                value: value.wait_after_use_millis,
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
            label,
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
fn ConfigEnumSelect<T: 'static + Clone + PartialEq + Display + FromStr + IntoEnumIterator>(
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
            select_class: "w-44 h-7 text-xs text-gray-700 text-ellipsis border border-gray-300 rounded outline-none disabled:cursor-not-allowed disabled:text-gray-400",
            on_select: move |variant: T| {
                on_select(variant);
            },
            selected,
        }
    }
}
