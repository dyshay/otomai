//! Inter-Process Communication between Auth and World servers.
//!
//! Simple TCP protocol: length-prefixed JSON messages.
//! Auth runs the IPC server, World connects as client.

pub mod messages;
pub mod client;
pub mod server;

use serde::{Deserialize, Serialize};

/// IPC message envelope: type tag + payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEnvelope {
    pub msg_type: String,
    pub payload: serde_json::Value,
}
