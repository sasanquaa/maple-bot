use std::{fmt::Display, str::FromStr};

use backend::KeyBinding;
use dioxus::{document::EvalError, prelude::*};
use num_traits::PrimInt;
use rand::distr::{Alphanumeric, SampleString};

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
    }: GenericInputProps<KeyBinding>,
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
pub fn MillisInput(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_input,
        value,
    }: GenericInputProps<u64>,
) -> Element {
    rsx! {
        NumberInput {
            label,
            label_class,
            div_class,
            input_class,
            disabled,
            on_input,
            value,
            suffix: "ms",
        }
    }
}

// FIXME: Please fix this god-tier UI for me. SOMEONE! ANYONE! PLEASE! HLEP MEM!
#[component]
pub fn PercentageInput(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_input,
        value,
    }: GenericInputProps<f32>,
) -> Element {
    let input_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));

    use_effect(move || {
        spawn(async move {
            let js = format!(
                r#"
                const input = document.getElementById("{}");
                console.log(input);
                if (input === null) {{
                    return;
                }}
                const autoNumeric = new AutoNumeric(input, {{
                    allowDecimalPadding: false,
                    emptyInputBehavior: "zero",
                    maximumValue: "100",
                    minimumValue: "0",
                    suffixText: "%",
                    defaultValueOverride: "{}"
                }});
                input.addEventListener("autoNumeric:rawValueModified", async (e) => {{
                    await dioxus.send(e.detail.newRawValue);
                }});
                input.addEventListener("autoNumeric:set", (e) => {{
                    autoNumeric.set(e.detail.value);
                }});
                "#,
                input_id(),
                value
            );
            let mut input = document::eval(js.as_str());
            loop {
                let value = input.recv::<String>().await;
                if let Err(EvalError::Finished) = value {
                    input = document::eval(js.as_str());
                    continue;
                };
                let Ok(value) = value.unwrap().parse::<f32>() else {
                    continue;
                };
                on_input(value);
            }
        });
    });
    use_effect(use_reactive!(|value| {
        document::eval(
            format!(
                r#"
                const input = document.getElementById("{}");
                input.dispatchEvent(new CustomEvent("autoNumeric:set", {{
                    detail: {{
                        value: {}
                    }}
                }}));
                "#,
                input_id(),
                value
            )
            .as_str(),
        );
    }));
    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            input { id: input_id(), disabled, class: input_class }
        }
    }
}

#[component]
pub fn NumberInputI32(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_input,
        value,
    }: GenericInputProps<i32>,
) -> Element {
    rsx! {
        NumberInput {
            label,
            label_class,
            div_class,
            input_class,
            disabled,
            on_input,
            value,
        }
    }
}

#[component]
fn NumberInput<T: 'static + IntoAttributeValue + PrimInt + FromStr + Display>(
    label: String,
    #[props(default = String::default())] label_class: String,
    #[props(default = String::default())] div_class: String,
    #[props(default = String::default())] input_class: String,
    #[props(default = String::default())] suffix: String,
    #[props(default = false)] disabled: bool,
    on_input: EventHandler<T>,
    value: T,
) -> Element {
    let input_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));

    use_effect(move || {
        let suffix = suffix.clone();
        spawn(async move {
            let js = format!(
                r#"
                const input = document.getElementById("{}");
                if (input === null) {{
                    return;
                }}
                const autoNumeric = new AutoNumeric(input, {{
                    decimalPlaces: 0,
                    emptyInputBehavior: "zero",
                    maximumValue: "{}",
                    minimumValue: "0",
                    suffixText: "{}",
                    defaultValueOverride: "{}"
                }});
                input.addEventListener("autoNumeric:rawValueModified", async (e) => {{
                    await dioxus.send(e.detail.newRawValue);
                }});
                input.addEventListener("autoNumeric:set", (e) => {{
                    autoNumeric.set(e.detail.value);
                }});
                "#,
                input_id(),
                T::max_value(),
                suffix,
                value
            );
            let mut input = document::eval(js.as_str());
            loop {
                let value = input.recv::<String>().await;
                if let Err(EvalError::Finished) = value {
                    input = document::eval(js.as_str());
                    continue;
                };
                let Ok(value) = value.unwrap().parse::<T>() else {
                    continue;
                };
                on_input(value);
            }
        });
    });
    use_effect(use_reactive!(|value| {
        document::eval(
            format!(
                r#"
                const input = document.getElementById("{}");
                input.dispatchEvent(new CustomEvent("autoNumeric:set", {{
                    detail: {{
                        value: {}
                    }}
                }}));
                "#,
                input_id(),
                value
            )
            .as_str(),
        );
    }));

    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            input { id: input_id(), disabled, class: input_class }
        }
    }
}
