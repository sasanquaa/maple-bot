use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct OptionsProps<T: 'static + Clone + PartialEq> {
    label: String,
    options: Vec<(T, String)>,
    on_select: EventHandler<(T, String)>,
    selected: T,
}

#[component]
pub fn Options<T>(props: OptionsProps<T>) -> Element
where
    T: PartialEq + Clone + 'static,
{
    rsx! {
        form {
            label {
                {props.label}
            }
            select {
                onchange: move |e| {
                    let value = e.value()
                        .parse::<usize>()
                        .map(|i| props.options[i].clone())
                        .unwrap();
                    (props.on_select)(value)
                },
                for (i, opt) in props.options.iter().enumerate() {
                    option {
                        selected: opt.0 == props.selected,
                        value: i.to_string(),
                        label: opt.1.clone()
                    }
                }
            }
        }
    }
}
