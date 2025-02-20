use dioxus::prelude::*;

pub mod button;
pub mod options;

const DIV_STYLE: &str = "flex gap-[8px] justify-items-center";
const INPUT_STYLE: &str = "font-main w-[64px] border border-black";

#[component]
pub fn Divider() -> Element {
    rsx! {
        hr { class: "w-full my-2 h-0.5 border-t-0 bg-black" }
    }
}

#[component]
pub fn Checkbox(props: InputProps<bool>) -> Element {
    rsx! {
        div { class: DIV_STYLE,
            p { class: "font-main", {props.label} }
            input {
                class: "w-4 h-4 border-black rounded-sm",
                r#type: "checkbox",
                oninput: move |e| {
                    (props.on_input)(e.value().parse::<bool>().unwrap());
                },
                checked: props.value,
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
pub struct InputProps<T: 'static + Clone + PartialEq> {
    label: String,
    on_input: EventHandler<T>,
    value: T,
}

#[component]
pub fn TextInput(props: InputProps<String>) -> Element {
    rsx! {
        div { class: DIV_STYLE,
            p { class: "font-main", {props.label} }
            input {
                class: INPUT_STYLE,
                oninput: move |e| { (props.on_input)(e.value()) },
                value: props.value,
            }
        }
    }
}

#[component]
pub fn NumberInput(props: InputProps<i32>) -> Element {
    rsx! {
        div { class: DIV_STYLE,
            p { class: "font-main", {props.label} }
            input {
                class: INPUT_STYLE,
                r#type: "number",
                min: "0",
                oninput: move |e| {
                    let value = e.value().parse::<i32>().unwrap_or(0);
                    (props.on_input)(value)
                },
                value: props.value,
            }
        }
    }
}
