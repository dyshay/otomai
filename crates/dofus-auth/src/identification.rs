use crate::crypto::{decrypt_credentials, hash_password};
use crate::AuthState;
use dofus_common::error::identification_failure;
use dofus_database::repository;
use dofus_io::DofusMessage as _;
use dofus_protocol::messages::auth::*;
use dofus_protocol::registry::ProtocolMessage;
use dofus_network::session::Session;
use std::net::IpAddr;
use std::sync::Arc;

/// Steps 3-6: Receive IdentificationMessage, decrypt credentials, verify, respond.
/// Returns (account_id, username) on success, or sends failure and returns None.
pub async fn handle_identification(
    session: &mut Session,
    state: &Arc<AuthState>,
    salt: &str,
    peer: std::net::SocketAddr,
    ip: IpAddr,
) -> anyhow::Result<Option<(i64, String)>> {
    // Step 3: Wait for IdentificationMessage
    let raw = match session.recv().await? {
        Some(raw) => raw,
        None => {
            tracing::warn!(%peer, "Client disconnected before identification");
            return Ok(None);
        }
    };

    if raw.message_id != IdentificationMessage::MESSAGE_ID {
        tracing::warn!(%peer, message_id = raw.message_id, "Expected IdentificationMessage");
        return Ok(None);
    }

    let msg = match ProtocolMessage::from_raw(raw.message_id, raw.payload) {
        Ok(ProtocolMessage::IdentificationMessage(m)) => m,
        _ => {
            tracing::warn!(%peer, "Failed to parse IdentificationMessage");
            return Ok(None);
        }
    };

    tracing::debug!(%peer, lang = %msg.lang, "Received identification");

    // Step 4: Decrypt RSA credentials
    let (username, password) = match decrypt_credentials(
        &state.session_private_key,
        salt,
        &msg.credentials,
        msg.use_certificate,
    ) {
        Ok(creds) => creds,
        Err(e) => {
            tracing::warn!(%peer, error = %e, "Failed to decrypt credentials");
            state.record_failed_attempt(ip);
            session.send(&IdentificationFailedMessage {
                reason: identification_failure::UNKNOWN_AUTH_ERROR,
            }).await?;
            return Ok(None);
        }
    };

    tracing::info!(%peer, %username, "Login attempt");

    // Step 5: Verify credentials
    let account = match repository::find_account_by_username(&state.pool, &username).await? {
        Some(acc) => acc,
        None => {
            if state.auto_create_accounts {
                let hash = hash_password(&password);
                let id = repository::create_account(&state.pool, &username, &hash, &username).await?;
                tracing::info!(%peer, %username, account_id = id, "Auto-created account");
                repository::find_account_by_username(&state.pool, &username)
                    .await?
                    .unwrap()
            } else {
                tracing::warn!(%peer, %username, "Account not found");
                state.record_failed_attempt(ip);
                session.send(&IdentificationFailedMessage {
                    reason: identification_failure::WRONG_CREDENTIALS,
                }).await?;
                return Ok(None);
            }
        }
    };

    if account.banned {
        session.send(&IdentificationFailedMessage {
            reason: identification_failure::BANNED,
        }).await?;
        return Ok(None);
    }

    let expected_hash = hash_password(&password);
    if account.password_hash != expected_hash {
        tracing::warn!(%peer, %username, "Wrong password");
        state.record_failed_attempt(ip);
        session.send(&IdentificationFailedMessage {
            reason: identification_failure::WRONG_CREDENTIALS,
        }).await?;
        return Ok(None);
    }

    // Step 6: Send IdentificationSuccessMessage
    session.send(&IdentificationSuccessMessage {
        has_rights: account.admin_level > 0,
        has_console_right: account.admin_level > 0,
        was_already_connected: false,
        login: account.username.clone(),
        nickname: account.nickname.clone(),
        account_id: account.id as i32,
        community_id: 0,
        secret_question: String::new(),
        account_creation: 0.0,
        subscription_elapsed_duration: 0.0,
        subscription_end_date: 0.0,
        havenbag_available_room: 0,
    }).await?;

    Ok(Some((account.id, username)))
}
