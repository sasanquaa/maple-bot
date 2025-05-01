use backend::{AutoMobbing, Bound, RotationMode};
use dioxus::prelude::*;

use crate::{
    input::{Checkbox, KeyBindingInput, MillisInput, NumberInputI32, NumberInputU32},
    select::EnumSelect,
};

const DIV_CLASS: &str = "flex py-2 border-b border-gray-100 space-x-2";
const LABEL_CLASS: &str = "flex-1 text-xs text-gray-700 inline-block data-[disabled]:text-gray-400";
const INPUT_CLASS: &str = "w-28 px-1.5 h-6 border border-gray-300 rounded text-xs text-ellipsis outline-none disabled:text-gray-400 disabled:cursor-not-allowed";

#[component]
pub fn Rotations(
    disabled: bool,
    on_rotation_mode: EventHandler<RotationMode>,
    on_reset_on_erda: EventHandler<bool>,
    rotation_mode: RotationMode,
    reset_on_erda: bool,
) -> Element {
    let auto_mobbing = if let RotationMode::AutoMobbing(mobbing) = rotation_mode {
        mobbing
    } else {
        AutoMobbing::default()
    };

    rsx! {
        div { class: "flex flex-col space-y-2",
            ul { class: "list-disc text-xs text-gray-700 pl-4",
                li { "Other rotation modes apply only to Any condition action" }
                li { "Action in preset with Any condition is ignored when auto mobbing enabled" }
                li {
                    "When reset rotation on Erda condotion is ticked, all Any condition actions will restart from the beginning"
                }
                li { "Mob detected outside of bound is ignored" }
                li { "Auto mobbing X,Y origin is top-left of minimap" }
                li { "Overrides the below bound if auto mobbing bound by platforms enabled" }
            }
            div { class: "h-2 border-b border-gray-300 mb-2" }
            EnumSelect {
                label: "Rotation Mode",
                div_class: DIV_CLASS,
                label_class: LABEL_CLASS,
                select_class: INPUT_CLASS,
                disabled,
                on_select: move |selected: RotationMode| {
                    on_rotation_mode(selected);
                },
                selected: rotation_mode,
            }
            Checkbox {
                label: "Reset Rotation On Erda Condition",
                div_class: DIV_CLASS,
                label_class: LABEL_CLASS,
                input_class: INPUT_CLASS,
                disabled,
                on_input: move |checked| {
                    on_reset_on_erda(checked);
                },
                value: reset_on_erda,
            }
            AutoMobbingInput {
                disabled: disabled || !matches!(rotation_mode, RotationMode::AutoMobbing(_)),
                on_input: move |mobbing| {
                    on_rotation_mode(RotationMode::AutoMobbing(mobbing));
                },
                value: auto_mobbing,
            }
        }
    }
}

#[component]
fn AutoMobbingInput(
    disabled: bool,
    on_input: EventHandler<AutoMobbing>,
    value: AutoMobbing,
) -> Element {
    let AutoMobbing {
        bound,
        key,
        key_count,
        key_wait_before_millis,
        key_wait_after_millis,
    } = value;

    rsx! {
        KeyBindingInput {
            label: "Key",
            label_class: LABEL_CLASS,
            div_class: DIV_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input: move |key| {
                on_input(AutoMobbing { key, ..value });
            },
            value: key,
        }
        NumberInputU32 {
            label: "Key Count",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            minimum_value: 1,
            on_input: move |key_count| {
                on_input(AutoMobbing { key_count, ..value });
            },
            value: key_count,
        }
        MillisInput {
            label: "Key Wait Before",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input: move |key_wait_before_millis| {
                on_input(AutoMobbing {
                    key_wait_before_millis,
                    ..value
                });
            },
            value: key_wait_before_millis,
        }
        MillisInput {
            label: "Key Wait After",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input: move |key_wait_after_millis| {
                on_input(AutoMobbing {
                    key_wait_after_millis,
                    ..value
                });
            },
            value: key_wait_after_millis,
        }
        NumberInputI32 {
            label: "X",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input: move |x| {
                on_input(AutoMobbing {
                    bound: Bound { x, ..bound },
                    ..value
                });
            },
            value: bound.x,
        }
        NumberInputI32 {
            label: "Y",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input: move |y| {
                on_input(AutoMobbing {
                    bound: Bound { y, ..bound },
                    ..value
                });
            },
            value: bound.y,
        }
        NumberInputI32 {
            label: "Width",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input: move |width| {
                on_input(AutoMobbing {
                    bound: Bound { width, ..bound },
                    ..value
                });
            },
            value: bound.width,
        }
        NumberInputI32 {
            label: "Height",
            div_class: DIV_CLASS,
            label_class: LABEL_CLASS,
            input_class: INPUT_CLASS,
            disabled,
            on_input: move |height| {
                on_input(AutoMobbing {
                    bound: Bound { height, ..bound },
                    ..value
                });
            },
            value: bound.height,
        }
    }
}
