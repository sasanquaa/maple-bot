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
use database::Minimap;
use tokio::sync::{
    mpsc,
    oneshot::{self, Sender},
};

pub mod context;
pub mod database;
#[cfg(debug_assertions)]
mod debug;
mod detect;
mod mat;
pub mod minimap;
pub mod player;
pub use strum::IntoEnumIterator;
mod rotator;
pub mod skill;

type Response = (Sender<Box<dyn Any + Send>>, Request);

static REQUESTS: LazyLock<(mpsc::Sender<Response>, Mutex<mpsc::Receiver<Response>>)> =
    LazyLock::new(|| {
        let (tx, rx) = mpsc::channel::<Response>(10);
        (tx, Mutex::new(rx))
    });

#[derive(Debug)]
enum Request {
    PrepareActions(String),
    RotateActions(bool),
    RedetectMinimap,
    RefreshMinimapData,
    PlayerPosition,
    MinimapFrame,
    MinimapData,
}

pub async fn prepare_actions(preset: String) {
    request::<()>(Request::PrepareActions(preset)).await
}

pub async fn rotate_actions(halting: bool) {
    request::<()>(Request::RotateActions(halting)).await
}

pub async fn redetect_minimap() {
    request::<()>(Request::RedetectMinimap).await
}

pub async fn refresh_minimap_data() {
    request::<()>(Request::RefreshMinimapData).await
}

pub async fn player_position() -> Result<(i32, i32)> {
    request::<Option<(i32, i32)>>(Request::PlayerPosition)
        .await
        .ok_or(anyhow!("player position not found"))
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
