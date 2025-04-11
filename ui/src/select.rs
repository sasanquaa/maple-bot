use std::{fmt::Display, str::FromStr};

use backend::IntoEnumIterator;
use dioxus::prelude::*;

use crate::input::LabeledInput;

#[derive(PartialEq, Props, Clone)]
pub struct SelectProps<T: 'static + Clone + PartialEq> {
    #[props(default = String::default())]
    label: String,
    #[props(default = String::from("collapse"))]
    label_class: String,
    #[props(default = String::default())]
    div_class: String,
    #[props(default = String::default())]
    select_class: String,
    #[props(default = false)]
    disabled: bool,
    options: Vec<(T, String)>,
    on_select: EventHandler<(usize, T)>,
    selected: T,
}

#[component]
pub fn EnumSelect<T: 'static + Clone + Copy + PartialEq + Display + FromStr + IntoEnumIterator>(
    #[props(default = String::default())] label: String,
    #[props(default = String::from("collapse"))] label_class: String,
    #[props(default = String::default())] div_class: String,
    #[props(default = String::default())] select_class: String,
    #[props(default = false)] disabled: bool,
    on_select: EventHandler<T>,
    selected: T,
    #[props(default = Vec::new())] excludes: Vec<T>,
) -> Element {
    let options = T::iter()
        .filter(|variant| !excludes.contains(variant))
        .map(|variant| (variant.to_string(), variant.to_string()))
        .collect::<Vec<_>>();
    let selected = selected.to_string();

    rsx! {
        Select {
            label,
            disabled,
            div_class,
            label_class,
            select_class,
            options,
            on_select: move |(_, variant): (usize, String)| {
                if let Ok(value) = T::from_str(variant.as_str()) {
                    on_select(value);
                }
            },
            selected,
        }
    }
}

#[component]
pub fn TextSelect(
    create_text: String,
    on_create: EventHandler<String>,
    disabled: bool,
    on_select: EventHandler<(usize, String)>,
    options: Vec<String>,
    selected: Option<String>,
) -> Element {
    let mut is_creating = use_signal(|| false);
    let mut creating_text = use_signal(String::default);
    let mut creating_error = use_signal(|| false);
    let reset_creating = use_callback(move |()| {
        is_creating.set(false);
        creating_text.set("".to_string());
        creating_error.set(false);
    });

    use_effect(use_reactive!(|selected| {
        if selected.is_none() {
            reset_creating(());
        }
    }));
    use_effect(use_reactive!(|disabled| {
        if disabled {
            reset_creating(());
        }
    }));

    rsx! {
        div { class: "flex w-fit h-7 items-stretch mb-5",
            if options.is_empty() && !is_creating() {
                button {
                    class: "button-secondary border border-gray-300",
                    disabled,
                    onclick: move |_| {
                        is_creating.set(true);
                    },
                    {create_text}
                }
            } else if !is_creating() {
                Select {
                    label: "",
                    label_class: "collapse",
                    select_class: "rounded h-full border border-gray-300 text-xs text-gray-800 outline-none",
                    disabled,
                    options: options
                        .into_iter()
                        .chain([create_text.clone()].into_iter())
                        .map(|text| (text.clone(), text))
                        .collect(),
                    on_select: move |(usize, text)| {
                        if text == create_text {
                            is_creating.set(true);
                        } else {
                            on_select((usize, text));
                        }
                    },
                    selected: selected.unwrap_or_default(),
                }
            } else {
                div { class: "flex space-x-1",
                    input {
                        class: {
                            let border = if creating_error() { "border-red-500" } else { "border-gray-300" };
                            format!(
                                "rounded flex-1 w-40 border {border} px-2 text-xs text-gray-800 outline-none",
                            )
                        },
                        placeholder: "New name",
                        onchange: move |e| {
                            creating_text.set(e.value());
                        },
                        value: creating_text(),
                    }
                    button {
                        class: "button-primary",
                        onclick: move |_| {
                            let text = creating_text.peek().clone();
                            if text.is_empty() {
                                creating_error.set(true);
                                return;
                            }
                            reset_creating(());
                            on_create(text);
                        },
                        "Save"
                    }
                    button {
                        class: "button-tertiary",
                        onclick: move |_| {
                            reset_creating(());
                        },
                        "Cancel"
                    }
                }
            }
        }
    }
}

#[component]
pub fn Select<T>(
    SelectProps {
        label,
        div_class,
        label_class,
        select_class,
        options,
        disabled,
        on_select,
        selected,
    }: SelectProps<T>,
) -> Element
where
    T: PartialEq + Clone + 'static,
{
    rsx! {
        LabeledInput {
            label,
            label_class,
            div_class,
            disabled,
            select {
                class: select_class,
                disabled,
                onchange: move |e| {
                    let i = e.value().parse::<usize>().unwrap();
                    let value = options[i].0.clone();
                    on_select((i, value))
                },
                for (i , opt) in options.iter().enumerate() {
                    option {
                        disabled,
                        selected: opt.0 == selected,
                        value: i.to_string(),
                        label: opt.1.clone(),
                    }
                }
            }
        }
    }
}
