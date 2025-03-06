use dioxus::{document::EvalError, prelude::*};

use backend::{
    Minimap as MinimapData, delete_map, minimap_data, minimap_frame, player_position,
    redetect_minimap, rotate_actions, update_minimap,
};
use tokio::task::spawn_blocking;

#[component]
pub fn Minimap(minimap: Signal<Option<MinimapData>>, preset: Signal<Option<String>>) -> Element {
    const MINIMAP_JS: &str = r#"
        let minimap = document.getElementById("canvas-minimap");
        let minimapCtx = minimap.getContext("2d");
        let lastWidth = minimap.width;
        let lastHeight = minimap.height;

        while (true) {
            let [buffer, width, height] = await dioxus.recv();
            let data = new ImageData(new Uint8ClampedArray(buffer), width, height);
            let bitmap = await createImageBitmap(data);
            minimapCtx.drawImage(bitmap, 0, 0);
            if (lastWidth != width || lastHeight != height) {
                lastWidth = width;
                lastHeight = height;
                minimap.width = width;
                minimap.height = height;
            }
        }
    "#;
    let mut halting = use_signal(|| true);
    let mut position = use_signal::<Option<(i32, i32)>>(|| None);
    let reset = use_callback(move |_| {
        minimap.set(None);
        position.set(None);
    });

    use_effect(move || {
        if let Some((minimap, preset)) = minimap().zip(preset()) {
            spawn(async move {
                update_minimap(preset, minimap).await;
            });
        }
    });
    use_effect(move || {
        if let Some(minimap) = minimap() {
            if preset.peek().is_none() {
                preset.set(minimap.actions.keys().next().cloned());
            }
        } else {
            preset.set(None);
        }
    });
    use_future(move || async move {
        let mut canvas = document::eval(MINIMAP_JS);
        loop {
            let result = minimap_frame().await;
            let Ok(frame) = result else {
                if minimap.peek().is_some() {
                    reset(());
                }
                continue;
            };
            if minimap.peek().is_none() {
                minimap.set(minimap_data().await.ok());
            }
            position.set(player_position().await.ok());
            let Err(error) = canvas.send(frame) else {
                continue;
            };
            if matches!(error, EvalError::Finished) {
                // probably: https://github.com/DioxusLabs/dioxus/issues/2979
                canvas = document::eval(MINIMAP_JS);
            }
        }
    });

    rsx! {
        div { class: "flex flex-col items-center justify-center space-y-4 mb-8",
            p { class: "text-gray-700 text-sm",
                {minimap().map(|minimap| minimap.name).unwrap_or("Detecting...".to_string())}
            }
            canvas {
                class: "h-36 p-3 border border-gray-300 rounded-md",
                id: "canvas-minimap",
            }
            p { class: "text-gray-700 text-xs",
                {position().map(|(x, y)| format!("{x}, {y}")).unwrap_or("X, Y".to_string())}
            }
            div { class: "flex w-full space-x-6 items-center justify-center items-stretch h-7",
                button {
                    class: "button-tertiary w-24",
                    disabled: minimap().is_none(),
                    onclick: move |_| async move {
                        let value = *halting.peek();
                        halting.set(!value);
                        rotate_actions(!value).await;
                    },
                    if halting() {
                        "Start actions"
                    } else {
                        "Stop actions"
                    }
                }
                button {
                    class: "button-secondary",
                    disabled: minimap().is_none(),
                    onclick: move |_| async move {
                        redetect_minimap().await;
                        reset(());
                    },
                    "Re-detect map"
                }
                button {
                    class: "button-danger",
                    disabled: minimap().is_none(),
                    onclick: move |_| async move {
                        if let Some(minimap) = minimap.peek().clone() {
                            spawn_blocking(move || {
                                    delete_map(&minimap).unwrap();
                                })
                                .await
                                .unwrap();
                        }
                        redetect_minimap().await;
                        reset(());
                    },
                    "Delete map"
                }
            }
        }
    }
}
