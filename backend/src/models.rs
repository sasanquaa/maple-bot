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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Map {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub actions: HashMap<i64, Vec<Action>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Action {
    pub x: i32,
    pub y: i32,
    pub kind: ActionKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum ActionKind {
    Move {
        allow_double_jump: bool,
        allow_adjustment: bool,
    },
    Wait(u64),
    Jump,
    DoubleJump,
    UpJump,
    Grappling,
    Attack {
        skill: Skill,
        allow_double_jump: bool,
    },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Character {
    #[serde(skip_serializing, default)]
    pub id: Option<i64>,
    pub name: String,
    pub skills: Vec<Skill>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Skill {
    ErdaShower(char),
    RopeLift(char),
    UpJump { is_composite: bool },
    Single(char),
}

pub fn query_maps() -> Result<Vec<Map>> {
    query_from_table("maps", |id, map| Map {
        id: Some(id),
        ..map
    })
}

pub fn upsert_map(map: &mut Map) -> Result<()> {
    if let Some(id) = upsert_to_table("maps", map.id, map)? {
        map.id = Some(id)
    }
    Ok(())
}

pub fn delete_map(id: i64) -> Result<()> {
    delete_from_table("maps", id)
}

pub fn query_characters() -> Result<Vec<Character>> {
    query_from_table("characters", |id, character| Character {
        id: Some(id),
        ..character
    })
}

pub fn upsert_character(character: &mut Character) -> Result<()> {
    if let Some(id) = upsert_to_table("characters", character.id, character)? {
        character.id = Some(id)
    }
    Ok(())
}

pub fn delete_character(id: i64) -> Result<()> {
    delete_from_table("characters", id)
}

fn query_from_table<T>(table: &str, reduce: impl Fn(i64, T) -> T) -> Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!("SELECT id, data FROM {}", table);
    let mut iter = conn.prepare(&stmt)?;
    Ok(iter
        .query_map::<T, _, _>([], |row| {
            let id = row.get::<_, i64>(0)?;
            let data = row.get::<_, String>(1)?;
            let value = serde_json::from_str(data.as_str()).unwrap();
            Ok(reduce(id, value))
        })?
        .filter_map(|c| c.ok())
        .collect::<Vec<_>>())
}

fn upsert_to_table<T>(table: &str, id: Option<i64>, data: &T) -> Result<Option<i64>>
where
    T: Serialize,
{
    let data = serde_json::to_string(&data)?;
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!(
        "INSERT INTO {} (id, data) VALUES (?1, ?2) ON CONFLICT (id) DO UPDATE SET data = ?2;",
        table
    );
    match id {
        Some(id) => {
            conn.execute(&stmt, (id, &data))?;
            Ok(None)
        }
        None => {
            conn.execute(&stmt, (Null, &data))?;
            Ok(Some(conn.last_insert_rowid()))
        }
    }
}

fn delete_from_table(table: &str, id: i64) -> Result<()> {
    let conn = CONNECTION.lock().unwrap();
    let stmt = format!("DELETE FROM {} WHERE id = ?1;", table);
    conn.execute(&stmt, [id])?;
    Ok(())
}
