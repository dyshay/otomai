pub mod models;
pub mod repository;

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

pub async fn create_pool(database_url: &str) -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            nickname TEXT NOT NULL,
            admin_level INTEGER NOT NULL DEFAULT 0,
            banned INTEGER NOT NULL DEFAULT 0,
            last_login TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS servers (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            address TEXT NOT NULL,
            port INTEGER NOT NULL,
            status INTEGER NOT NULL DEFAULT 1,
            completion INTEGER NOT NULL DEFAULT 0,
            community INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS tickets (
            ticket TEXT PRIMARY KEY,
            account_id INTEGER NOT NULL,
            server_id INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            expires_at TEXT NOT NULL,
            FOREIGN KEY (account_id) REFERENCES accounts(id)
        );

        CREATE TABLE IF NOT EXISTS characters (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            account_id INTEGER NOT NULL,
            name TEXT NOT NULL UNIQUE,
            breed_id INTEGER NOT NULL,
            sex INTEGER NOT NULL DEFAULT 0,
            level INTEGER NOT NULL DEFAULT 1,
            experience INTEGER NOT NULL DEFAULT 0,
            kamas INTEGER NOT NULL DEFAULT 0,
            map_id INTEGER NOT NULL DEFAULT 0,
            cell_id INTEGER NOT NULL DEFAULT 0,
            direction INTEGER NOT NULL DEFAULT 1,
            colors TEXT NOT NULL DEFAULT '[]',
            stats TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_login TEXT,
            FOREIGN KEY (account_id) REFERENCES accounts(id)
        );

        CREATE TABLE IF NOT EXISTS inventory_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            character_id INTEGER NOT NULL,
            item_template_id INTEGER NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 1,
            position INTEGER NOT NULL DEFAULT 63,
            effects TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY (character_id) REFERENCES characters(id)
        );

        CREATE TABLE IF NOT EXISTS spells (
            character_id INTEGER NOT NULL,
            spell_id INTEGER NOT NULL,
            level INTEGER NOT NULL DEFAULT 1,
            position INTEGER NOT NULL DEFAULT 63,
            PRIMARY KEY (character_id, spell_id),
            FOREIGN KEY (character_id) REFERENCES characters(id)
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
