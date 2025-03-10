use std::{
    collections::HashMap,
    env,
    sync::{LazyLock, Mutex},
};

use anyhow::Result;
use platforms::windows::KeyKind;
use rand::distr::{Alphanumeric, SampleString};
use rusqlite::{Connection, Params, Statement, types::Null};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use strum::{Display, EnumIter, EnumString};

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
        "#,
    )
    .unwrap();
    Mutex::new(conn)
});

trait Identifiable {
    fn id(&self) -> Option<i64>;

    fn set_id(&mut self, id: i64);
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Configuration {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: String,
    pub ropelift_key: KeyBindingConfiguration,
    pub up_jump_key: Option<KeyBindingConfiguration>,
    pub interact_key: KeyBindingConfiguration,
    pub cash_shop_key: KeyBindingConfiguration,
    pub feed_pet_key: KeyBindingConfiguration,
    pub potion_key: KeyBindingConfiguration,
    pub rotation_mode: RotationMode,
    pub sayram_elixir_key: KeyBindingConfiguration,
    pub exp_x3_key: KeyBindingConfiguration,
    pub bonus_exp_key: KeyBindingConfiguration,
    pub legion_wealth_key: KeyBindingConfiguration,
    pub legion_luck_key: KeyBindingConfiguration,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct KeyBindingConfiguration {
    pub key: KeyBinding,
    pub enabled: bool,
}

impl Default for KeyBindingConfiguration {
    fn default() -> Self {
        Self {
            key: KeyBinding::default(),
            enabled: true,
        }
    }
}

#[derive(
    Clone, Copy, PartialEq, Default, Debug, Serialize, Deserialize, EnumIter, Display, EnumString,
)]
pub enum RotationMode {
    StartToEnd,
    #[default]
    StartToEndThenReverse,
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
pub struct Minimap {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: String,
    pub width: i32,
    pub height: i32,
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

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub allow_adjusting: bool,
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActionMove {
    pub position: Position,
    pub condition: ActionCondition,
    pub wait_after_move_ticks: u32,
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct ActionKey {
    pub key: KeyBinding,
    pub position: Option<Position>,
    pub condition: ActionCondition,
    pub direction: ActionKeyDirection,
    pub with: ActionKeyWith,
    pub wait_before_use_ticks: u32,
    pub wait_after_use_ticks: u32,
    pub queue_to_front: Option<bool>,
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
    let stmt = format!("SELECT id, data FROM {}", table);
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
        "INSERT INTO {} (id, data) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET data = ?2;",
        table
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
            let stmt = format!("DELETE FROM {} WHERE id = ?1;", table);
            conn.execute(&stmt, [id.unwrap()])?;
        }
        Ok(())
    }
    inner(table, data.id())
}
