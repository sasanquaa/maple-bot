use std::{
    collections::HashMap,
    env,
    sync::{LazyLock, Mutex},
};

use anyhow::Result;
use opencv::core::Rect;
use platforms::windows::KeyKind;
use rand::distr::{Alphanumeric, SampleString};
use rusqlite::{Connection, Params, Statement, types::Null};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use strum::{Display, EnumIter, EnumString};

use crate::pathing;

static CONNECTION: LazyLock<Mutex<Connection>> = LazyLock::new(|| {
    let path = env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .join("local.db")
        .to_path_buf();
    let conn = Connection::open(path.to_str().unwrap()).expect("failed to open local.db");
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS maps (
            id INTEGER PRIMARY KEY,
            data TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS configurations (
            id INTEGER PRIMARY KEY,
            data TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS settings (
            id INTEGER PRIMARY KEY,
            data TEXT NOT NULL
        );
        "#,
    )
    .unwrap();
    Mutex::new(conn)
});

trait Identifiable {
    fn id(&self) -> Option<i64>;

    fn set_id(&mut self, id: i64);
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum InputMethod {
    #[default]
    Default,
    Rpc,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Notifications {
    pub discord_webhook_url: String,
    pub discord_user_id: String,
    pub notify_on_fail_or_change_map: bool,
    pub notify_on_rune_appear: bool,
    pub notify_on_elite_boss_appear: bool,
    pub notify_on_player_die: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub capture_mode: CaptureMode,
    #[serde(default = "enable_rune_solving_default")]
    pub enable_rune_solving: bool,
    #[serde(default)]
    pub stop_on_fail_or_change_map: bool,
    #[serde(default)]
    pub input_method: InputMethod,
    #[serde(default)]
    pub input_method_rpc_server_url: String,
    #[serde(default)]
    pub notifications: Notifications,
    #[serde(default = "toggle_actions_key_default")]
    pub toggle_actions_key: KeyBindingConfiguration,
    #[serde(default = "platform_start_key_default")]
    pub platform_start_key: KeyBindingConfiguration,
    #[serde(default = "platform_end_key_default")]
    pub platform_end_key: KeyBindingConfiguration,
    #[serde(default = "platform_add_key_default")]
    pub platform_add_key: KeyBindingConfiguration,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            id: None,
            capture_mode: CaptureMode::default(),
            enable_rune_solving: enable_rune_solving_default(),
            input_method: InputMethod::default(),
            input_method_rpc_server_url: String::default(),
            stop_on_fail_or_change_map: false,
            notifications: Notifications::default(),
            toggle_actions_key: toggle_actions_key_default(),
            platform_start_key: platform_start_key_default(),
            platform_end_key: platform_end_key_default(),
            platform_add_key: platform_add_key_default(),
        }
    }
}

impl Identifiable for Settings {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn set_id(&mut self, id: i64) {
        self.id = Some(id);
    }
}

fn enable_rune_solving_default() -> bool {
    true
}

fn toggle_actions_key_default() -> KeyBindingConfiguration {
    KeyBindingConfiguration {
        key: KeyBinding::Comma,
        enabled: false,
    }
}

fn platform_start_key_default() -> KeyBindingConfiguration {
    KeyBindingConfiguration {
        key: KeyBinding::J,
        enabled: false,
    }
}

fn platform_end_key_default() -> KeyBindingConfiguration {
    KeyBindingConfiguration {
        key: KeyBinding::K,
        enabled: false,
    }
}

fn platform_add_key_default() -> KeyBindingConfiguration {
    KeyBindingConfiguration {
        key: KeyBinding::L,
        enabled: false,
    }
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum CaptureMode {
    #[default]
    BitBlt,
    #[strum(to_string = "Windows 10 (1903 and up)")] // Thanks OBS
    WindowsGraphicsCapture,
    BitBltArea,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Configuration {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: String,
    pub ropelift_key: KeyBindingConfiguration,
    pub teleport_key: Option<KeyBindingConfiguration>,
    #[serde(default = "jump_key_default")]
    pub jump_key: KeyBindingConfiguration,
    pub up_jump_key: Option<KeyBindingConfiguration>,
    pub interact_key: KeyBindingConfiguration,
    pub cash_shop_key: KeyBindingConfiguration,
    pub feed_pet_key: KeyBindingConfiguration,
    pub feed_pet_millis: u64,
    pub potion_key: KeyBindingConfiguration,
    pub potion_mode: PotionMode,
    pub health_update_millis: u64,
    pub sayram_elixir_key: KeyBindingConfiguration,
    pub aurelia_elixir_key: KeyBindingConfiguration,
    pub exp_x3_key: KeyBindingConfiguration,
    pub bonus_exp_key: KeyBindingConfiguration,
    pub legion_wealth_key: KeyBindingConfiguration,
    pub legion_luck_key: KeyBindingConfiguration,
    #[serde(default)]
    pub wealth_acquisition_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub exp_accumulation_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub extreme_red_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub extreme_blue_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub extreme_green_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub extreme_gold_potion_key: KeyBindingConfiguration,
    #[serde(default)]
    pub class: Class,
    #[serde(default)]
    pub actions: Vec<ActionConfiguration>,
}

fn jump_key_default() -> KeyBindingConfiguration {
    KeyBindingConfiguration {
        key: KeyBinding::Space,
        enabled: true,
    }
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            id: None,
            name: String::new(),
            ropelift_key: KeyBindingConfiguration::default(),
            teleport_key: None,
            jump_key: jump_key_default(),
            up_jump_key: None,
            interact_key: KeyBindingConfiguration::default(),
            cash_shop_key: KeyBindingConfiguration::default(),
            feed_pet_key: KeyBindingConfiguration::default(),
            feed_pet_millis: 320000,
            potion_key: KeyBindingConfiguration::default(),
            potion_mode: PotionMode::EveryMillis(180000),
            health_update_millis: 1000,
            sayram_elixir_key: KeyBindingConfiguration::default(),
            aurelia_elixir_key: KeyBindingConfiguration::default(),
            exp_x3_key: KeyBindingConfiguration::default(),
            bonus_exp_key: KeyBindingConfiguration::default(),
            legion_wealth_key: KeyBindingConfiguration::default(),
            legion_luck_key: KeyBindingConfiguration::default(),
            wealth_acquisition_potion_key: KeyBindingConfiguration::default(),
            exp_accumulation_potion_key: KeyBindingConfiguration::default(),
            extreme_red_potion_key: KeyBindingConfiguration::default(),
            extreme_blue_potion_key: KeyBindingConfiguration::default(),
            extreme_green_potion_key: KeyBindingConfiguration::default(),
            extreme_gold_potion_key: KeyBindingConfiguration::default(),
            class: Class::default(),
            actions: vec![],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize, EnumIter, Display, EnumString)]
pub enum PotionMode {
    EveryMillis(u64),
    Percentage(f32),
}

impl Default for PotionMode {
    fn default() -> Self {
        Self::EveryMillis(0)
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActionConfiguration {
    pub key: KeyBinding,
    pub every_millis: u64,
    pub require_stationary: bool,
    pub wait_before_use_millis: u64,
    pub wait_after_use_millis: u64,
    pub enabled: bool,
}

impl Default for ActionConfiguration {
    fn default() -> Self {
        // Template for a buff
        Self {
            key: KeyBinding::default(),
            every_millis: 180000,
            require_stationary: true,
            wait_before_use_millis: 500,
            wait_after_use_millis: 500,
            enabled: false,
        }
    }
}

impl From<ActionConfiguration> for Action {
    fn from(value: ActionConfiguration) -> Self {
        Self::Key(ActionKey {
            key: value.key,
            link_key: None,
            count: 1,
            position: None,
            condition: ActionCondition::EveryMillis(value.every_millis),
            direction: ActionKeyDirection::Any,
            with: if value.require_stationary {
                ActionKeyWith::Stationary
            } else {
                Default::default()
            },
            queue_to_front: Some(true),
            wait_before_use_millis: value.wait_before_use_millis,
            wait_before_use_millis_random_range: 0,
            wait_after_use_millis: value.wait_after_use_millis,
            wait_after_use_millis_random_range: 0,
        })
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct KeyBindingConfiguration {
    pub key: KeyBinding,
    pub enabled: bool,
}

#[derive(Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize)]
pub struct Bound {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl From<Bound> for Rect {
    fn from(value: Bound) -> Self {
        Self::new(value.x, value.y, value.width, value.height)
    }
}

impl From<Rect> for Bound {
    fn from(value: Rect) -> Self {
        Self {
            x: value.x,
            y: value.y,
            width: value.width,
            height: value.height,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct AutoMobbing {
    pub bound: Bound,
    pub key: KeyBinding,
    #[serde(default = "auto_mobbing_key_count_default")]
    pub key_count: u32,
    pub key_wait_before_millis: u64,
    pub key_wait_after_millis: u64,
}

impl Default for AutoMobbing {
    fn default() -> Self {
        Self {
            bound: Bound::default(),
            key: KeyBinding::default(),
            key_count: auto_mobbing_key_count_default(),
            key_wait_before_millis: 0,
            key_wait_after_millis: 0,
        }
    }
}

fn auto_mobbing_key_count_default() -> u32 {
    1
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum RotationMode {
    StartToEnd,
    #[default]
    StartToEndThenReverse,
    AutoMobbing(AutoMobbing),
}

impl Identifiable for Configuration {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn set_id(&mut self, id: i64) {
        self.id = Some(id)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
#[serde(default)]
pub struct Minimap {
    #[serde(skip_serializing)]
    pub id: Option<i64>,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub rotation_mode: RotationMode,
    pub platforms: Vec<Platform>,
    pub rune_platforms_pathing: bool,
    pub rune_platforms_pathing_up_jump_only: bool,
    pub auto_mob_platforms_pathing: bool,
    pub auto_mob_platforms_pathing_up_jump_only: bool,
    pub auto_mob_platforms_bound: bool,
    pub actions_any_reset_on_erda_condition: bool,
    pub actions: HashMap<String, Vec<Action>>,
}

impl Identifiable for Minimap {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn set_id(&mut self, id: i64) {
        self.id = Some(id)
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct Platform {
    pub x_start: i32,
    pub x_end: i32,
    pub y: i32,
}

impl From<Platform> for pathing::Platform {
    fn from(value: Platform) -> Self {
        Self::new(value.x_start..value.x_end, value.y)
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    #[serde(default)]
    pub x_random_range: i32,
    pub y: i32,
    pub allow_adjusting: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActionMove {
    pub position: Position,
    pub condition: ActionCondition,
    pub wait_after_move_millis: u64,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActionKey {
    pub key: KeyBinding,
    #[serde(default)]
    pub link_key: Option<LinkKeyBinding>,
    #[serde(default = "count_default")]
    pub count: u32,
    pub position: Option<Position>,
    pub condition: ActionCondition,
    pub direction: ActionKeyDirection,
    pub with: ActionKeyWith,
    pub wait_before_use_millis: u64,
    #[serde(default)]
    pub wait_before_use_millis_random_range: u64,
    pub wait_after_use_millis: u64,
    #[serde(default)]
    pub wait_after_use_millis_random_range: u64,
    pub queue_to_front: Option<bool>,
}

impl Default for ActionKey {
    fn default() -> Self {
        Self {
            key: KeyBinding::default(),
            link_key: None,
            count: count_default(),
            position: None,
            condition: ActionCondition::default(),
            direction: ActionKeyDirection::default(),
            with: ActionKeyWith::default(),
            wait_before_use_millis: 0,
            wait_before_use_millis_random_range: 0,
            wait_after_use_millis: 0,
            wait_after_use_millis_random_range: 0,
            queue_to_front: None,
        }
    }
}

#[derive(Clone, Copy, Display, EnumString, EnumIter, PartialEq, Debug, Serialize, Deserialize)]
pub enum LinkKeyBinding {
    Before(KeyBinding),
    AtTheSame(KeyBinding),
    After(KeyBinding),
    Along(KeyBinding),
}

impl LinkKeyBinding {
    pub fn key(&self) -> KeyBinding {
        match self {
            LinkKeyBinding::Before(key)
            | LinkKeyBinding::AtTheSame(key)
            | LinkKeyBinding::After(key)
            | LinkKeyBinding::Along(key) => *key,
        }
    }

    pub fn with_key(&self, key: KeyBinding) -> Self {
        match self {
            LinkKeyBinding::Before(_) => LinkKeyBinding::Before(key),
            LinkKeyBinding::AtTheSame(_) => LinkKeyBinding::AtTheSame(key),
            LinkKeyBinding::After(_) => LinkKeyBinding::After(key),
            LinkKeyBinding::Along(_) => LinkKeyBinding::Along(key),
        }
    }
}

impl Default for LinkKeyBinding {
    fn default() -> Self {
        LinkKeyBinding::Before(KeyBinding::default())
    }
}

fn count_default() -> u32 {
    1
}

#[derive(
    Clone, Copy, Display, Default, EnumString, EnumIter, PartialEq, Debug, Serialize, Deserialize,
)]
pub enum Class {
    Cadena,
    Blaster,
    Ark,
    #[default]
    Generic,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize, EnumIter, Display, EnumString)]
pub enum Action {
    Move(ActionMove),
    Key(ActionKey),
}

#[derive(
    Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum ActionCondition {
    #[default]
    Any,
    EveryMillis(u64),
    ErdaShowerOffCooldown,
    Linked,
}

#[derive(
    Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum ActionKeyWith {
    #[default]
    Any,
    Stationary,
    DoubleJump,
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum ActionKeyDirection {
    #[default]
    Any,
    Left,
    Right,
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum KeyBinding {
    #[default]
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Zero,
    One,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Enter,
    Space,
    Tilde,
    Quote,
    Semicolon,
    Comma,
    Period,
    Slash,
    Esc,
    Shift,
    Ctrl,
    Alt,
}

impl From<KeyBinding> for KeyKind {
    fn from(value: KeyBinding) -> Self {
        match value {
            KeyBinding::A => KeyKind::A,
            KeyBinding::B => KeyKind::B,
            KeyBinding::C => KeyKind::C,
            KeyBinding::D => KeyKind::D,
            KeyBinding::E => KeyKind::E,
            KeyBinding::F => KeyKind::F,
            KeyBinding::G => KeyKind::G,
            KeyBinding::H => KeyKind::H,
            KeyBinding::I => KeyKind::I,
            KeyBinding::J => KeyKind::J,
            KeyBinding::K => KeyKind::K,
            KeyBinding::L => KeyKind::L,
            KeyBinding::M => KeyKind::M,
            KeyBinding::N => KeyKind::N,
            KeyBinding::O => KeyKind::O,
            KeyBinding::P => KeyKind::P,
            KeyBinding::Q => KeyKind::Q,
            KeyBinding::R => KeyKind::R,
            KeyBinding::S => KeyKind::S,
            KeyBinding::T => KeyKind::T,
            KeyBinding::U => KeyKind::U,
            KeyBinding::V => KeyKind::V,
            KeyBinding::W => KeyKind::W,
            KeyBinding::X => KeyKind::X,
            KeyBinding::Y => KeyKind::Y,
            KeyBinding::Z => KeyKind::Z,
            KeyBinding::Zero => KeyKind::Zero,
            KeyBinding::One => KeyKind::One,
            KeyBinding::Two => KeyKind::Two,
            KeyBinding::Three => KeyKind::Three,
            KeyBinding::Four => KeyKind::Four,
            KeyBinding::Five => KeyKind::Five,
            KeyBinding::Six => KeyKind::Six,
            KeyBinding::Seven => KeyKind::Seven,
            KeyBinding::Eight => KeyKind::Eight,
            KeyBinding::Nine => KeyKind::Nine,
            KeyBinding::F1 => KeyKind::F1,
            KeyBinding::F2 => KeyKind::F2,
            KeyBinding::F3 => KeyKind::F3,
            KeyBinding::F4 => KeyKind::F4,
            KeyBinding::F5 => KeyKind::F5,
            KeyBinding::F6 => KeyKind::F6,
            KeyBinding::F7 => KeyKind::F7,
            KeyBinding::F8 => KeyKind::F8,
            KeyBinding::F9 => KeyKind::F9,
            KeyBinding::F10 => KeyKind::F10,
            KeyBinding::F11 => KeyKind::F11,
            KeyBinding::F12 => KeyKind::F12,
            KeyBinding::Up => KeyKind::Up,
            KeyBinding::Down => KeyKind::Down,
            KeyBinding::Left => KeyKind::Left,
            KeyBinding::Right => KeyKind::Right,
            KeyBinding::Home => KeyKind::Home,
            KeyBinding::End => KeyKind::End,
            KeyBinding::PageUp => KeyKind::PageUp,
            KeyBinding::PageDown => KeyKind::PageDown,
            KeyBinding::Insert => KeyKind::Insert,
            KeyBinding::Delete => KeyKind::Delete,
            KeyBinding::Enter => KeyKind::Enter,
            KeyBinding::Space => KeyKind::Space,
            KeyBinding::Tilde => KeyKind::Tilde,
            KeyBinding::Quote => KeyKind::Quote,
            KeyBinding::Semicolon => KeyKind::Semicolon,
            KeyBinding::Comma => KeyKind::Comma,
            KeyBinding::Period => KeyKind::Period,
            KeyBinding::Slash => KeyKind::Slash,
            KeyBinding::Esc => KeyKind::Esc,
            KeyBinding::Shift => KeyKind::Shift,
            KeyBinding::Ctrl => KeyKind::Ctrl,
            KeyBinding::Alt => KeyKind::Alt,
        }
    }
}

impl From<KeyKind> for KeyBinding {
    fn from(value: KeyKind) -> Self {
        match value {
            KeyKind::A => KeyBinding::A,
            KeyKind::B => KeyBinding::B,
            KeyKind::C => KeyBinding::C,
            KeyKind::D => KeyBinding::D,
            KeyKind::E => KeyBinding::E,
            KeyKind::F => KeyBinding::F,
            KeyKind::G => KeyBinding::G,
            KeyKind::H => KeyBinding::H,
            KeyKind::I => KeyBinding::I,
            KeyKind::J => KeyBinding::J,
            KeyKind::K => KeyBinding::K,
            KeyKind::L => KeyBinding::L,
            KeyKind::M => KeyBinding::M,
            KeyKind::N => KeyBinding::N,
            KeyKind::O => KeyBinding::O,
            KeyKind::P => KeyBinding::P,
            KeyKind::Q => KeyBinding::Q,
            KeyKind::R => KeyBinding::R,
            KeyKind::S => KeyBinding::S,
            KeyKind::T => KeyBinding::T,
            KeyKind::U => KeyBinding::U,
            KeyKind::V => KeyBinding::V,
            KeyKind::W => KeyBinding::W,
            KeyKind::X => KeyBinding::X,
            KeyKind::Y => KeyBinding::Y,
            KeyKind::Z => KeyBinding::Z,
            KeyKind::Zero => KeyBinding::Zero,
            KeyKind::One => KeyBinding::One,
            KeyKind::Two => KeyBinding::Two,
            KeyKind::Three => KeyBinding::Three,
            KeyKind::Four => KeyBinding::Four,
            KeyKind::Five => KeyBinding::Five,
            KeyKind::Six => KeyBinding::Six,
            KeyKind::Seven => KeyBinding::Seven,
            KeyKind::Eight => KeyBinding::Eight,
            KeyKind::Nine => KeyBinding::Nine,
            KeyKind::F1 => KeyBinding::F1,
            KeyKind::F2 => KeyBinding::F2,
            KeyKind::F3 => KeyBinding::F3,
            KeyKind::F4 => KeyBinding::F4,
            KeyKind::F5 => KeyBinding::F5,
            KeyKind::F6 => KeyBinding::F6,
            KeyKind::F7 => KeyBinding::F7,
            KeyKind::F8 => KeyBinding::F8,
            KeyKind::F9 => KeyBinding::F9,
            KeyKind::F10 => KeyBinding::F10,
            KeyKind::F11 => KeyBinding::F11,
            KeyKind::F12 => KeyBinding::F12,
            KeyKind::Up => KeyBinding::Up,
            KeyKind::Down => KeyBinding::Down,
            KeyKind::Left => KeyBinding::Left,
            KeyKind::Right => KeyBinding::Right,
            KeyKind::Home => KeyBinding::Home,
            KeyKind::End => KeyBinding::End,
            KeyKind::PageUp => KeyBinding::PageUp,
            KeyKind::PageDown => KeyBinding::PageDown,
            KeyKind::Insert => KeyBinding::Insert,
            KeyKind::Delete => KeyBinding::Delete,
            KeyKind::Enter => KeyBinding::Enter,
            KeyKind::Space => KeyBinding::Space,
            KeyKind::Tilde => KeyBinding::Tilde,
            KeyKind::Quote => KeyBinding::Quote,
            KeyKind::Semicolon => KeyBinding::Semicolon,
            KeyKind::Comma => KeyBinding::Comma,
            KeyKind::Period => KeyBinding::Period,
            KeyKind::Slash => KeyBinding::Slash,
            KeyKind::Esc => KeyBinding::Esc,
            KeyKind::Shift => KeyBinding::Shift,
            KeyKind::Ctrl => KeyBinding::Ctrl,
            KeyKind::Alt => KeyBinding::Alt,
        }
    }
}

pub fn query_settings() -> Settings {
    let mut settings = query_from_table("settings").unwrap().into_iter().next();
    if settings.is_none() {
        settings = Some(Settings::default());
        upsert_settings(settings.as_mut().unwrap()).unwrap();
    }
    settings.unwrap()
}

pub fn upsert_settings(settings: &mut Settings) -> Result<()> {
    upsert_to_table("settings", settings)
}

pub fn query_configs() -> Result<Vec<Configuration>> {
    let mut result = query_from_table("configurations");
    if let Ok(vec) = result.as_mut() {
        if vec.is_empty() {
            let mut config = Configuration {
                name: "default".to_string(),
                ..Configuration::default()
            };
            upsert_config(&mut config).unwrap();
            vec.push(config);
        } else {
            vec.iter_mut().for_each(|config| {
                if config.name.is_empty() {
                    config.name = Alphanumeric.sample_string(&mut rand::rng(), 8);
                    upsert_config(config).unwrap();
                }
            });
        }
    }
    result
}

pub fn upsert_config(config: &mut Configuration) -> Result<()> {
    upsert_to_table("configurations", config)
}

pub fn query_maps() -> Result<Vec<Minimap>> {
    query_from_table("maps")
}

pub fn upsert_map(map: &mut Minimap) -> Result<()> {
    upsert_to_table("maps", map)
}

pub fn delete_map(map: &Minimap) -> Result<()> {
    delete_from_table("maps", map)
}

fn map_data<T>(mut stmt: Statement<'_>, params: impl Params) -> Result<Vec<T>>
where
    T: DeserializeOwned + Identifiable + Default,
{
    Ok(stmt
        .query_map::<T, _, _>(params, |row| {
            let id = row.get::<_, i64>(0).unwrap();
            let data = row.get::<_, String>(1).unwrap();
            let mut value = serde_json::from_str::<'_, T>(data.as_str()).unwrap_or_default();
            value.set_id(id);
            Ok(value)
        })?
        .filter_map(|c| c.ok())
        .collect::<Vec<_>>())
}

fn query_from_table<T>(table: &str) -> Result<Vec<T>>
where
    T: DeserializeOwned + Identifiable + Default,
{
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!("SELECT id, data FROM {table}");
    let stmt = conn.prepare(&stmt).unwrap();
    map_data(stmt, [])
}

fn upsert_to_table<T>(table: &str, data: &mut T) -> Result<()>
where
    T: Serialize + Identifiable,
{
    let json = serde_json::to_string(&data).unwrap();
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!(
        "INSERT INTO {table} (id, data) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET data = ?2;",
    );
    match data.id() {
        Some(id) => {
            conn.execute(&stmt, (id, &json))?;
            Ok(())
        }
        None => {
            conn.execute(&stmt, (Null, &json))?;
            data.set_id(conn.last_insert_rowid());
            Ok(())
        }
    }
}

fn delete_from_table<T: Identifiable>(table: &str, data: &T) -> Result<()> {
    fn inner(table: &str, id: Option<i64>) -> Result<()> {
        if id.is_some() {
            let conn = CONNECTION.lock().unwrap();
            let stmt = format!("DELETE FROM {table} WHERE id = ?1;");
            conn.execute(&stmt, [id.unwrap()])?;
        }
        Ok(())
    }
    inner(table, data.id())
}
