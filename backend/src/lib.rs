#![feature(new_range_api)]
#![feature(slice_pattern)]
#![feature(map_try_insert)]
#![feature(variant_count)]
#![feature(let_chains)]
#![feature(iter_array_chunks)]
#![feature(associated_type_defaults)]
#![feature(assert_matches)]

use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow};
use tokio::sync::{
    broadcast, mpsc,
    oneshot::{self, Sender},
};

mod array;
mod bridge;
mod buff;
mod context;
mod database;
#[cfg(debug_assertions)]
mod debug;
mod detect;
mod llm;
mod mat;
mod minimap;
mod network;
mod pathing;
mod player;
mod request_handler;
mod rotator;
mod rpc;
mod skill;
mod task;

pub use {
    context::init,
    database::{
        Action, ActionCondition, ActionConfiguration, ActionKey, ActionKeyDirection, ActionKeyWith,
        ActionMove, AutoMobbing, Bound, CaptureMode, Class, Configuration, InputMethod, KeyBinding,
        KeyBindingConfiguration, LinkKeyBinding, Minimap, Notifications, Platform, Position,
        PotionMode, RotationMode, Settings, delete_map, query_configs, query_maps, query_settings,
        upsert_config, upsert_map, upsert_settings,
    },
    pathing::MAX_PLATFORMS_COUNT,
    rotator::RotatorMode,
    strum::{EnumMessage, IntoEnumIterator, ParseError},
};

type RequestItem = (Request, Sender<Response>);

static REQUESTS: LazyLock<(
    mpsc::Sender<RequestItem>,
    Mutex<mpsc::Receiver<RequestItem>>,
)> = LazyLock::new(|| {
    let (tx, rx) = mpsc::channel::<RequestItem>(10);
    (tx, Mutex::new(rx))
});

macro_rules! expect_unit_variant {
    ($e:expr, $p:path) => {
        match $e {
            $p => (),
            _ => unreachable!(),
        }
    };
}

macro_rules! expect_value_variant {
    ($e:expr, $p:path) => {
        match $e {
            $p(value) => value,
            _ => unreachable!(),
        }
    };
}

/// Represents request from UI
#[derive(Debug)]
enum Request {
    RotateActions(bool),
    RotateActionsHalting,
    CreateMinimap(String),
    UpdateMinimap(Option<String>, Minimap),
    UpdateConfiguration(Configuration),
    UpdateSettings(Settings),
    RedetectMinimap,
    GameState,
    MinimapFrame,
    MinimapPlatformsBound,
    KeyReceiver,
    QueryCaptureHandles,
    SelectCaptureHandle(Option<usize>),
    #[cfg(debug_assertions)]
    CaptureImage(bool),
    #[cfg(debug_assertions)]
    InferRune,
    #[cfg(debug_assertions)]
    InferMinimap,
    #[cfg(debug_assertions)]
    RecordImages(bool),
    #[cfg(debug_assertions)]
    TestSpinRune,
}

/// Represents response to UI [`Request`]
///
/// All internal (e.g. OpenCV) structs must be converted to either database structs
/// or appropriate counterparts before passing to UI.
#[derive(Debug)]
enum Response {
    RotateActions,
    RotateActionsHalting(bool),
    CreateMinimap(Option<Minimap>),
    UpdateMinimap,
    UpdateConfiguration,
    UpdateSettings,
    RedetectMinimap,
    GameState(GameState),
    MinimapFrame(Option<(Vec<u8>, usize, usize)>),
    MinimapPlatformsBound(Option<Bound>),
    KeyReceiver(broadcast::Receiver<KeyBinding>),
    QueryCaptureHandles((Vec<String>, Option<usize>)),
    SelectCaptureHandle,
    #[cfg(debug_assertions)]
    CaptureImage,
    #[cfg(debug_assertions)]
    InferRune,
    #[cfg(debug_assertions)]
    InferMinimap,
    #[cfg(debug_assertions)]
    RecordImages,
    #[cfg(debug_assertions)]
    TestSpinRune,
}

pub(crate) trait RequestHandler {
    fn on_rotate_actions(&mut self, halting: bool);

    fn on_rotate_actions_halting(&self) -> bool;

    fn on_create_minimap(&self, name: String) -> Option<Minimap>;

    fn on_update_minimap(&mut self, preset: Option<String>, minimap: Minimap);

    fn on_update_configuration(&mut self, config: Configuration);

    fn on_update_settings(&mut self, settings: Settings);

    fn on_redetect_minimap(&mut self);

    fn on_game_state(&self) -> GameState;

    fn on_minimap_frame(&self) -> Option<(Vec<u8>, usize, usize)>;

    fn on_minimap_platforms_bound(&self) -> Option<Bound>;

    fn on_key_receiver(&self) -> broadcast::Receiver<KeyBinding>;

    fn on_query_capture_handles(&mut self) -> (Vec<String>, Option<usize>);

    fn on_select_capture_handle(&mut self, index: Option<usize>);

    #[cfg(debug_assertions)]
    fn on_capture_image(&self, is_grayscale: bool);

    #[cfg(debug_assertions)]
    fn on_infer_rune(&mut self);

    #[cfg(debug_assertions)]
    fn on_infer_minimap(&self);

    #[cfg(debug_assertions)]
    fn on_record_images(&mut self, start: bool);

    #[cfg(debug_assertions)]
    fn on_test_spin_rune(&self);
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub position: Option<(i32, i32)>,
    pub health: Option<(u32, u32)>,
    pub state: String,
    pub normal_action: Option<String>,
    pub priority_action: Option<String>,
    pub erda_shower_state: String,
    pub destinations: Vec<(i32, i32)>,
}

pub async fn rotate_actions(halting: bool) {
    expect_unit_variant!(
        request(Request::RotateActions(halting)).await,
        Response::RotateActions
    )
}

pub async fn rotate_actions_halting() -> bool {
    expect_value_variant!(
        request(Request::RotateActionsHalting).await,
        Response::RotateActionsHalting
    )
}

pub async fn create_minimap(name: String) -> Option<Minimap> {
    expect_value_variant!(
        request(Request::CreateMinimap(name)).await,
        Response::CreateMinimap
    )
}

pub async fn update_minimap(preset: Option<String>, minimap: Minimap) {
    expect_unit_variant!(
        request(Request::UpdateMinimap(preset, minimap)).await,
        Response::UpdateMinimap
    )
}

pub async fn update_configuration(config: Configuration) {
    expect_unit_variant!(
        request(Request::UpdateConfiguration(config)).await,
        Response::UpdateConfiguration
    )
}

pub async fn update_settings(settings: Settings) {
    expect_unit_variant!(
        request(Request::UpdateSettings(settings)).await,
        Response::UpdateSettings
    )
}

pub async fn redetect_minimap() {
    expect_unit_variant!(
        request(Request::RedetectMinimap).await,
        Response::RedetectMinimap
    )
}

pub async fn player_state() -> GameState {
    expect_value_variant!(request(Request::GameState).await, Response::GameState)
}

pub async fn minimap_frame() -> Result<(Vec<u8>, usize, usize)> {
    expect_value_variant!(request(Request::MinimapFrame).await, Response::MinimapFrame)
        .ok_or(anyhow!("minimap frame not found"))
}

pub async fn minimap_platforms_bound() -> Option<Bound> {
    expect_value_variant!(
        request(Request::MinimapPlatformsBound).await,
        Response::MinimapPlatformsBound
    )
}

pub async fn key_receiver() -> broadcast::Receiver<KeyBinding> {
    expect_value_variant!(request(Request::KeyReceiver).await, Response::KeyReceiver)
}

pub async fn query_capture_handles() -> (Vec<String>, Option<usize>) {
    expect_value_variant!(
        request(Request::QueryCaptureHandles).await,
        Response::QueryCaptureHandles
    )
}

pub async fn select_capture_handle(index: Option<usize>) {
    expect_unit_variant!(
        request(Request::SelectCaptureHandle(index)).await,
        Response::SelectCaptureHandle
    )
}

#[cfg(debug_assertions)]
pub async fn capture_image(is_grayscale: bool) {
    expect_unit_variant!(
        request(Request::CaptureImage(is_grayscale)).await,
        Response::CaptureImage
    )
}

#[cfg(debug_assertions)]
pub async fn infer_rune() {
    expect_unit_variant!(request(Request::InferRune).await, Response::InferRune)
}

#[cfg(debug_assertions)]
pub async fn infer_minimap() {
    expect_unit_variant!(request(Request::InferMinimap).await, Response::InferMinimap)
}

#[cfg(debug_assertions)]
pub async fn record_images(start: bool) {
    expect_unit_variant!(
        request(Request::RecordImages(start)).await,
        Response::RecordImages
    )
}

#[cfg(debug_assertions)]
pub async fn test_spin_rune() {
    expect_unit_variant!(request(Request::TestSpinRune).await, Response::TestSpinRune)
}

pub(crate) fn poll_request(handler: &mut dyn RequestHandler) {
    if let Ok((request, sender)) = LazyLock::force(&REQUESTS).1.lock().unwrap().try_recv() {
        let result = match request {
            Request::RotateActions(halting) => {
                handler.on_rotate_actions(halting);
                Response::RotateActions
            }
            Request::RotateActionsHalting => {
                Response::RotateActionsHalting(handler.on_rotate_actions_halting())
            }
            Request::CreateMinimap(name) => {
                Response::CreateMinimap(handler.on_create_minimap(name))
            }
            Request::UpdateMinimap(preset, minimap) => {
                handler.on_update_minimap(preset, minimap);
                Response::UpdateMinimap
            }
            Request::UpdateConfiguration(config) => {
                handler.on_update_configuration(config);
                Response::UpdateConfiguration
            }
            Request::UpdateSettings(settings) => {
                handler.on_update_settings(settings);
                Response::UpdateSettings
            }
            Request::RedetectMinimap => {
                handler.on_redetect_minimap();
                Response::RedetectMinimap
            }
            Request::GameState => Response::GameState(handler.on_game_state()),
            Request::MinimapFrame => Response::MinimapFrame(handler.on_minimap_frame()),
            Request::MinimapPlatformsBound => {
                Response::MinimapPlatformsBound(handler.on_minimap_platforms_bound())
            }
            Request::KeyReceiver => Response::KeyReceiver(handler.on_key_receiver()),
            Request::QueryCaptureHandles => {
                Response::QueryCaptureHandles(handler.on_query_capture_handles())
            }
            Request::SelectCaptureHandle(index) => {
                handler.on_select_capture_handle(index);
                Response::SelectCaptureHandle
            }
            #[cfg(debug_assertions)]
            Request::CaptureImage(is_grayscale) => {
                handler.on_capture_image(is_grayscale);
                Response::CaptureImage
            }
            #[cfg(debug_assertions)]
            Request::InferRune => {
                handler.on_infer_rune();
                Response::InferRune
            }
            #[cfg(debug_assertions)]
            Request::InferMinimap => {
                handler.on_infer_minimap();
                Response::InferMinimap
            }
            #[cfg(debug_assertions)]
            Request::RecordImages(start) => {
                handler.on_record_images(start);
                Response::RecordImages
            }
            #[cfg(debug_assertions)]
            Request::TestSpinRune => {
                handler.on_test_spin_rune();
                Response::TestSpinRune
            }
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
