use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub rsa_private_key_path: String,
    #[serde(default = "default_protocol_version")]
    pub protocol_version: String,
    #[serde(default = "default_ipc_port")]
    pub ipc_port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WorldConfig {
    pub host: String,
    pub port: u16,
    pub server_id: u16,
    pub server_name: String,
    pub database_url: String,
    #[serde(default = "default_protocol_version")]
    pub protocol_version: String,
    /// Path to the client content/maps/ directory containing maps*.d2p files.
    #[serde(default)]
    pub maps_dir: Option<String>,
    #[serde(default = "default_ipc_addr")]
    pub ipc_addr: String,
}

fn default_protocol_version() -> String {
    "1966".to_string()
}

fn default_ipc_port() -> u16 {
    9999
}

fn default_ipc_addr() -> String {
    "127.0.0.1:9999".to_string()
}

impl AuthConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str::<Self>(&content)?)
    }
}

impl WorldConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str::<Self>(&content)?)
    }
}
