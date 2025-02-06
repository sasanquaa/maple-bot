use std::ops::DerefMut;

use backend::{
    game,
    models::{Character, query_characters},
};
use components::button::{OneButton, TwoButtons};
use dioxus::{
    desktop::{
        WindowBuilder,
        wry::dpi::{PhysicalSize, Size},
    },
    document::EvalError,
    prelude::*,
};
use tokio::{sync::mpsc, task::spawn_blocking};
use tracing_log::LogTracer;

mod components;

const TAILWIND_CSS: Asset = asset!("public/tailwind.css");

fn main() {
    LogTracer::init().unwrap();
    let window = WindowBuilder::new()
        .with_inner_size(Size::Physical(PhysicalSize::new(510, 400)))
        .with_resizable(false)
        .with_maximizable(false)
        .with_title("Maple Bot")
        .with_always_on_top(true);
    let cfg = dioxus::desktop::Config::default()
        .with_menu(None)
        .with_window(window);
    dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        div {
            class: "flex",
            Minimap {}
            div {
                class: "w-[160px]",
                Characters {}
            }
        }
    }
}

#[component]
fn Minimap() -> Element {
    let mut grid_width = use_signal_sync(|| 0);
    let mut grid_height = use_signal_sync(|| 0);

    use_future(move || async move {
        let (tx, mut rx) = mpsc::channel::<(Vec<u8>, usize, usize)>(1);
        let _ = spawn(async move {
            let mut canvas = document::eval(include_str!("js/minimap.js"));
            loop {
                let result = rx.recv().await;
                let Some(frame) = result else {
                    continue;
                };
                let Err(error) = canvas.send(frame) else {
                    continue;
                };
                if matches!(error, EvalError::Finished) {
                    // probably: https://github.com/DioxusLabs/dioxus/issues/2979
                    canvas = document::eval(include_str!("js/minimap.js"));
                }
            }
        });
        let _ = spawn_blocking(move || {
            game::Context::new()
                .expect("failed to start game update loop")
                .update_loop(|context| {
                    if let Ok((bytes, width, height)) = context.minimap() {
                        let cur_width = *grid_width.peek();
                        let cur_height = *grid_height.peek();
                        if cur_width != width || cur_height != height {
                            *grid_width.write() = width;
                            *grid_height.write() = height;
                        }
                        let _ = tx.try_send((bytes, width, height));
                    }
                })
        })
        .await;
    });

    rsx! {
        div {
            class: "grid grid-flow-row auto-rows-max p-[16px] w-[350px] place-items-center",
            p {
                "Player State"
            }
            div {
                class: "flex w-full relative",
                canvas {
                    class: "w-full",
                    id: "canvas-minimap",
                },
                canvas {
                    id: "canvas-minimap-magnifier",
                    class: "absolute hidden",
                }
            }
            p {
                "Action 1"
            }
            p {
                "Action 2"
            }
            p {
                "Action 3"
            }
            p {
                "Action 4"
            }
        }
    }
}

#[component]
fn Characters() -> Element {
    let characters = use_resource(|| async {
        spawn_blocking(|| query_characters().unwrap_or_default())
            .await
            .unwrap()
    });
    let mut creating = use_signal(|| false);
    let mut creating_character = use_signal(Character::default);

    use_effect(move || {
        if creating() {
            *creating_character.write() = Character::default();
        }
    });

    rsx! {
        match characters() {
            Some(characters) => rsx! {
                div {
                    class: "grid grid-flow-row grid-cols-1 gap-y-3 w-40 h-fit",
                    if !creating() {
                        OneButton {
                            on_ok: move |_| {
                                *creating.write() = true;
                            },
                            "Create"
                        }
                    } else {
                        input {
                            class: "font-meiryo",
                            oninput: move |e| {
                                creating_character.write().deref_mut().name = e.value();
                            },
                            placeholder: "Character name"
                        }
                        div {
                            class: "flex flex-row gap-x-2",
                            OneButton {
                                on_ok: move |_| {
                                    *creating.write() = true;
                                },
                                "Add skill"
                            }
                        }
                        TwoButtons {
                            on_ok: move |_| {
                                *creating.write() = false;
                            },
                            ok_body: rsx! {"Save"},
                            on_cancel: move |_| {
                                *creating.write() = false;
                            },
                            cancel_body: rsx! {"Cancel"}
                        }
                    }
                    ul {
                        for character in characters {
                            li {
                                p {
                                    class: "text-sm text-dark",
                                    {character.name}
                                }
                            }
                        }
                    }
                }
            },
            None => rsx! {},
        }
    }
}

#[component]
fn Dropdown() -> Element {
    rsx! {}
}
