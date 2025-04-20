use backend::{CaptureMode, KeyBindingConfiguration, Settings as SettingsData};
use dioxus::prelude::*;

use crate::{AppMessage, configuration::ConfigEnumSelect, key::KeyBindingConfigurationInput};

const TOGGLE_ACTIONS: &str = "Start/Stop Actions";
const PLATFORM_START: &str = "Mark Platform Start";
const PLATFORM_END: &str = "Mark Platform End";
const PLATFORM_ADD: &str = "Add Platform";

#[component]
pub fn Settings(
    app_coroutine: Coroutine<AppMessage>,
    settings: ReadOnlySignal<Option<SettingsData>>,
) -> Element {
    let settings_view = use_memo(move || settings().unwrap_or_default());
    let active = use_signal(|| None);
    let on_settings = move |updated| {
        app_coroutine.send(AppMessage::UpdateSettings(updated));
    };

    rsx! {
        div { class: "px-2 pb-2 pt-2 flex flex-col overflow-y-auto scrollbar h-full",
            ul { class: "list-disc text-xs text-gray-700 pl-4",
                li { class: "mb-1", "Platform keys must have a Map created and Platforms tab opened" }
                li { class: "mb-1", "BltBltArea can stay behind other windows but cannot be minimized" }
                li { class: "mb-1 font-bold",
                    "BitBltArea relies on high-quality game images for detection (e.g. no blurry)"
                }
                li { class: "mb-1 font-bold",
                    "When using BitBltArea, make sure the window on top of the capture area is the game or where the game images can be captured if the game is inside a something else (e.g. VM)"
                }
                li { class: "mb-1 font-bold",
                    "When using BitBltArea, the game must be contained inside the capture area even when resizing (e.g. going to cash shop)"
                }
                li { class: "mb-1 font-bold",
                    "When using BitBltArea, for key inputs to work, make sure the window on top of the capture area is focused by clicking it. For example, if you have Notepad on top of the game and focused, it will send input to the Notepad instead of the game."
                }
            }
            div { class: "h-2 border-b border-gray-300 mb-2" }
            ConfigEnumSelect::<CaptureMode> {
                label: "Capture Mode",
                on_select: move |capture_mode| {
                    on_settings(SettingsData {
                        capture_mode,
                        ..settings_view.peek().clone()
                    });
                },
                disabled: false,
                selected: settings_view().capture_mode,
            }
            KeyBindingConfigurationInput {
                label: TOGGLE_ACTIONS,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_settings(SettingsData {
                        toggle_actions_key: key.unwrap(),
                        ..settings_view.peek().clone()
                    });
                },
                value: Some(settings_view().toggle_actions_key),
            }
            KeyBindingConfigurationInput {
                label: PLATFORM_START,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_settings(SettingsData {
                        platform_start_key: key.unwrap(),
                        ..settings_view.peek().clone()
                    });
                },
                value: Some(settings_view().platform_start_key),
            }
            KeyBindingConfigurationInput {
                label: PLATFORM_END,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_settings(SettingsData {
                        platform_end_key: key.unwrap(),
                        ..settings_view.peek().clone()
                    });
                },
                value: Some(settings_view().platform_end_key),
            }
            KeyBindingConfigurationInput {
                label: PLATFORM_ADD,
                label_active: active,
                is_toggleable: true,
                is_disabled: false,
                on_input: move |key: Option<KeyBindingConfiguration>| {
                    on_settings(SettingsData {
                        platform_add_key: key.unwrap(),
                        ..settings_view.peek().clone()
                    });
                },
                value: Some(settings_view().platform_add_key),
            }
        }
    }
}
