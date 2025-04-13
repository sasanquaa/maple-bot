use backend::{
    Action, ActionKey, ActionMove, Configuration, Minimap as MinimapData, PlayerState,
    RotationMode, create_minimap, delete_map, minimap_frame, player_state, query_maps,
    redetect_minimap, rotate_actions, update_configuration, update_minimap, upsert_map,
};
use dioxus::{document::EvalError, prelude::*};
use serde::Serialize;
use tokio::task::spawn_blocking;

use crate::select::TextSelect;

const MINIMAP_JS: &str = r#"
    const canvas = document.getElementById("canvas-minimap");
    const canvasCtx = canvas.getContext("2d");
    let lastWidth = canvas.width;
    let lastHeight = canvas.height;

    while (true) {
        const [buffer, width, height, destinations] = await dioxus.recv();
        const data = new ImageData(new Uint8ClampedArray(buffer), width, height);
        const bitmap = await createImageBitmap(data);
        canvasCtx.beginPath()
        canvasCtx.fillStyle = "rgb(128, 255, 204)";
        canvasCtx.strokeStyle = "rgb(128, 255, 204)";
        canvasCtx.drawImage(bitmap, 0, 0);
        if (lastWidth != width || lastHeight != height) {
            lastWidth = width;
            lastHeight = height;
            canvas.width = width;
            canvas.height = height;
        }
        // TODO: ??????????????????????????
        let prevX = 0;
        let prevY = 0;
        for (let i = 0; i < destinations.length; i++) {
            let [x, y] = destinations[i];
            x = (x / width) * canvas.width;
            y = ((height - y) / height) * canvas.height;
            canvasCtx.fillRect(x - 2, y - 2, 2, 2);
            if (i > 0) {
                canvasCtx.moveTo(prevX, prevY);
                canvasCtx.lineTo(x, y);
                canvasCtx.stroke();
            }
            prevX = x;
            prevY = y;
        }
    }
"#;
const MINIMAP_ACTIONS_JS: &str = r#"
    const canvas = document.getElementById("canvas-minimap-actions");
    const canvasCtx = canvas.getContext("2d");
    const [width, height, actions, autoMobEnabled, autoMobBound, platforms] = await dioxus.recv();
    canvasCtx.clearRect(0, 0, canvas.width, canvas.height);
    const anyActions = actions.filter((action) => action.condition === "Any");
    const erdaActions = actions.filter((action) => action.condition === "ErdaShowerOffCooldown");
    const millisActions = actions.filter((action) => action.condition === "EveryMillis");

    canvasCtx.fillStyle = "rgb(255, 153, 128)";
    canvasCtx.strokeStyle = "rgb(255, 153, 128)";
    drawActions(canvas, canvasCtx, anyActions, true);
    if (autoMobEnabled) {
        const x = (autoMobBound.x / width) * canvas.width;
        const y = (autoMobBound.y / height) * canvas.height;
        const w = (autoMobBound.width / width) * canvas.width;
        const h = (autoMobBound.height / height) * canvas.height;
        canvasCtx.beginPath();
        canvasCtx.rect(x, y, w, h);
        canvasCtx.stroke();
    }
    for (const platform of platforms) {
        const xStart = (platform.x_start / width) * canvas.width;
        const xEnd = (platform.x_end / width) * canvas.width;
        const y = ((height - platform.y) / height) * canvas.height;
        canvasCtx.beginPath();
        canvasCtx.moveTo(xStart, y);
        canvasCtx.lineTo(xEnd, y);
        canvasCtx.stroke();
    }

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
            const x = (action.x / width) * canvas.width;
            const y = ((height - action.y) / height) * canvas.height;
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
    copy_position: Signal<Option<(i32, i32)>>,
    config: ReadOnlySignal<Option<Configuration>, SyncStorage>,
) -> Element {
    let mut halting = use_signal(|| true);
    let mut state = use_signal::<Option<PlayerState>>(|| None);
    let mut detected_minimap_size = use_signal::<Option<(usize, usize)>>(|| None);
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
        copy_position.set(state().and_then(|state| state.position));
    });
    use_effect(move || {
        if let (Some(minimap), preset) = (minimap(), preset()) {
            spawn(async move { update_minimap(preset, minimap).await });
        }
        if let Some(config) = config() {
            spawn(async move {
                update_configuration(config).await;
            });
        }
    });
    use_effect(move || {
        if let Some(minimap) = minimap() {
            let preset_peek = preset.peek().clone();
            if preset_peek.is_none() || !minimap.actions.contains_key(preset_peek.as_ref().unwrap())
            {
                preset.set(minimap.actions.keys().next().cloned());
            }
        } else {
            preset.set(None);
        }
    });
    // draw actions, auto mob bound
    use_effect(move || {
        let config = minimap().map(|minimap| {
            let bound = if let RotationMode::AutoMobbing(mobbing) = minimap.rotation_mode {
                Some(mobbing.bound)
            } else {
                None
            };
            (
                minimap.width,
                minimap.height,
                bound.is_some(),
                bound.unwrap_or_default(),
                minimap.platforms,
            )
        });
        let actions = actions();
        if let Some((width, height, enabled, bound, platforms)) = config {
            spawn(async move {
                document::eval(MINIMAP_ACTIONS_JS)
                    .send((width, height, actions, enabled, bound, platforms))
                    .unwrap();
            });
        }
    });
    // draw minimap and update states
    use_future(move || async move {
        let mut canvas = document::eval(MINIMAP_JS);
        loop {
            let player = player_state().await;
            let destinations = player.destinations.clone();
            state.set(Some(player));
            let minimap_frame = minimap_frame().await;
            let Ok((frame, width, height)) = minimap_frame else {
                detected_minimap_size.set(None);
                continue;
            };
            if detected_minimap_size.peek().is_none() {
                detected_minimap_size.set(Some((width, height)));
            }
            let Err(error) = canvas.send((frame, width, height, destinations)) else {
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
            div { class: "flex flex-col items-center justify-center space-y-2 text-gray-700 text-xs",
                MinimapsSelect { minimap }
                p {
                    {
                        minimap()
                            .map(|minimap| {
                                format!("Selected: {}px x {}px", minimap.width, minimap.height)
                            })
                            .unwrap_or("Selected: Width, Height".to_string())
                    }
                }
                p {
                    {
                        detected_minimap_size()
                            .map(|(width, height)| { format!("Detected: {}px x {}px", width, height) })
                            .unwrap_or("Detected: Width, Height".to_string())
                    }
                }
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
                                .and_then(|state| state.health)
                                .map(|(current_health, max_health)| {
                                    format!("Health: {} / {}", current_health, max_health)
                                })
                                .unwrap_or("Health: Unknown".to_string())
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
                    },
                    "Re-detect map"
                }
                button {
                    class: "button-danger",
                    disabled: minimap().is_none(),
                    onclick: move |_| async move {
                        let data = minimap.peek().clone();
                        if let Some(data) = data {
                            spawn_blocking(move || {
                                    delete_map(&data).unwrap();
                                })
                                .await
                                .unwrap();
                            minimap.set(None);
                        }
                    },
                    "Delete map"
                }
            }
        }
    }
}

#[component]
fn MinimapsSelect(minimap: Signal<Option<MinimapData>>) -> Element {
    let mut is_creating = use_signal(|| false);
    let mut minimaps = use_resource(|| async {
        spawn_blocking(|| query_maps().unwrap_or_default())
            .await
            .unwrap()
    });
    let minimaps_value = minimaps.value();
    let minimap_names = use_memo(move || match minimaps_value() {
        Some(minimaps) => minimaps
            .into_iter()
            .map(|minimap| minimap.name)
            .collect::<Vec<_>>(),
        None => vec![],
    });

    // why is it so easy to shoot myself in the foot
    use_effect(move || {
        minimap();
        minimaps.restart()
    });
    use_effect(move || {
        if let Some(minimaps) = minimaps_value() {
            if minimap.peek().is_none() && !minimaps.is_empty() {
                minimap.set(minimaps.into_iter().next());
            }
        }
    });

    rsx! {
        TextSelect {
            create_text: "+ Create minimap",
            on_create: move |name: String| async move {
                if let Some(mut data) = create_minimap(name).await {
                    is_creating.set(true);
                    upsert_map(&mut data).unwrap();
                    is_creating.set(false);
                    minimap.set(Some(data.clone()));
                }
            },
            disabled: is_creating(),
            on_select: move |(i, _)| {
                minimap
                    .set(
                        match minimaps_value() {
                            Some(minimaps) => minimaps.get(i).cloned(),
                            None => None,
                        },
                    )
            },
            options: minimap_names(),
            selected: minimap().map(|minimap| minimap.name),
        }
    }
}
