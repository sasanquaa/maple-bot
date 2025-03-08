#![feature(str_from_raw_parts)]
#![feature(iter_array_chunks)]
#![feature(slice_pattern)]
#![feature(variant_count)]
#![feature(let_chains)]
#![feature(box_into_inner)]
#![feature(downcast_unchecked)]
#![feature(associated_type_defaults)]

use std::{
    any::Any,
    sync::{LazyLock, Mutex},
};

use anyhow::{Result, anyhow};
use tokio::sync::{
    mpsc,
    oneshot::{self, Sender},
};

mod buff;
mod context;
mod database;
#[cfg(debug_assertions)]
mod debug;
mod detect;
mod mat;
mod minimap;
mod player;
mod rotator;
mod skill;

pub use {
    context::start_update_loop,
    database::{
        Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove,
        Configuration, KeyBinding, KeyBindingConfiguration, Minimap, Position, RotationMode,
        delete_map, query_configs, upsert_config, upsert_map,
    },
    rotator::RotatorMode,
    strum::{IntoEnumIterator, ParseError},
};

type Response = (Sender<Box<dyn Any + Send>>, Request);

static REQUESTS: LazyLock<(mpsc::Sender<Response>, Mutex<mpsc::Receiver<Response>>)> =
    LazyLock::new(|| {
        let (tx, rx) = mpsc::channel::<Response>(10);
        (tx, Mutex::new(rx))
    });

#[derive(Debug)]
enum Request {
    RotateActions(bool),
    UpdateMinimap(Option<String>, Minimap),
    UpdateConfiguration(Configuration),
    RedetectMinimap,
    PlayerState,
    MinimapFrame,
    MinimapData,
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub position: Option<(i32, i32)>,
    pub state: String,
    pub normal_action: Option<String>,
    pub priority_action: Option<String>,
}

pub async fn rotate_actions(halting: bool) {
    request::<()>(Request::RotateActions(halting)).await
}

pub async fn update_minimap(preset: Option<String>, minimap: Minimap) {
    request::<()>(Request::UpdateMinimap(preset, minimap)).await
}

pub async fn update_configuration(config: Configuration) {
    request::<()>(Request::UpdateConfiguration(config)).await
}

pub async fn redetect_minimap() {
    request::<()>(Request::RedetectMinimap).await
}

pub async fn player_state() -> PlayerState {
    request::<PlayerState>(Request::PlayerState).await
}

pub async fn minimap_frame() -> Result<(Vec<u8>, usize, usize)> {
    request::<Option<(Vec<u8>, usize, usize)>>(Request::MinimapFrame)
        .await
        .ok_or(anyhow!("minimap frame not found"))
}

pub async fn minimap_data() -> Result<Minimap> {
    request::<Option<Minimap>>(Request::MinimapData)
        .await
        .ok_or(anyhow!("minimap data not found"))
}

pub(crate) fn poll_request(mut callback: impl FnMut(Request) -> Box<dyn Any + Send>) {
    if let Ok((sender, request)) = LazyLock::force(&REQUESTS).1.lock().unwrap().try_recv() {
        let _ = sender.send(callback(request));
    }
}

async fn request<T: Any + Send>(request: Request) -> T {
    let (tx, rx) = oneshot::channel();
    LazyLock::force(&REQUESTS)
        .0
        .send((tx, request))
        .await
        .unwrap();
    let result = rx.await.unwrap();
    // SAFETY: it is safe because it will crash if it is unsafe
    Box::into_inner(unsafe { result.downcast_unchecked::<T>() })
}
