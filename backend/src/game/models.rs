use std::{
    collections::HashMap,
    path::Path,
    sync::{LazyLock, Mutex},
};

use anyhow::Result;
use rusqlite::{Connection, types::Null};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

static CONNECTION: LazyLock<Mutex<Connection>> = LazyLock::new(|| {
    let path = if cfg!(debug_assertions) {
        Path::new(env!("OUT_DIR")).join("local.db")
    } else {
        Path::new("local.db").to_path_buf()
    };
    let conn = Connection::open(path.to_str().unwrap()).expect("failed to open local.db");
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS characters (
            id INTEGER PRIMARY KEY,
            data TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS maps (
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Map {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: String,
    pub width: i32,
    pub height: i32,
    pub actions: HashMap<i64, Vec<Action>>,
}

impl Identifiable for Map {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn set_id(&mut self, id: i64) {
        self.id = Some(id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub x: i32,
    pub y: i32,
    pub kind: ActionKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ActionKind {
    Move {
        allow_double_jump: bool,
        allow_adjustment: bool,
    },
    Wait(u64),
    Jump,
    Skill {
        skill: Skill,
        condition: UseCondition,
        site: UseSite,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum UseCondition {
    None,
    ErdaShowerOffCooldown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum UseSite {
    WithDoubleJump,
    AtProximity,
    AtExact,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Character {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: String,
    pub skills: Vec<Skill>,
}

impl Identifiable for Character {
    fn id(&self) -> Option<i64> {
        self.id
    }

    fn set_id(&mut self, id: i64) {
        self.id = Some(id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub kind: SkillKind,
    pub binding: SkillBinding,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SkillKind {
    RopeLift,
    UpJump,
    DoubleJump,
    Other,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum SkillBinding {
    W,
    Y,
    F,
    C,
    A,
}

pub(crate) fn query_maps() -> Result<Vec<Map>> {
    query_from_table("maps")
}

pub(crate) fn upsert_map(map: &mut Map) -> Result<()> {
    upsert_to_table("maps", map)
}

pub(crate) fn delete_map(map: &Map) -> Result<()> {
    delete_from_table("maps", map)
}

pub fn query_characters() -> Result<Vec<Character>> {
    query_from_table("characters")
}

pub fn upsert_character(character: &mut Character) -> Result<()> {
    upsert_to_table("characters", character)
}

pub fn delete_character(character: &Character) -> Result<()> {
    delete_from_table("characters", character)
}

fn query_from_table<T>(table: &str) -> Result<Vec<T>>
where
    T: DeserializeOwned + Identifiable,
{
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!("SELECT id, data FROM {}", table);
    let mut iter = conn.prepare(&stmt).unwrap();
    Ok(iter
        .query_map::<T, _, _>([], |row| {
            let id = row.get::<_, i64>(0).unwrap();
            let data = row.get::<_, String>(1).unwrap();
            let mut value = serde_json::from_str::<'_, T>(data.as_str()).unwrap();
            value.set_id(id);
            Ok(value)
        })?
        .filter_map(|c| c.ok())
        .collect::<Vec<_>>())
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
