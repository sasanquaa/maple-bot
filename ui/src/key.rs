use backend::KeyBinding;
use dioxus::prelude::*;

#[derive(PartialEq, Props, Clone)]
pub struct KeyInputProps {
    #[props(default = String::default())]
    class: String,
    disabled: bool,
    is_active: bool,
    on_active: EventHandler<bool>,
    on_input: EventHandler<KeyBinding>,
    value: Option<KeyBinding>,
}

#[component]
pub fn KeyInput(
    KeyInputProps {
        class,
        disabled,
        is_active,
        on_active,
        on_input,
        value,
    }: KeyInputProps,
) -> Element {
    let mut has_error = use_signal(|| false);
    let mut input_element = use_signal(|| None);
    let border = if is_active {
        if has_error() {
            "border-red-500 ring-1 ring-red-200"
        } else {
            "border-blue-500 ring-1 ring-blue-200"
        }
    } else {
        "border-gray-300"
    };
    let active_background = if has_error() {
        "bg-red-50"
    } else {
        "bg-blue-50"
    };
    let active_color = if has_error() {
        "text-red-700"
    } else {
        "text-blue-700"
    };

    rsx! {
        div { class: "relative",
            input {
                r#type: "text",
                disabled,
                onmounted: move |e| {
                    input_element.set(Some(e.data()));
                },
                class: "outline-none {class} {border} text-xs text-center text-gray-700",
                readonly: true,
                onfocus: move |_| {
                    on_active(true);
                },
                onblur: move |_| {
                    on_active(false);
                    has_error.set(false);
                },
                onkeydown: move |e: Event<KeyboardData>| async move {
                    e.prevent_default();
                    if let Some(key) = map_key(e.key()) {
                        if let Some(input) = input_element().as_ref() {
                            let _ = input.set_focus(false).await;
                        }
                        has_error.set(false);
                        on_active(false);
                        on_input(key);
                    } else {
                        has_error.set(true);
                    }
                },
                placeholder: "Click to set",
                value: value.map(|key| key.to_string()),
            }
            if is_active {
                div { class: "absolute inset-0 flex items-center justify-center rounded {active_background} bg-opacity-50 text-xs {active_color}",
                    "Press any key..."
                }
            }
        }
    }
}

fn map_key(key: Key) -> Option<KeyBinding> {
    Some(match key {
        Key::Character(s) => match s.to_lowercase().as_str() {
            "a" => KeyBinding::A,
            "b" => KeyBinding::B,
            "c" => KeyBinding::C,
            "d" => KeyBinding::D,
            "e" => KeyBinding::E,
            "f" => KeyBinding::F,
            "g" => KeyBinding::G,
            "h" => KeyBinding::H,
            "i" => KeyBinding::I,
            "j" => KeyBinding::J,
            "k" => KeyBinding::K,
            "l" => KeyBinding::L,
            "m" => KeyBinding::M,
            "n" => KeyBinding::N,
            "o" => KeyBinding::O,
            "p" => KeyBinding::P,
            "q" => KeyBinding::Q,
            "r" => KeyBinding::R,
            "s" => KeyBinding::S,
            "t" => KeyBinding::T,
            "u" => KeyBinding::U,
            "v" => KeyBinding::V,
            "w" => KeyBinding::W,
            "x" => KeyBinding::X,
            "y" => KeyBinding::Y,
            "z" => KeyBinding::Z,
            "0" => KeyBinding::Zero,
            "1" => KeyBinding::One,
            "2" => KeyBinding::Two,
            "3" => KeyBinding::Three,
            "4" => KeyBinding::Four,
            "5" => KeyBinding::Five,
            "6" => KeyBinding::Six,
            "7" => KeyBinding::Seven,
            "8" => KeyBinding::Eight,
            "9" => KeyBinding::Nine,
            "`" => KeyBinding::Tilde,
            " " => KeyBinding::Space,
            _ => return None,
        },
        Key::F1 => KeyBinding::F1,
        Key::F2 => KeyBinding::F2,
        Key::F3 => KeyBinding::F3,
        Key::F4 => KeyBinding::F4,
        Key::F5 => KeyBinding::F5,
        Key::F6 => KeyBinding::F6,
        Key::F7 => KeyBinding::F7,
        Key::F8 => KeyBinding::F8,
        Key::F9 => KeyBinding::F9,
        Key::F10 => KeyBinding::F10,
        Key::F11 => KeyBinding::F11,
        Key::F12 => KeyBinding::F12,
        Key::ArrowUp => KeyBinding::Up,
        Key::Home => KeyBinding::Home,
        Key::End => KeyBinding::End,
        Key::PageUp => KeyBinding::PageUp,
        Key::PageDown => KeyBinding::PageDown,
        Key::Insert => KeyBinding::Insert,
        Key::Delete => KeyBinding::Delete,
        Key::Enter => KeyBinding::Enter,
        Key::Escape => KeyBinding::Esc,
        Key::Shift => KeyBinding::Shift,
        Key::Control => KeyBinding::Ctrl,
        Key::Alt => KeyBinding::Alt,
        _ => return None,
    })
}
