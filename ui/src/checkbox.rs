use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct CheckboxProps {
    label: String,
    #[props(default = String::default())]
    div_class: String,
    #[props(default = String::default())]
    label_class: String,
    #[props(default = String::default())]
    input_class: String,
    on_checked: EventHandler<bool>,
    checked: bool,
}

#[component]
pub fn Checkbox(props: CheckboxProps) -> Element {
    rsx! {
        div { class: props.div_class,
            label { class: props.label_class, {props.label} }
            input {
                class: props.input_class,
                r#type: "checkbox",
                oninput: move |e| {
                    (props.on_checked)(e.parsed::<bool>().unwrap());
                },
                checked: props.checked,
            }
        }
    }
}
