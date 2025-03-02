use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct TabProps {
    tabs: Vec<String>,
    on_tab: EventHandler<String>,
    tab: String,
}

#[component]
pub fn Tab(props: TabProps) -> Element {
    rsx! {
        div { class: "flex",
            for tab in props.tabs {
                button {
                    class: {
                        let class = if tab == props.tab {
                            "bg-white text-gray-800"
                        } else {
                            "hover:text-gray-700 text-gray-400 bg-gray-100"
                        };
                        format!("{class} py-2 px-4 font-medium text-sm focus:outline-none")
                    },
                    onclick: move |_| {
                        (props.on_tab)(tab.clone());
                    },
                    {tab.clone()}
                }
            }
        }
    }
}
