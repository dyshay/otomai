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

    // Add a table for game data assets (D2O import cache)
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

    Ok(())
}
