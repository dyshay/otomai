use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AuthConfig {
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub rsa_private_key_path: String,
    pub protocol_version: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WorldConfig {
    pub host: String,
    pub port: u16,
    pub server_id: u16,
    pub server_name: String,
    pub database_url: String,
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
