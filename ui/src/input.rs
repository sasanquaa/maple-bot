use std::str::FromStr;

use backend::KeyBinding;
use dioxus::prelude::*;

use crate::key::KeyInput;

#[derive(Clone, PartialEq, Props)]
pub struct LabeledInputProps {
    label: String,
    label_class: String,
    div_class: String,
    disabled: bool,
    children: Element,
}

#[component]
pub fn LabeledInput(props: LabeledInputProps) -> Element {
    let data_disabled = props.disabled.then_some(true);

    rsx! {
        div { class: props.div_class, "data-disabled": data_disabled,
            label { class: props.label_class, "data-disabled": data_disabled, {props.label} }
            {props.children}
        }
    }
}

#[derive(Clone, PartialEq, Props)]
pub struct GenericInputProps<T: 'static + Clone + PartialEq> {
    label: String,
    #[props(default = String::default())]
    label_class: String,
    #[props(default = String::default())]
    div_class: String,
    #[props(default = String::default())]
    input_class: String,
    #[props(default = false)]
    disabled: bool,
    on_input: EventHandler<T>,
    value: T,
}

#[component]
pub fn KeyBindingInput(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_input,
        value,
    }: GenericInputProps<Option<KeyBinding>>,
) -> Element {
    let mut is_active = use_signal(|| false);

    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            KeyInput {
                class: input_class,
                disabled,
                is_active: is_active(),
                on_active: move |active| {
                    is_active.set(active);
                },
                on_input,
                value,
            }
        }
    }
}

#[component]
pub fn Checkbox(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_input,
        value,
    }: GenericInputProps<bool>,
) -> Element {
    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            input {
                class: input_class,
                disabled,
                r#type: "checkbox",
                oninput: move |e| {
                    on_input(e.parsed::<bool>().unwrap());
                },
                checked: value,
            }
        }
    }
}

#[component]
pub fn NumberInputU32(props: GenericInputProps<u32>) -> Element {
    rsx! {
        NumberInput { ..props }
    }
}

#[component]
pub fn NumberInputU64(props: GenericInputProps<u64>) -> Element {
    rsx! {
        NumberInput { ..props }
    }
}

#[component]
pub fn NumberInputI32(props: GenericInputProps<i32>) -> Element {
    rsx! {
        NumberInput { ..props }
    }
}

#[component]
fn NumberInput<T: 'static + Copy + Clone + PartialEq + FromStr + IntoAttributeValue>(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_input,
        value,
    }: GenericInputProps<T>,
) -> Element {
    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            input {
                disabled,
                r#type: "number",
                class: input_class,
                min: "0",
                onchange: move |e| {
                    on_input(e.parsed::<T>().unwrap_or(value));
                },
                value,
            }
        }
    }
}
