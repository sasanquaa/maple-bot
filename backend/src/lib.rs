#![feature(str_from_raw_parts)]
#![feature(maybe_uninit_uninit_array)]
#![feature(iter_array_chunks)]
#![feature(new_range_api)]
#![feature(slice_pattern)]
#![feature(variant_count)]
#![feature(let_chains)]
#![feature(box_into_inner)]
#![feature(downcast_unchecked)]
#![feature(associated_type_defaults)]
#![feature(assert_matches)]

use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow};
use tokio::sync::{
    mpsc,
    oneshot::{self, Sender},
};

mod array;
mod buff;
mod context;
mod database;
#[cfg(debug_assertions)]
mod debug;
mod detect;
mod mat;
mod minimap;
mod pathing;
mod player;
mod rotator;
mod skill;
mod task;

pub use {
    context::start_update_loop,
    database::{
        Action, ActionCondition, ActionKey, ActionKeyDirection, ActionKeyWith, ActionMove,
        AutoMobbing, Bound, Configuration, KeyBinding, KeyBindingConfiguration, Minimap, Platform,
        Position, PotionMode, RotationMode, delete_map, query_configs, upsert_config, upsert_map,
    },
    rotator::RotatorMode,
    strum::{IntoEnumIterator, ParseError},
};

type RequestItem = (Request, Sender<Response>);

static REQUESTS: LazyLock<(
    mpsc::Sender<RequestItem>,
    Mutex<mpsc::Receiver<RequestItem>>,
)> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel::<RequestItem>(10);
    (tx, Mutex::new(rx))
});

macro_rules! expect_variant {
    ($e:expr, $p:path) => {
        match $e {
            $p(value) => value,
            _ => unreachable!(),
        }
    };
}

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

#[derive(Debug)]
enum Response {
    RotateActions(()),
    UpdateMinimap(()),
    UpdateConfiguration(()),
    RedetectMinimap(()),
    PlayerState(PlayerState),
    MinimapFrame(Option<(Vec<u8>, usize, usize)>),
    MinimapData(Option<Minimap>),
}

pub(crate) trait RequestHandler {
    fn on_rotate_actions(&mut self, halting: bool);

    fn on_update_minimap(&mut self, preset: Option<String>, minimap: Minimap);

    fn on_update_configuration(&mut self, config: Configuration);

    fn on_redetect_minimap(&mut self);

    fn on_player_state(&mut self) -> PlayerState;

    fn on_minimap_frame(&mut self) -> Option<(Vec<u8>, usize, usize)>;

    fn on_minimap_data(&mut self) -> Option<Minimap>;
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub position: Option<(i32, i32)>,
    pub health: Option<(u32, u32)>,
    pub state: String,
    pub normal_action: Option<String>,
    pub priority_action: Option<String>,
    pub erda_shower_state: String,
    pub destinations: Vec<(i32, i32)>,
}

pub async fn rotate_actions(halting: bool) {
    expect_variant!(
        request(Request::RotateActions(halting)).await,
        Response::RotateActions
    )
}

pub async fn update_minimap(preset: Option<String>, minimap: Minimap) {
    expect_variant!(
        request(Request::UpdateMinimap(preset, minimap)).await,
        Response::UpdateMinimap
    )
}

pub async fn update_configuration(config: Configuration) {
    expect_variant!(
        request(Request::UpdateConfiguration(config)).await,
        Response::UpdateConfiguration
    )
}

pub async fn redetect_minimap() {
    expect_variant!(
        request(Request::RedetectMinimap).await,
        Response::RedetectMinimap
    )
}

pub async fn player_state() -> PlayerState {
    expect_variant!(request(Request::PlayerState).await, Response::PlayerState)
}

pub async fn minimap_frame() -> Result<(Vec<u8>, usize, usize)> {
    expect_variant!(request(Request::MinimapFrame).await, Response::MinimapFrame)
        .ok_or(anyhow!("minimap frame not found"))
}

pub async fn minimap_data() -> Result<Minimap> {
    expect_variant!(request(Request::MinimapData).await, Response::MinimapData)
        .ok_or(anyhow!("minimap data not found"))
}

pub(crate) fn poll_request(handler: &mut dyn RequestHandler) {
    if let Ok((request, sender)) = LazyLock::force(&REQUESTS).1.lock().unwrap().try_recv() {
        let result = match request {
            Request::RotateActions(halting) => {
                handler.on_rotate_actions(halting);
                Response::RotateActions(())
            }
            Request::UpdateMinimap(preset, minimap) => {
                handler.on_update_minimap(preset, minimap);
                Response::UpdateMinimap(())
            }
            Request::UpdateConfiguration(config) => {
                handler.on_update_configuration(config);
                Response::UpdateConfiguration(())
            }
            Request::RedetectMinimap => {
                handler.on_redetect_minimap();
                Response::RedetectMinimap(())
            }
            Request::PlayerState => Response::PlayerState(handler.on_player_state()),
            Request::MinimapFrame => Response::MinimapFrame(handler.on_minimap_frame()),
            Request::MinimapData => Response::MinimapData(handler.on_minimap_data()),
        };
        let _ = sender.send(result);
    }
}

async fn request(request: Request) -> Response {
    let (tx, rx) = oneshot::channel();
    LazyLock::force(&REQUESTS)
        .0
        .send((request, tx))
        .await
        .unwrap();
    rx.await.unwrap()
}
