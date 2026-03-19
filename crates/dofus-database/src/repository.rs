use crate::models::{Account, Character, GameData, Server, Spell, Ticket};
use sqlx::PgPool;

// --- Accounts ---

pub async fn find_account_by_username(
    pool: &PgPool,
    username: &str,
) -> anyhow::Result<Option<Account>> {
    let account = sqlx::query_as::<_, Account>(
        "SELECT * FROM accounts WHERE username = $1",
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;
    Ok(account)
}

pub async fn create_account(
    pool: &PgPool,
    username: &str,
    password_hash: &str,
    nickname: &str,
) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "INSERT INTO accounts (username, password_hash, nickname) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(username)
    .bind(password_hash)
    .bind(nickname)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

// --- Servers ---

pub async fn get_all_servers(pool: &PgPool) -> anyhow::Result<Vec<Server>> {
    let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers")
        .fetch_all(pool)
        .await?;
    Ok(servers)
}

pub async fn get_server_by_id(
    pool: &PgPool,
    server_id: i64,
) -> anyhow::Result<Option<Server>> {
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = $1")
        .bind(server_id)
        .fetch_optional(pool)
        .await?;
    Ok(server)
}

pub async fn insert_server(
    pool: &PgPool,
    id: i64,
    name: &str,
    address: &str,
    port: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO servers (id, name, address, port, status, completion, community)
         VALUES ($1, $2, $3, $4, 3, 0, 0)
         ON CONFLICT (id) DO UPDATE SET name = $2, address = $3, port = $4",
    )
    .bind(id)
    .bind(name)
    .bind(address)
    .bind(port)
    .execute(pool)
    .await?;
    Ok(())
}

// --- Tickets ---

pub async fn create_ticket(
    pool: &PgPool,
    ticket: &str,
    account_id: i64,
    server_id: i64,
    expires_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO tickets (ticket, account_id, server_id, expires_at)
         VALUES ($1, $2, $3, $4::timestamptz)",
    )
    .bind(ticket)
    .bind(account_id)
    .bind(server_id)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn consume_ticket(
    pool: &PgPool,
    ticket: &str,
) -> anyhow::Result<Option<Ticket>> {
    let row = sqlx::query_as::<_, Ticket>(
        "DELETE FROM tickets WHERE ticket = $1 AND expires_at > NOW() RETURNING *",
    )
    .bind(ticket)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// --- Characters ---

pub async fn list_characters(
    pool: &PgPool,
    account_id: i64,
) -> anyhow::Result<Vec<Character>> {
    let characters = sqlx::query_as::<_, Character>(
        "SELECT * FROM characters WHERE account_id = $1 ORDER BY id",
    )
    .bind(account_id)
    .fetch_all(pool)
    .await?;
    Ok(characters)
}

pub async fn get_character(
    pool: &PgPool,
    character_id: i64,
) -> anyhow::Result<Option<Character>> {
    let character = sqlx::query_as::<_, Character>(
        "SELECT * FROM characters WHERE id = $1",
    )
    .bind(character_id)
    .fetch_optional(pool)
    .await?;
    Ok(character)
}

pub async fn get_character_for_account(
    pool: &PgPool,
    character_id: i64,
    account_id: i64,
) -> anyhow::Result<Option<Character>> {
    let character = sqlx::query_as::<_, Character>(
        "SELECT * FROM characters WHERE id = $1 AND account_id = $2",
    )
    .bind(character_id)
    .bind(account_id)
    .fetch_optional(pool)
    .await?;
    Ok(character)
}

/// Default spawn: Incarnam statue.
const DEFAULT_MAP_ID: i64 = 154010883;
const DEFAULT_CELL_ID: i32 = 297;

pub async fn create_character(
    pool: &PgPool,
    account_id: i64,
    name: &str,
    breed_id: i32,
    sex: i32,
    colors: &serde_json::Value,
) -> anyhow::Result<Character> {
    let default_stats = serde_json::json!({
        "vitality": 0,
        "wisdom": 0,
        "strength": 0,
        "intelligence": 0,
        "chance": 0,
        "agility": 0
    });
    let character = sqlx::query_as::<_, Character>(
        "INSERT INTO characters (account_id, name, breed_id, sex, colors, map_id, cell_id, direction, stats)
         VALUES ($1, $2, $3, $4, $5, $6, $7, 1, $8) RETURNING *",
    )
    .bind(account_id)
    .bind(name)
    .bind(breed_id)
    .bind(sex)
    .bind(colors)
    .bind(DEFAULT_MAP_ID)
    .bind(DEFAULT_CELL_ID)
    .bind(&default_stats)
    .fetch_one(pool)
    .await?;

    // Insert breed starting spells
    insert_breed_spells(pool, character.id, breed_id).await?;

    Ok(character)
}

pub async fn character_name_exists(
    pool: &PgPool,
    name: &str,
) -> anyhow::Result<bool> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM characters WHERE name = $1",
    )
    .bind(name)
    .fetch_one(pool)
    .await?;
    Ok(row.0 > 0)
}

// --- Game Data (D2O import) ---

pub async fn upsert_game_data(
    pool: &PgPool,
    file_name: &str,
    object_id: i32,
    class_name: &str,
    data: &serde_json::Value,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO game_data (file_name, object_id, class_name, data)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (file_name, object_id) DO UPDATE SET class_name = $3, data = $4",
    )
    .bind(file_name)
    .bind(object_id)
    .bind(class_name)
    .bind(data)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_game_data(
    pool: &PgPool,
    file_name: &str,
    object_id: i32,
) -> anyhow::Result<Option<GameData>> {
    let row = sqlx::query_as::<_, GameData>(
        "SELECT * FROM game_data WHERE file_name = $1 AND object_id = $2",
    )
    .bind(file_name)
    .bind(object_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

pub async fn get_all_game_data(
    pool: &PgPool,
    file_name: &str,
) -> anyhow::Result<Vec<GameData>> {
    let rows = sqlx::query_as::<_, GameData>(
        "SELECT * FROM game_data WHERE file_name = $1 ORDER BY object_id",
    )
    .bind(file_name)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn count_game_data(
    pool: &PgPool,
    file_name: &str,
) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM game_data WHERE file_name = $1",
    )
    .bind(file_name)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

// --- D2I Translations ---

pub async fn upsert_game_text(
    pool: &PgPool,
    file_name: &str,
    text_id: i32,
    text: &str,
    undiacritical: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO game_texts (file_name, text_id, text, undiacritical)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (file_name, text_id) DO UPDATE SET text = $3, undiacritical = $4",
    )
    .bind(file_name)
    .bind(text_id)
    .bind(text)
    .bind(undiacritical)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_game_named_text(
    pool: &PgPool,
    file_name: &str,
    text_key: &str,
    text: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO game_named_texts (file_name, text_key, text)
         VALUES ($1, $2, $3)
         ON CONFLICT (file_name, text_key) DO UPDATE SET text = $3",
    )
    .bind(file_name)
    .bind(text_key)
    .bind(text)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_all_game_texts(
    pool: &PgPool,
    file_name: &str,
) -> anyhow::Result<Vec<(i32, String, Option<String>)>> {
    let rows: Vec<(i32, String, Option<String>)> = sqlx::query_as(
        "SELECT text_id, text, undiacritical FROM game_texts WHERE file_name = $1 ORDER BY text_id",
    )
    .bind(file_name)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_all_game_named_texts(
    pool: &PgPool,
    file_name: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT text_key, text FROM game_named_texts WHERE file_name = $1 ORDER BY text_key",
    )
    .bind(file_name)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// --- D2P Files ---

pub async fn upsert_game_file(
    pool: &PgPool,
    archive: &str,
    file_path: &str,
    file_size: i32,
    data: Option<&[u8]>,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO game_files (archive, file_path, file_size, data)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (archive, file_path) DO UPDATE SET file_size = $3, data = $4",
    )
    .bind(archive)
    .bind(file_path)
    .bind(file_size)
    .bind(data)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_all_game_files(
    pool: &PgPool,
    archive: &str,
) -> anyhow::Result<Vec<(String, i32)>> {
    let rows: Vec<(String, i32)> = sqlx::query_as(
        "SELECT file_path, file_size FROM game_files WHERE archive = $1 ORDER BY file_path",
    )
    .bind(archive)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_game_file_data(
    pool: &PgPool,
    archive: &str,
    file_path: &str,
) -> anyhow::Result<Option<Vec<u8>>> {
    let row: Option<(Option<Vec<u8>>,)> = sqlx::query_as(
        "SELECT data FROM game_files WHERE archive = $1 AND file_path = $2",
    )
    .bind(archive)
    .bind(file_path)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|r| r.0))
}

// --- Spells ---

pub async fn list_spells(
    pool: &PgPool,
    character_id: i64,
) -> anyhow::Result<Vec<Spell>> {
    let spells = sqlx::query_as::<_, Spell>(
        "SELECT * FROM spells WHERE character_id = $1 ORDER BY spell_id",
    )
    .bind(character_id)
    .fetch_all(pool)
    .await?;
    Ok(spells)
}

/// Breed starting spells — first 3 spells per breed (breed_id 1-18).
/// Source: Breeds D2O data, spells field.
const BREED_SPELLS: &[(i32, &[i32])] = &[
    (1,  &[3, 6, 17]),       // Feca
    (2,  &[21, 22, 23]),     // Osamodas
    (3,  &[41, 43, 44]),     // Enutrof
    (4,  &[61, 62, 65]),     // Sram
    (5,  &[81, 82, 83]),     // Xelor
    (6,  &[102, 103, 105]),  // Ecaflip
    (7,  &[121, 124, 125]),  // Eniripsa
    (8,  &[141, 142, 143]),  // Iop
    (9,  &[161, 162, 163]),  // Cra
    (10, &[183, 184, 189]),  // Sadida
    (11, &[432, 433, 434]),  // Sacrieur
    (12, &[686, 687, 688]),  // Pandawa
    (13, &[2763, 2766, 2769]), // Roublard
    (14, &[2872, 2876, 2878]), // Zobal
    (15, &[2980, 2981, 2982]), // Steamer
    (16, &[3499, 3500, 3501]), // Eliotrope
    (17, &[4627, 4628, 4629]), // Huppermage
    (18, &[5084, 5085, 5086]), // Ouginak
];

pub async fn insert_breed_spells(
    pool: &PgPool,
    character_id: i64,
    breed_id: i32,
) -> anyhow::Result<()> {
    let spells = BREED_SPELLS
        .iter()
        .find(|(id, _)| *id == breed_id)
        .map(|(_, s)| *s)
        .unwrap_or(&[]);

    for (pos, spell_id) in spells.iter().enumerate() {
        sqlx::query(
            "INSERT INTO spells (character_id, spell_id, level, position)
             VALUES ($1, $2, 1, $3)
             ON CONFLICT (character_id, spell_id) DO NOTHING",
        )
        .bind(character_id)
        .bind(*spell_id)
        .bind(pos as i32)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn update_character_position(
    pool: &PgPool,
    character_id: i64,
    map_id: i64,
    cell_id: i32,
    direction: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE characters SET map_id = $2, cell_id = $3, direction = $4 WHERE id = $1",
    )
    .bind(character_id)
    .bind(map_id)
    .bind(cell_id)
    .bind(direction)
    .execute(pool)
    .await?;
    Ok(())
}
