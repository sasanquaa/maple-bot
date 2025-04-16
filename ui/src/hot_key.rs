use backend::{HotKeys as HotKeysData, KeyBindingConfiguration};
use dioxus::prelude::*;

use crate::{AppMessage, configuration::KeyBindingConfigurationInput};

const TOGGLE_ACTIONS: &str = "Start/Stop Actions";
const PLATFORM_START: &str = "Mark Platform Start";
const PLATFORM_END: &str = "Mark Platform End";
const PLATFORM_ADD: &str = "Add Platform";

#[component]
pub fn HotKeys(
    app_coroutine: Coroutine<AppMessage>,
    hot_keys: ReadOnlySignal<Option<HotKeysData>>,
) -> Element {
    let hot_keys_view = use_memo(move || hot_keys().unwrap_or_default());
    let active = use_signal(|| None);
    let on_hot_keys = move |updated| {
        app_coroutine.send(AppMessage::UpdateHotKeys(updated));
    };

    rsx! {
        div { class: "px-2 pb-2 pt-2 flex flex-col overflow-y-auto scrollbar h-full",
            p { class: "font-normal italic text-xs text-gray-400 mb-1",
                "Platform keys must have a Map created and Platforms tab opened"
            }
            KeyBindingConfigurationInput {
                label: TOGGLE_ACTIONS,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_hot_keys(HotKeysData {
                        toggle_actions_key: key.unwrap(),
                        ..hot_keys_view.peek().clone()
                    });
                },
                value: Some(hot_keys_view().toggle_actions_key),
            }
            KeyBindingConfigurationInput {
                label: PLATFORM_START,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_hot_keys(HotKeysData {
                        platform_start_key: key.unwrap(),
                        ..hot_keys_view.peek().clone()
                    });
                },
                value: Some(hot_keys_view().platform_start_key),
            }
            KeyBindingConfigurationInput {
                label: PLATFORM_END,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_hot_keys(HotKeysData {
                        platform_end_key: key.unwrap(),
                        ..hot_keys_view.peek().clone()
                    });
                },
                value: Some(hot_keys_view().platform_end_key),
            }
            KeyBindingConfigurationInput {
                label: PLATFORM_ADD,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_hot_keys(HotKeysData {
                        platform_add_key: key.unwrap(),
                        ..hot_keys_view.peek().clone()
                    });
                },
                value: Some(hot_keys_view().platform_add_key),
            }
        }
    }
}
