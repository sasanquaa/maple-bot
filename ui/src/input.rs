use std::{fmt::Display, str::FromStr, time::Duration};

use backend::KeyBinding;
use dioxus::{document::EvalError, prelude::*};
use num_traits::PrimInt;
use rand::distr::{Alphanumeric, SampleString};

use crate::key::KeyInput;

pub fn use_auto_numeric(
    id: Memo<String>,
    value: String,
    on_value: Option<EventHandler<String>>,
    maximum_value: String,
    suffix: String,
) {
    let has_input = on_value.is_some();
    let value = use_memo(use_reactive!(|value| value));
    let maximum_value = use_memo(move || maximum_value.clone());
    let suffix = use_memo(move || suffix.clone());

    // I am had enough, this stuff way too hard
    use_future(move || async move {
        let js = format!(
            r#"
            const element = document.getElementById("{}");
            const hasInput = {};
            const autoNumeric = new AutoNumeric(element, {{
                allowDecimalPadding: false,
                emptyInputBehavior: "zero",
                maximumValue: "{}",
                minimumValue: "0",
                suffixText: "{}",
                defaultValueOverride: "{}"
            }});
            if (hasInput) {{
                let ignoreInitial = true;
                element.addEventListener("autoNumeric:rawValueModified", async (e) => {{
                    if (ignoreInitial) {{
                        ignoreInitial = false;
                        return;
                    }} 
                    await dioxus.send(e.detail.newRawValue);
                }});
            }}
            element.addEventListener("autoNumeric:set", (e) => {{
                autoNumeric.set(e.detail.value);
            }});
            "#,
            id(),
            has_input,
            maximum_value(),
            suffix(),
            value()
        );
        let mut element = document::eval(js.as_str());
        let mut task = None::<Task>;
        use_effect(move || {
            let js = format!(
                r#"
                const element = document.getElementById("{}");
                element.dispatchEvent(new CustomEvent("autoNumeric:set", {{
                    detail: {{
                        value: {}
                    }}
                }}));
                "#,
                id, value
            );
            document::eval(js.as_str());
        });
        loop {
            let value = element.recv::<String>().await;
            if let Err(EvalError::Finished) = value {
                element = document::eval(js.as_str());
                continue;
            };
            if let Some(task) = task {
                task.cancel();
            }
            if let Some(on_value) = on_value {
                task = Some(spawn(async move {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    on_value(value.unwrap());
                }));
            }
        }
    });
}

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
            div { class: input_class,
                input {
                    class: "appearance-none h-4 w-4 border border-gray-300 rounded checked:bg-gray-400",
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
        PrimIntInput {
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

// FIXME: :smiling-doge:
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
    use_auto_numeric(
        input_id,
        value.to_string(),
        Some(EventHandler::new(move |value: String| {
            if let Ok(value) = value.parse::<f32>() {
                on_input(value)
            }
        })),
        "100".to_string(),
        "%".to_string(),
    );

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
pub fn NumberInputU32(
    GenericInputProps {
        label,
        label_class,
        div_class,
        input_class,
        disabled,
        on_input,
        value,
    }: GenericInputProps<u32>,
) -> Element {
    rsx! {
        PrimIntInput {
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
        PrimIntInput {
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
fn PrimIntInput<T: 'static + IntoAttributeValue + PrimInt + FromStr + Display>(
    label: String,
    #[props(default = String::default())] label_class: String,
    #[props(default = String::default())] div_class: String,
    #[props(default = String::default())] input_class: String,
    #[props(default = None)] maximum_value: Option<T>,
    #[props(default = String::default())] suffix: String,
    #[props(default = false)] disabled: bool,
    on_input: EventHandler<T>,
    value: T,
) -> Element {
    let input_id = use_memo(|| Alphanumeric.sample_string(&mut rand::rng(), 8));
    use_auto_numeric(
        input_id,
        value.to_string(),
        Some(EventHandler::new(move |value: String| {
            if let Ok(value) = value.parse::<T>() {
                on_input(value)
            }
        })),
        maximum_value.unwrap_or(T::max_value()).to_string(),
        suffix,
    );

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
