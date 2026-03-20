//! IPC message types exchanged between Auth and World.

use serde::{Deserialize, Serialize};

/// World → Auth: register this world server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub server_id: i16,
    pub server_name: String,
}

/// World → Auth: update server status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerStatusUpdate {
    pub server_id: i16,
    pub player_count: u32,
    pub status: i32, // 1=offline, 2=saving, 3=online, 4=full
}

/// Auth → World: disconnect a client by account_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectClientRequest {
    pub account_id: i64,
}

/// World → Auth: disconnect result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectClientResult {
    pub account_id: i64,
    pub success: bool,
}

/// Auth → World: check if an IP is connected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsIpConnectedRequest {
    pub ip: String,
}

/// World → Auth: IP connection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsIpConnectedResult {
    pub ip: String,
    pub connected: bool,
}

/// Auth → World: account data for a ticket.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountData {
    pub account_id: i64,
    pub username: String,
    pub nickname: String,
    pub admin_level: i32,
    pub ticket: String,
}
