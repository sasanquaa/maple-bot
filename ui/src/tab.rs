use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct TabProps {
    tabs: Vec<String>,
    #[props(default = String::new())]
    div_class: String,
    class: String,
    selected_class: String,
    unselected_class: String,
    on_tab: EventHandler<String>,
    tab: String,
}

#[component]
pub fn Tab(
    TabProps {
        tabs,
        div_class,
        class,
        selected_class,
        unselected_class,
        on_tab,
        tab,
    }: TabProps,
) -> Element {
    rsx! {
        div { class: "flex {div_class}",
            for t in tabs {
                button {
                    class: {
                        let conditional_class = if t == tab {
                            selected_class.clone()
                        } else {
                            unselected_class.clone()
                        };
                        format!("{conditional_class} {class}")
                    },
                    onclick: move |_| {
                        on_tab(t.clone());
                    },
                    {t.clone()}
                }
            }
        }
    }
}
