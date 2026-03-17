use crate::models::{Account, Server, Ticket};
use sqlx::SqlitePool;

pub async fn find_account_by_username(
    pool: &SqlitePool,
    username: &str,
) -> anyhow::Result<Option<Account>> {
    let account = sqlx::query_as::<_, Account>("SELECT * FROM accounts WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;
    Ok(account)
}

pub async fn get_all_servers(pool: &SqlitePool) -> anyhow::Result<Vec<Server>> {
    let servers = sqlx::query_as::<_, Server>("SELECT * FROM servers")
        .fetch_all(pool)
        .await?;
    Ok(servers)
}

pub async fn get_server_by_id(
    pool: &SqlitePool,
    server_id: i64,
) -> anyhow::Result<Option<Server>> {
    let server = sqlx::query_as::<_, Server>("SELECT * FROM servers WHERE id = ?")
        .bind(server_id)
        .fetch_optional(pool)
        .await?;
    Ok(server)
}

pub async fn create_ticket(
    pool: &SqlitePool,
    ticket: &str,
    account_id: i64,
    server_id: i64,
    expires_at: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO tickets (ticket, account_id, server_id, expires_at) VALUES (?, ?, ?, ?)")
        .bind(ticket)
        .bind(account_id)
        .bind(server_id)
        .bind(expires_at)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn consume_ticket(
    pool: &SqlitePool,
    ticket: &str,
) -> anyhow::Result<Option<Ticket>> {
    let row = sqlx::query_as::<_, Ticket>(
        "SELECT * FROM tickets WHERE ticket = ? AND expires_at > datetime('now')",
    )
    .bind(ticket)
    .fetch_optional(pool)
    .await?;

    if row.is_some() {
        sqlx::query("DELETE FROM tickets WHERE ticket = ?")
            .bind(ticket)
            .execute(pool)
            .await?;
    }

    Ok(row)
}

pub async fn create_account(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
    nickname: &str,
) -> anyhow::Result<i64> {
    let result = sqlx::query(
        "INSERT INTO accounts (username, password_hash, nickname) VALUES (?, ?, ?)",
    )
    .bind(username)
    .bind(password_hash)
    .bind(nickname)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn insert_server(
    pool: &SqlitePool,
    id: i64,
    name: &str,
    address: &str,
    port: i32,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO servers (id, name, address, port, status, completion, community) VALUES (?, ?, ?, ?, 2, 0, 0)",
    )
    .bind(id)
    .bind(name)
    .bind(address)
    .bind(port)
    .execute(pool)
    .await?;
    Ok(())
}
