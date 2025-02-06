use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct OneButtonProps {
    on_ok: EventHandler,
    children: Element,
}

#[derive(PartialEq, Props, Clone)]
pub struct TwoButtonsProps {
    on_ok: EventHandler,
    ok_body: Element,
    on_cancel: EventHandler,
    cancel_body: Element,
}

#[component]
pub fn OneButton(props: OneButtonProps) -> Element {
    rsx! {
        BaseButton {
            on_click: props.on_ok,
            {props.children}
        }
    }
}

#[component]
pub fn TwoButtons(props: TwoButtonsProps) -> Element {
    rsx! {
        div {
            class: "grid grid-rows-1 grid-cols-2 gap-x-2",
            BaseButton {
                on_click: props.on_ok,
                {props.ok_body}
            }
            BaseButton {
                on_click: props.on_cancel,
                {props.cancel_body}
            }
        }
    }
}

#[derive(PartialEq, Props, Clone)]
struct BaseButtonProps {
    on_click: EventHandler,
    children: Element,
}

#[component]
fn BaseButton(props: BaseButtonProps) -> Element {
    rsx! {
        div {
            class: "flex justify-center",
            button {
                class: "w-fit h-fit border border-black text-sm text-black px-2 py-1 font-meiryo hover:bg-gray-100",
                onclick: move |_| (props.on_click)(()),
                {props.children}
            }
        }
    }
}
