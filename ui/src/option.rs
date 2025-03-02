use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct OptionProps<T: 'static + Clone + PartialEq> {
    label: String,
    #[props(default = String::default())]
    div_class: String,
    #[props(default = String::default())]
    label_class: String,
    #[props(default = String::default())]
    select_class: String,
    options: Vec<(T, String)>,
    on_select: EventHandler<T>,
    selected: T,
}

#[component]
pub fn Option<T>(props: OptionProps<T>) -> Element
where
    T: PartialEq + Clone + 'static,
{
    rsx! {
        div { class: props.div_class,
            label { class: props.label_class, {props.label} }
            select {
                class: props.select_class,
                onchange: move |e| {
                    let value = e
                        .value()
                        .parse::<usize>()
                        .map(|i| props.options[i].0.clone())
                        .unwrap();
                    (props.on_select)(value)
                },
                for (i , opt) in props.options.iter().enumerate() {
                    option {
                        selected: opt.0 == props.selected,
                        value: i.to_string(),
                        label: opt.1.clone(),
                    }
                }
            }
        }
    }
}
