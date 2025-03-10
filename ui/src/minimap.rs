use dioxus::{document::EvalError, prelude::*};

use backend::{
    Action, ActionKey, ActionMove, Configuration, Minimap as MinimapData, PlayerState, delete_map,
    minimap_data, minimap_frame, player_state, redetect_minimap, rotate_actions,
    update_configuration, update_minimap,
};
use serde::Serialize;
use tokio::task::spawn_blocking;

#[derive(Clone, PartialEq, Serialize)]
struct ActionView {
    x: i32,
    y: i32,
    condition: String,
}

#[component]
pub fn Minimap(
    minimap: Signal<Option<MinimapData>>,
    preset: Signal<Option<String>>,
    last_preset: Signal<Option<(i64, String)>>,
    config: ReadOnlySignal<Option<Configuration>, SyncStorage>,
) -> Element {
    const MINIMAP_JS: &str = r#"
        const canvas = document.getElementById("canvas-minimap");
        const canvasCtx = canvas.getContext("2d");
        let lastWidth = canvas.width;
        let lastHeight = canvas.height;

        while (true) {
            const [buffer, width, height] = await dioxus.recv();
            const data = new ImageData(new Uint8ClampedArray(buffer), width, height);
            const bitmap = await createImageBitmap(data);
            canvasCtx.drawImage(bitmap, 0, 0);
            if (lastWidth != width || lastHeight != height) {
                lastWidth = width;
                lastHeight = height;
                canvas.width = width;
                canvas.height = height;
            }
        }
    "#;
    const MINIMAP_ACTIONS_JS: &str = r#"
        const canvas = document.getElementById("canvas-minimap-actions");
        const canvasCtx = canvas.getContext("2d");
        const [width, height, actions] = await dioxus.recv();
        canvasCtx.clearRect(0, 0, canvas.width, canvas.height);
        const anyActions = actions.filter((action) => action.condition === "Any");
        const erdaActions = actions.filter((action) => action.condition === "ErdaShowerOffCooldown");
        const millisActions = actions.filter((action) => action.condition === "EveryMillis");

        canvasCtx.fillStyle = "rgb(255, 153, 128)";
        canvasCtx.strokeStyle = "rgb(255, 153, 128)";
        drawActions(canvas, canvasCtx, anyActions, true);

        canvasCtx.fillStyle = "rgb(179, 198, 255)";
        canvasCtx.strokeStyle = "rgb(179, 198, 255)";
        drawActions(canvas, canvasCtx, erdaActions, true);

        canvasCtx.fillStyle = "rgb(128, 255, 204)";
        canvasCtx.strokeStyle = "rgb(128, 255, 204)";
        drawActions(canvas, canvasCtx, millisActions, false);

        function drawActions(canvas, ctx, actions, hasArc) {
            const rectSize = 4;
            const rectHalf = rectSize / 2;
            let lastAction = null;
            for (const action of actions) {
                let x = (action.x / width) * canvas.width;
                let y = ((height - action.y) / height) * canvas.height;
                ctx.fillRect(x, y, rectSize, rectSize);
                if (!hasArc) {
                    continue;
                }
                if (lastAction !== null) {
                    let [fromX, fromY] = lastAction;
                    drawArc(ctx, fromX + rectHalf, fromY + rectHalf, x + rectHalf, y + rectHalf);
                }
                lastAction = [x, y];
            }
        }
        function drawArc(ctx, fromX, fromY, toX, toY) {
            const cx = (fromX + toX) / 2;
            const cy = (fromY + toY) / 2;
            const dx = cx - fromX;
            const dy = cy - fromY;
            const radius = Math.sqrt(dx * dx + dy * dy);
            const startAngle = Math.atan2(fromY - cy, fromX - cx);
            const endAngle = Math.atan2(toY - cy, toX - cx);
            ctx.beginPath();
            ctx.arc(cx, cy, radius, startAngle, endAngle, false);
            ctx.stroke();
        }
    "#;
    let mut halting = use_signal(|| true);
    let mut state = use_signal::<Option<PlayerState>>(|| None);
    let reset = use_callback(move |_| {
        minimap.set(None);
    });
    let actions = use_memo::<Vec<ActionView>>(move || {
        minimap()
            .zip(preset())
            .and_then(|(minimap, preset)| minimap.actions.get(&preset).cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|action| match action {
                Action::Move(ActionMove {
                    position,
                    condition,
                    ..
                }) => Some(ActionView {
                    x: position.x,
                    y: position.y,
                    condition: condition.to_string(),
                }),
                Action::Key(ActionKey {
                    position: Some(position),
                    condition,
                    ..
                }) => Some(ActionView {
                    x: position.x,
                    y: position.y,
                    condition: condition.to_string(),
                }),
                Action::Key(ActionKey { position: None, .. }) => None,
            })
            .collect()
    });

    use_effect(move || {
        #[allow(clippy::single_match)]
        match (minimap(), preset()) {
            (Some(minimap), preset) => {
                spawn(async move { update_minimap(preset, minimap).await });
            }
            (None, _) => (),
        }
        if let Some(config) = config() {
            spawn(async move {
                update_configuration(config).await;
            });
        }
    });
    use_effect(move || {
        if let Some(minimap) = minimap() {
            if preset.peek().is_none() {
                match last_preset.peek().clone() {
                    Some((id, last_preset)) if Some(id) == minimap.id => {
                        preset.set(Some(last_preset));
                    }
                    _ => {
                        preset.set(minimap.actions.keys().next().cloned());
                    }
                }
            }
        } else {
            preset.set(None);
        }
    });
    use_effect(move || {
        let size = minimap().map(|minimap| (minimap.width, minimap.height));
        let actions = actions();
        if let Some((width, height)) = size {
            spawn(async move {
                document::eval(MINIMAP_ACTIONS_JS)
                    .send((width, height, actions))
                    .unwrap();
            });
        }
    });
    use_future(move || async move {
        let mut canvas = document::eval(MINIMAP_JS);
        loop {
            state.set(Some(player_state().await));
            let result = minimap_frame().await;
            let Ok(frame) = result else {
                let minimap = minimap.peek().clone();
                if let Some(minimap) = minimap {
                    last_preset.set(minimap.id.zip(preset.peek().clone()));
                    reset(());
                }
                continue;
            };
            if minimap.peek().is_none() {
                minimap.set(minimap_data().await.ok());
            }
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
            div { class: "relative p-3 h-36 border border-gray-300 rounded-md",
                canvas { class: "w-full h-full", id: "canvas-minimap" }
                div { class: "absolute inset-3",
                    canvas { class: "w-full h-full", id: "canvas-minimap-actions" }
                }
            }
            div { class: "flex flex-col text-gray-700 text-xs space-y-1 font-mono",
                p { class: "text-center",
                    {
                        state()
                            .and_then(|state| state.position)
                            .map(|(x, y)| { format!("{}, {}", x, y) })
                            .unwrap_or("X, Y".to_string())
                    }
                }
                div { class: "flex flex-col text-left",
                    p {
                        {
                            state()
                                .map(|state| format!("State: {}", state.state))
                                .unwrap_or("State: Unknown".to_string())
                        }
                    }
                    p {
                        {
                            state()
                                .map(|state| {
                                    format!(
                                        "Priority Action: {}",
                                        state.priority_action.unwrap_or("None".to_string()),
                                    )
                                })
                                .unwrap_or("Priority Action: Unknown".to_string())
                        }
                    }
                    p {
                        {
                            state()
                                .map(|state| {
                                    format!(
                                        "Normal Action: {}",
                                        state.normal_action.unwrap_or("None".to_string()),
                                    )
                                })
                                .unwrap_or("Normal Action: Unknown".to_string())
                        }
                    }
                    p {
                        {
                            state()
                                .map(|state| { format!("Erda Shower: {}", state.erda_shower_state) })
                                .unwrap_or("Erda Shower: Unknown".to_string())
                        }
                    }
                }
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
