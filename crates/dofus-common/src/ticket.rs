use crate::id::{AccountId, ServerId};
use chrono::{DateTime, Utc};

/// Auth ticket passed from auth server to world server via shared DB.
#[derive(Debug, Clone)]
pub struct AuthTicket {
    pub ticket: String,
    pub account_id: AccountId,
    pub server_id: ServerId,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
