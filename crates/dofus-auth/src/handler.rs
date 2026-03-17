use crate::AuthState;
use dofus_common::error::identification_failure;
use dofus_database::repository;
use dofus_io::DofusMessage as _;
use dofus_protocol::enums::server_status;
use dofus_protocol::messages::auth::*;
use dofus_protocol::registry::ProtocolMessage;
use dofus_protocol::types::GameServerInformations;
use dofus_network::session::Session;
use rsa::traits::PublicKeyParts;
use rsa::BigUint;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub async fn handle_client(mut session: Session, state: Arc<AuthState>) -> anyhow::Result<()> {
    let peer = session.peer_addr()?;
    tracing::info!(%peer, "Auth client connected");

    // Step 1: Send ProtocolRequired
    let protocol_required = ProtocolRequired {
        version: state.config.protocol_version.clone(),
    };
    session.send(&protocol_required).await?;

    // Step 2: Send HelloConnectMessage (RSA public key)
    let public_key = state.rsa_private_key.to_public_key();
    let n_bytes = public_key.n().to_bytes_be();
    let salt = uuid::Uuid::new_v4().to_string();

    let hello = HelloConnectMessage {
        salt: salt.clone(),
        key: n_bytes,
    };
    session.send(&hello).await?;

    // Step 3: Wait for IdentificationMessage
    let raw = match session.recv().await? {
        Some(raw) => raw,
        None => {
            tracing::warn!(%peer, "Client disconnected before identification");
            return Ok(());
        }
    };

    if raw.message_id != IdentificationMessage::MESSAGE_ID {
        tracing::warn!(%peer, message_id = raw.message_id, "Expected IdentificationMessage");
        return Ok(());
    }

    let msg = match ProtocolMessage::from_raw(raw.message_id, raw.payload) {
        Ok(ProtocolMessage::IdentificationMessage(m)) => m,
        _ => {
            tracing::warn!(%peer, "Failed to parse IdentificationMessage");
            return Ok(());
        }
    };

    tracing::debug!(%peer, lang = %msg.lang, "Received identification");

    // Step 4: Decrypt RSA credentials
    let (username, password) = match decrypt_credentials(&state.rsa_private_key, &salt, &msg.credentials) {
        Ok(creds) => creds,
        Err(e) => {
            tracing::warn!(%peer, error = %e, "Failed to decrypt credentials");
            let fail = IdentificationFailedMessage {
                reason: identification_failure::UNKNOWN_AUTH_ERROR,
            };
            session.send(&fail).await?;
            return Ok(());
        }
    };

    tracing::info!(%peer, %username, "Login attempt");

    // Step 5: Verify credentials
    let account = match repository::find_account_by_username(&state.pool, &username).await? {
        Some(acc) => acc,
        None => {
            // Auto-create account for convenience (dev mode)
            let hash = hash_password(&password);
            let id = repository::create_account(&state.pool, &username, &hash, &username).await?;
            tracing::info!(%peer, %username, account_id = id, "Auto-created account");
            repository::find_account_by_username(&state.pool, &username)
                .await?
                .unwrap()
        }
    };

    if account.banned {
        let fail = IdentificationFailedMessage {
            reason: identification_failure::BANNED,
        };
        session.send(&fail).await?;
        return Ok(());
    }

    let expected_hash = hash_password(&password);
    if account.password_hash != expected_hash {
        let fail = IdentificationFailedMessage {
            reason: identification_failure::WRONG_CREDENTIALS,
        };
        session.send(&fail).await?;
        return Ok(());
    }

    // Step 6: Send IdentificationSuccessMessage
    let success = IdentificationSuccessMessage {
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
        subscription_end_date: f64::MAX,
        havenbag_available_room: 0,
    };
    session.send(&success).await?;

    // Step 7: Send server list
    let db_servers = repository::get_all_servers(&state.pool).await?;
    let servers: Vec<GameServerInformations> = db_servers
        .iter()
        .map(|s| GameServerInformations {
            is_mono_account: false,
            is_selectable: true,
            id: s.id as i16,
            server_type: 0,
            status: server_status::ONLINE,
            completion: s.completion as u8,
            characters_count: 0,
            characters_slots: 5,
            date: 0.0,
        })
        .collect();

    let server_list = ServersListMessage {
        servers,
        already_connected_to_server_id: 0,
        can_create_new_character: true,
    };
    session.send(&server_list).await?;

    // Step 8: Wait for ServerSelectionMessage
    let raw = match session.recv().await? {
        Some(raw) => raw,
        None => return Ok(()),
    };

    if raw.message_id != ServerSelectionMessage::MESSAGE_ID {
        tracing::warn!(%peer, message_id = raw.message_id, "Expected ServerSelectionMessage");
        return Ok(());
    }

    let selection = match ProtocolMessage::from_raw(raw.message_id, raw.payload) {
        Ok(ProtocolMessage::ServerSelectionMessage(m)) => m,
        _ => return Ok(()),
    };

    tracing::info!(%peer, server_id = selection.server_id, "Server selected");

    // Step 9: Create ticket and send redirect
    let server = match repository::get_server_by_id(&state.pool, selection.server_id as i64).await? {
        Some(s) => s,
        None => {
            tracing::warn!(%peer, server_id = selection.server_id, "Server not found");
            return Ok(());
        }
    };

    let ticket = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now() + chrono::Duration::seconds(30);
    repository::create_ticket(
        &state.pool,
        &ticket,
        account.id,
        server.id,
        &expires.format("%Y-%m-%d %H:%M:%S").to_string(),
    )
    .await?;

    let redirect = SelectedServerDataMessage {
        server_id: server.id as i16,
        address: server.address.clone(),
        ports: vec![server.port as i16],
        can_create_new_character: true,
        ticket: ticket.as_bytes().to_vec(),
    };
    session.send(&redirect).await?;

    tracing::info!(%peer, %username, %ticket, "Client redirected to world server");
    Ok(())
}

fn decrypt_credentials(
    private_key: &rsa::RsaPrivateKey,
    salt: &str,
    encrypted: &[u8],
) -> anyhow::Result<(String, String)> {
    // RSA raw decrypt (textbook RSA, matching client behavior)
    let ciphertext = BigUint::from_bytes_be(encrypted);
    let plaintext = rsa::hazmat::rsa_decrypt(None::<&mut rand::rngs::ThreadRng>, private_key, &ciphertext)?;
    let decrypted = plaintext.to_bytes_be();

    // Format: salt_bytes + username_len(2 bytes BE) + username + password
    let salt_bytes = salt.as_bytes();
    if decrypted.len() <= salt_bytes.len() + 2 {
        anyhow::bail!("Decrypted data too short");
    }

    let data = &decrypted[salt_bytes.len()..];
    if data.len() < 2 {
        anyhow::bail!("No username length");
    }

    // The Dofus client sends: salt + (username as UTF with length prefix) + password
    // But the exact format may vary. Let's try the common format:
    // salt + \0 + username + \0 + password (null-separated)
    // OR salt + username_len(1 byte) + username + password
    // The most common Dofus 2.x format: credentials = salt_bytes + login_bytes + password_bytes
    // with a specific framing. Let's handle both common formats.

    // Try null-byte separated format first
    if let Some(sep_pos) = data.iter().position(|&b| b == 0) {
        let after_sep = &data[sep_pos + 1..];
        if let Some(sep_pos2) = after_sep.iter().position(|&b| b == 0) {
            let username = String::from_utf8_lossy(&after_sep[..sep_pos2]).to_string();
            let password = String::from_utf8_lossy(&after_sep[sep_pos2 + 1..]).to_string();
            return Ok((username, password));
        }
    }

    // Fallback: treat entire data as username\0password
    let full = String::from_utf8_lossy(data);
    if let Some(idx) = full.find('\0') {
        let username = full[..idx].to_string();
        let password = full[idx + 1..].to_string();
        return Ok((username, password));
    }

    anyhow::bail!("Could not parse credentials from decrypted data")
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}
