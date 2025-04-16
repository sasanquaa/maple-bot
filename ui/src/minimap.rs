use std::sync::Arc;

use backend::{
    Action, ActionKey, ActionMove, Minimap as MinimapData, PlayerState, RotationMode,
    create_minimap, delete_map, minimap_frame, minimap_platforms_bound, player_state, query_maps,
    redetect_minimap, rotate_actions, rotate_actions_halting, update_minimap, upsert_map,
};
use dioxus::{document::EvalError, prelude::*};
use futures_util::StreamExt;
use serde::Serialize;
use tokio::{
    sync::{Mutex, mpsc::Receiver},
    task::spawn_blocking,
};

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

#[derive(Clone)]
pub enum MinimapMessage {
    ToggleHalting,
    RedetectMinimap,
    CreateMinimap(String),
    UpdateMinimap(MinimapData, bool),
    UpdateMinimapPreset(String),
    DeleteMinimap,
}

#[component]
pub fn Minimap(
    minimap_rx: ReadOnlySignal<Arc<Mutex<Receiver<MinimapMessage>>>>,
    minimap: Signal<Option<MinimapData>>,
    preset: Signal<Option<String>>,
    copy_position: Signal<Option<(i32, i32)>>,
) -> Element {
    let mut halting = use_signal(|| true);
    let mut state = use_signal::<Option<PlayerState>>(|| None);
    let mut detected_minimap_size = use_signal::<Option<(usize, usize)>>(|| None);
    let mut platforms_bound = use_signal(|| None);
    let mut minimaps = use_resource(move || async move {
        let minimaps = spawn_blocking(|| query_maps().unwrap_or_default())
            .await
            .unwrap();
        if !minimaps.is_empty() && minimap.peek().is_none() {
            minimap.set(minimaps.first().cloned());
            preset.set(
                minimap
                    .peek()
                    .clone()
                    .unwrap()
                    .actions
                    .keys()
                    .next()
                    .cloned(),
            );
            update_minimap(preset.peek().clone(), minimap.peek().clone().unwrap()).await;
        }
        minimaps
    });
    let coroutine = use_coroutine(
        move |mut rx: UnboundedReceiver<MinimapMessage>| async move {
            while let Some(msg) = rx.next().await {
                match msg {
                    MinimapMessage::ToggleHalting => {
                        rotate_actions(!halting()).await;
                    }
                    MinimapMessage::RedetectMinimap => {
                        redetect_minimap().await;
                    }
                    MinimapMessage::CreateMinimap(name) => {
                        if let Some(mut data) = create_minimap(name).await {
                            upsert_map(&mut data).unwrap();
                            minimap.set(Some(data));
                            minimaps.restart();
                        }
                    }
                    MinimapMessage::UpdateMinimap(mut data, save) => {
                        preset.set(data.actions.keys().next().cloned());
                        minimap.set(Some(data.clone()));
                        update_minimap(preset(), data.clone()).await;
                        if save {
                            spawn_blocking(move || {
                                upsert_map(&mut data).unwrap();
                            })
                            .await
                            .unwrap();
                            minimaps.restart();
                        }
                    }
                    MinimapMessage::UpdateMinimapPreset(new_preset) => {
                        if preset().as_ref() != Some(&new_preset) {
                            preset.set(Some(new_preset));
                            update_minimap(preset(), minimap().unwrap()).await;
                        }
                    }
                    MinimapMessage::DeleteMinimap => {
                        let minimap = minimap.replace(None);
                        if let Some(minimap) = minimap {
                            spawn_blocking(move || {
                                delete_map(&minimap).unwrap();
                            })
                            .await
                            .unwrap();
                            minimaps.restart();
                        }
                    }
                }
            }
        },
    );

    // draw actions, auto mob bound
    use_effect(move || {
        let minimap = minimap();
        let preset = preset();
        let actions = minimap
            .clone()
            .zip(preset)
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
            .collect::<Vec<ActionView>>();
        let platforms_bound = platforms_bound();
        if let Some(minimap) = minimap {
            let bound = if let RotationMode::AutoMobbing(mobbing) = minimap.rotation_mode {
                if minimap.auto_mob_platforms_bound {
                    platforms_bound.or(Some(mobbing.bound))
                } else {
                    Some(mobbing.bound)
                }
            } else {
                None
            };
            spawn(async move {
                document::eval(MINIMAP_ACTIONS_JS)
                    .send((
                        minimap.width,
                        minimap.height,
                        actions,
                        bound.is_some(),
                        bound.unwrap_or_default(),
                        minimap.platforms,
                    ))
                    .unwrap();
            });
        }
    });
    use_future(move || async move {
        loop {
            if let Some(msg) = minimap_rx().lock().await.recv().await {
                coroutine.send(msg);
            }
        }
    });
    // draw minimap and update states
    use_future(move || async move {
        let mut canvas = document::eval(MINIMAP_JS);
        loop {
            let player_state = player_state().await;
            let destinations = player_state.destinations.clone();
            let is_halting = rotate_actions_halting().await;
            let bound = minimap_platforms_bound().await;
            if halting() != is_halting {
                halting.set(is_halting);
            }
            if platforms_bound() != bound {
                platforms_bound.set(bound);
            }
            if copy_position() != player_state.position {
                copy_position.set(player_state.position);
            }
            state.set(Some(player_state));
            let minimap_frame = minimap_frame().await;
            let Ok((frame, width, height)) = minimap_frame else {
                if detected_minimap_size().is_some() {
                    detected_minimap_size.set(None);
                }
                continue;
            };
            if detected_minimap_size().is_none() {
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
                MinimapsSelect { minimap, minimaps, coroutine }
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
                        coroutine.send(MinimapMessage::ToggleHalting);
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
                        coroutine.send(MinimapMessage::RedetectMinimap);
                    },
                    "Re-detect map"
                }
                button {
                    class: "button-danger",
                    disabled: minimap().is_none(),
                    onclick: move |_| async move {
                        coroutine.send(MinimapMessage::DeleteMinimap);
                    },
                    "Delete map"
                }
            }
        }
    }
}

#[component]
fn MinimapsSelect(
    minimap: ReadOnlySignal<Option<MinimapData>>,
    minimaps: ReadOnlySignal<Option<Vec<MinimapData>>>,
    coroutine: Coroutine<MinimapMessage>,
) -> Element {
    let minimap_names = use_memo(move || {
        minimaps()
            .map(|minimaps| {
                minimaps
                    .into_iter()
                    .map(|minimap| minimap.name)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });

    rsx! {
        TextSelect {
            create_text: "+ Create map",
            on_create: move |name: String| async move {
                coroutine.send(MinimapMessage::CreateMinimap(name));
            },
            disabled: false,
            on_select: move |(i, _)| {
                if let Some(minimaps) = minimaps() {
                    coroutine
                        .send(
                            MinimapMessage::UpdateMinimap(
                                minimaps.get(i).cloned().unwrap(),
                                false,
                            ),
                        );
                }
            },
            options: minimap_names(),
            selected: minimap().map(|minimap| minimap.name),
        }
    }
}
