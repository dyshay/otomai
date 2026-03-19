pub mod models;
pub mod repository;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub async fn create_pool(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    // Run each table creation separately (Postgres doesn't support multi-statement in one query)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
            id BIGSERIAL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            nickname TEXT NOT NULL,
            admin_level INT NOT NULL DEFAULT 0,
            banned BOOLEAN NOT NULL DEFAULT FALSE,
            last_login TIMESTAMPTZ,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS servers (
            id BIGINT PRIMARY KEY,
            name TEXT NOT NULL,
            address TEXT NOT NULL,
            port INT NOT NULL,
            status INT NOT NULL DEFAULT 1,
            completion INT NOT NULL DEFAULT 0,
            community INT NOT NULL DEFAULT 0
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tickets (
            ticket TEXT PRIMARY KEY,
            account_id BIGINT NOT NULL REFERENCES accounts(id),
            server_id BIGINT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            expires_at TIMESTAMPTZ NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS characters (
            id BIGSERIAL PRIMARY KEY,
            account_id BIGINT NOT NULL REFERENCES accounts(id),
            name TEXT NOT NULL UNIQUE,
            breed_id INT NOT NULL,
            sex INT NOT NULL DEFAULT 0,
            level INT NOT NULL DEFAULT 1,
            experience BIGINT NOT NULL DEFAULT 0,
            kamas BIGINT NOT NULL DEFAULT 0,
            map_id BIGINT NOT NULL DEFAULT 0,
            cell_id INT NOT NULL DEFAULT 0,
            direction INT NOT NULL DEFAULT 1,
            colors JSONB NOT NULL DEFAULT '[]',
            stats JSONB NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            last_login TIMESTAMPTZ
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS inventory_items (
            id BIGSERIAL PRIMARY KEY,
            character_id BIGINT NOT NULL REFERENCES characters(id),
            item_template_id INT NOT NULL,
            quantity INT NOT NULL DEFAULT 1,
            position INT NOT NULL DEFAULT 63,
            effects JSONB NOT NULL DEFAULT '[]'
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS spells (
            character_id BIGINT NOT NULL REFERENCES characters(id),
            spell_id INT NOT NULL,
            level INT NOT NULL DEFAULT 1,
            position INT NOT NULL DEFAULT 63,
            PRIMARY KEY (character_id, spell_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // --- Social tables ---

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS friends (
            account_id BIGINT NOT NULL REFERENCES accounts(id),
            friend_account_id BIGINT NOT NULL REFERENCES accounts(id),
            PRIMARY KEY (account_id, friend_account_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ignored (
            account_id BIGINT NOT NULL REFERENCES accounts(id),
            ignored_account_id BIGINT NOT NULL REFERENCES accounts(id),
            PRIMARY KEY (account_id, ignored_account_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // --- NPC + Quest tables ---

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS npc_spawns (
            id SERIAL PRIMARY KEY,
            npc_id INT NOT NULL,
            map_id BIGINT NOT NULL,
            cell_id INT NOT NULL,
            direction INT NOT NULL DEFAULT 3,
            look TEXT NOT NULL DEFAULT ''
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS character_quests (
            character_id BIGINT NOT NULL REFERENCES characters(id),
            quest_id INT NOT NULL,
            step_id INT NOT NULL,
            status INT NOT NULL DEFAULT 0,
            objectives JSONB NOT NULL DEFAULT '[]',
            started_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
            completed_at TIMESTAMPTZ,
            PRIMARY KEY (character_id, quest_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // --- Game data tables ---

    // D2O objects (Items, Spells, Monsters, etc.)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS game_data (
            file_name TEXT NOT NULL,
            object_id INT NOT NULL,
            class_name TEXT NOT NULL,
            data JSONB NOT NULL,
            PRIMARY KEY (file_name, object_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // D2I translations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS game_texts (
            file_name TEXT NOT NULL,
            text_id INT NOT NULL,
            text TEXT NOT NULL,
            undiacritical TEXT,
            PRIMARY KEY (file_name, text_id)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // D2I named texts (UI strings)
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS game_named_texts (
            file_name TEXT NOT NULL,
            text_key TEXT NOT NULL,
            text TEXT NOT NULL,
            PRIMARY KEY (file_name, text_key)
        )
        "#,
    )
    .execute(pool)
    .await?;

    // D2P archive index
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS game_files (
            archive TEXT NOT NULL,
            file_path TEXT NOT NULL,
            file_size INT NOT NULL,
            data BYTEA,
            PRIMARY KEY (archive, file_path)
        )
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
