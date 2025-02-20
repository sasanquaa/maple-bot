use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct OptionsProps<T: 'static + Clone + PartialEq> {
    label: String,
    options: Vec<(T, String)>,
    on_select: EventHandler<T>,
    selected: T,
}

#[component]
pub fn Options<T>(props: OptionsProps<T>) -> Element
where
    T: PartialEq + Clone + 'static,
{
    rsx! {
        div { class: "flex gap-[8px] justify-items-center",
            p { class: "font-main", {props.label} }
            select {
                class: "border border-black",
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
                        class: "font-main",
                        selected: opt.0 == props.selected,
                        value: i.to_string(),
                        label: opt.1.clone(),
                    }
                }
            }
        }
    }
}
