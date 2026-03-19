use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::types::JsonValue;

#[derive(Debug, Clone, FromRow)]
pub struct Account {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub nickname: String,
    pub admin_level: i32,
    pub banned: bool,
    pub last_login: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Server {
    pub id: i64,
    pub name: String,
    pub address: String,
    pub port: i32,
    pub status: i32,
    pub completion: i32,
    pub community: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct Ticket {
    pub ticket: String,
    pub account_id: i64,
    pub server_id: i64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Character {
    pub id: i64,
    pub account_id: i64,
    pub name: String,
    pub breed_id: i32,
    pub sex: i32,
    pub level: i32,
    pub experience: i64,
    pub kamas: i64,
    pub map_id: i64,
    pub cell_id: i32,
    pub direction: i32,
    pub colors: JsonValue,
    pub stats: JsonValue,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct Spell {
    pub character_id: i64,
    pub spell_id: i32,
    pub level: i32,
    pub position: i32,
}

#[derive(Debug, Clone, FromRow)]
pub struct NpcSpawn {
    pub id: i32,
    pub npc_id: i32,
    pub map_id: i64,
    pub cell_id: i32,
    pub direction: i32,
    pub look: String,
}

#[derive(Debug, Clone, FromRow)]
pub struct CharacterQuest {
    pub character_id: i64,
    pub quest_id: i32,
    pub step_id: i32,
    pub status: i32, // 0=active, 1=completed
    pub objectives: JsonValue,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct GameData {
    pub file_name: String,
    pub object_id: i32,
    pub class_name: String,
    pub data: JsonValue,
}
