use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct Account {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub nickname: String,
    pub admin_level: i32,
    pub banned: bool,
    pub last_login: Option<String>,
    pub created_at: String,
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
    pub created_at: String,
    pub expires_at: String,
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
    pub colors: String,
    pub stats: String,
    pub created_at: String,
    pub last_login: Option<String>,
}
