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
use std::net::IpAddr;
use std::sync::Arc;

const SALT_MIN_LENGTH: usize = 32;

pub async fn handle_client(mut session: Session, state: Arc<AuthState>) -> anyhow::Result<()> {
    let peer = session.peer_addr()?;
    let ip: IpAddr = peer.ip();

    // --- Rate limiting ---
    if !state.check_rate_limit(ip) {
        tracing::warn!(%peer, "Rate limited");
        let fail = IdentificationFailedMessage {
            reason: identification_failure::TOO_MANY_ON_IP,
        };
        session.send(&fail).await?;
        return Ok(());
    }

    // --- Maintenance mode ---
    if state.is_maintenance() {
        tracing::info!(%peer, "Rejected: maintenance mode");
        let fail = IdentificationFailedMessage {
            reason: identification_failure::IN_MAINTENANCE,
        };
        session.send(&fail).await?;
        return Ok(());
    }

    // --- Connection queue ---
    let _permit = state.connection_semaphore.acquire().await?;
    tracing::info!(%peer, "Auth client connected");

    // Step 1: Send ProtocolRequired
    let proto_version: i32 = state.config.protocol_version.parse().unwrap_or(0);
    session.send(&ProtocolRequired {
        required_version: proto_version,
        current_version: proto_version,
    }).await?;

    // Step 2: Send HelloConnectMessage (RSA public key + salt)
    let public_key = state.rsa_private_key.to_public_key();
    let n_bytes = public_key.n().to_bytes_be();
    let salt = uuid::Uuid::new_v4().to_string();

    session.send(&HelloConnectMessage {
        salt: salt.clone(),
        key: n_bytes,
    }).await?;

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
    // Format (AS3 cipherRsa): salt(32 padded) + AES_key(32) + [cert_id(4)+cert_hash if cert]
    //                        + username_len(1) + username + password
    let (username, password) = match decrypt_credentials(
        &state.rsa_private_key,
        &salt,
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
            return Ok(());
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
                return Ok(());
            }
        }
    };

    if account.banned {
        session.send(&IdentificationFailedMessage {
            reason: identification_failure::BANNED,
        }).await?;
        return Ok(());
    }

    let expected_hash = hash_password(&password);
    if account.password_hash != expected_hash {
        tracing::warn!(%peer, %username, "Wrong password");
        state.record_failed_attempt(ip);
        session.send(&IdentificationFailedMessage {
            reason: identification_failure::WRONG_CREDENTIALS,
        }).await?;
        return Ok(());
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
        subscription_end_date: f64::MAX,
        havenbag_available_room: 0,
    }).await?;

    // Step 7: Send server list (multi-server)
    let db_servers = repository::get_all_servers(&state.pool).await?;
    let servers: Vec<GameServerInformations> = db_servers
        .iter()
        .map(|s| {
            let status = if state.is_maintenance() {
                server_status::NOJOIN as u8
            } else {
                s.status as u8
            };
            GameServerInformations {
                is_mono_account: false,
                is_selectable: status == server_status::ONLINE as u8,
                id: s.id as i16,
                r#type: 0,
                status,
                completion: s.completion as u8,
                characters_count: 0, // TODO: query per account
                characters_slots: 5,
                date: 0.0,
            }
        })
        .collect();

    session.send(&ServersListMessage {
        servers,
        already_connected_to_server_id: 0,
        can_create_new_character: true,
    }).await?;

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

    // Step 9: Validate server & create ticket
    let server = match repository::get_server_by_id(&state.pool, selection.server_id as i64).await? {
        Some(s) => s,
        None => {
            tracing::warn!(%peer, server_id = selection.server_id, "Server not found");
            return Ok(());
        }
    };

    if (server.status as u8) != server_status::ONLINE as u8 {
        tracing::warn!(%peer, server_id = selection.server_id, "Server not online");
        return Ok(());
    }

    let ticket = uuid::Uuid::new_v4().to_string();
    let expires = chrono::Utc::now() + chrono::Duration::seconds(30);
    repository::create_ticket(
        &state.pool,
        &ticket,
        account.id,
        server.id,
        &expires.to_rfc3339(),
    )
    .await?;

    session.send(&SelectedServerDataMessage {
        server_id: server.id as i16,
        address: server.address.clone(),
        ports: vec![server.port as i16],
        can_create_new_character: true,
        ticket: ticket.as_bytes().to_vec(),
    }).await?;

    tracing::info!(%peer, %username, %ticket, "Client redirected to {}", server.name);
    Ok(())
}

/// Decrypt RSA-encrypted credentials from the Dofus client.
///
/// AS3 format (AuthentificationManager.cipherRsa):
///   salt_bytes (padded to 32 with spaces)
///   + AES_key (32 bytes)
///   + [if certificate: cert_id (u32 BE) + cert_hash (UTF-8 bytes)]
///   + username_len (1 byte)
///   + username (UTF-8 bytes)
///   + password (UTF-8 bytes, remaining data)
fn decrypt_credentials(
    private_key: &rsa::RsaPrivateKey,
    salt: &str,
    encrypted: &[u8],
    has_certificate: bool,
) -> anyhow::Result<(String, String)> {
    // RSA raw/textbook decrypt (matching Dofus client RSA.publicEncrypt)
    let ciphertext = BigUint::from_bytes_be(encrypted);
    let plaintext = rsa::hazmat::rsa_decrypt(None::<&mut rand::rngs::ThreadRng>, private_key, &ciphertext)?;
    let decrypted = plaintext.to_bytes_be();

    // Salt is padded to minimum 32 bytes with spaces (AS3: setSalt)
    let salt_len = salt.len().max(SALT_MIN_LENGTH);

    let mut offset = salt_len;

    // AES key: 32 bytes
    const AES_KEY_LENGTH: usize = 32;
    if decrypted.len() < offset + AES_KEY_LENGTH {
        anyhow::bail!("Decrypted data too short for AES key (len={}, need={})", decrypted.len(), offset + AES_KEY_LENGTH);
    }
    // Skip AES key (used for subsequent AES communication, not needed for auth)
    offset += AES_KEY_LENGTH;

    // Certificate (optional)
    if has_certificate {
        if decrypted.len() < offset + 4 {
            anyhow::bail!("Decrypted data too short for certificate");
        }
        let _cert_id = u32::from_be_bytes([
            decrypted[offset], decrypted[offset + 1],
            decrypted[offset + 2], decrypted[offset + 3],
        ]);
        offset += 4;
        // cert_hash is a variable-length UTF string without length prefix
        // We can't know the length without a delimiter, so skip cert handling for now
        // In practice, most connections don't use certificates
        tracing::debug!("Certificate present, cert_id={}", _cert_id);
    }

    // Username length (1 byte)
    if decrypted.len() < offset + 1 {
        anyhow::bail!("Decrypted data too short for username length");
    }
    let username_len = decrypted[offset] as usize;
    offset += 1;

    // Username
    if decrypted.len() < offset + username_len {
        anyhow::bail!("Decrypted data too short for username (need={}, have={})",
            offset + username_len, decrypted.len());
    }
    let username = String::from_utf8_lossy(&decrypted[offset..offset + username_len]).to_string();
    offset += username_len;

    // Password (remaining bytes)
    let password = String::from_utf8_lossy(&decrypted[offset..]).to_string();

    if username.is_empty() {
        anyhow::bail!("Empty username");
    }

    Ok((username, password))
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Parse decrypted credential bytes (after RSA decryption).
/// Exposed for testing.
pub(crate) fn parse_credential_bytes(
    decrypted: &[u8],
    salt: &str,
    has_certificate: bool,
) -> anyhow::Result<(String, String)> {
    let salt_len = salt.len().max(SALT_MIN_LENGTH);
    let mut offset = salt_len;

    const AES_KEY_LENGTH: usize = 32;
    if decrypted.len() < offset + AES_KEY_LENGTH {
        anyhow::bail!("Decrypted data too short for AES key");
    }
    offset += AES_KEY_LENGTH;

    if has_certificate {
        if decrypted.len() < offset + 4 {
            anyhow::bail!("Decrypted data too short for certificate");
        }
        offset += 4;
        // Skip cert hash — variable length, not parseable without delimiter
    }

    if decrypted.len() < offset + 1 {
        anyhow::bail!("Decrypted data too short for username length");
    }
    let username_len = decrypted[offset] as usize;
    offset += 1;

    if decrypted.len() < offset + username_len {
        anyhow::bail!("Decrypted data too short for username");
    }
    let username = String::from_utf8_lossy(&decrypted[offset..offset + username_len]).to_string();
    offset += username_len;

    let password = String::from_utf8_lossy(&decrypted[offset..]).to_string();

    if username.is_empty() {
        anyhow::bail!("Empty username");
    }

    Ok((username, password))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a fake decrypted credential buffer matching the AS3 cipherRsa format.
    fn build_credentials(salt: &str, username: &str, password: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        // Salt padded to 32
        let mut padded_salt = salt.to_string();
        while padded_salt.len() < SALT_MIN_LENGTH {
            padded_salt.push(' ');
        }
        buf.extend_from_slice(padded_salt.as_bytes());
        // AES key (32 random bytes)
        buf.extend_from_slice(&[0xAA; 32]);
        // Username length (1 byte) + username + password
        buf.push(username.len() as u8);
        buf.extend_from_slice(username.as_bytes());
        buf.extend_from_slice(password.as_bytes());
        buf
    }

    fn build_credentials_with_cert(
        salt: &str,
        username: &str,
        password: &str,
        cert_id: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut padded_salt = salt.to_string();
        while padded_salt.len() < SALT_MIN_LENGTH {
            padded_salt.push(' ');
        }
        buf.extend_from_slice(padded_salt.as_bytes());
        buf.extend_from_slice(&[0xBB; 32]); // AES key
        buf.extend_from_slice(&cert_id.to_be_bytes()); // cert_id
        // No cert_hash for simplicity (test without cert parsing)
        buf.push(username.len() as u8);
        buf.extend_from_slice(username.as_bytes());
        buf.extend_from_slice(password.as_bytes());
        buf
    }

    #[test]
    fn parse_credentials_basic() {
        let salt = "a1b2c3d4-e5f6-7890-abcd-ef1234567890";
        let data = build_credentials(salt, "testuser", "mypassword");
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "testuser");
        assert_eq!(pass, "mypassword");
    }

    #[test]
    fn parse_credentials_short_salt() {
        let salt = "short"; // will be padded to 32
        let data = build_credentials(salt, "admin", "secret123");
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "admin");
        assert_eq!(pass, "secret123");
    }

    #[test]
    fn parse_credentials_long_salt() {
        let salt = "this-is-a-very-long-salt-string-that-exceeds-32-characters-easily";
        let mut buf = Vec::new();
        buf.extend_from_slice(salt.as_bytes()); // salt is longer than 32, used as-is
        buf.extend_from_slice(&[0xCC; 32]); // AES key
        buf.push(4); // username len
        buf.extend_from_slice(b"user");
        buf.extend_from_slice(b"pass");
        let (user, pass) = parse_credential_bytes(&buf, salt, false).unwrap();
        assert_eq!(user, "user");
        assert_eq!(pass, "pass");
    }

    #[test]
    fn parse_credentials_unicode() {
        let salt = "12345678901234567890123456789012";
        let username = "joueur";
        let password = "motdepasse123";
        let data = build_credentials(salt, username, password);
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "joueur");
        assert_eq!(pass, "motdepasse123");
    }

    #[test]
    fn parse_credentials_empty_password() {
        let salt = "12345678901234567890123456789012";
        let data = build_credentials(salt, "user", "");
        let (user, pass) = parse_credential_bytes(&data, salt, false).unwrap();
        assert_eq!(user, "user");
        assert_eq!(pass, "");
    }

    #[test]
    fn parse_credentials_empty_username_fails() {
        let salt = "12345678901234567890123456789012";
        let data = build_credentials(salt, "", "pass");
        let result = parse_credential_bytes(&data, salt, false);
        assert!(result.is_err());
    }

    #[test]
    fn parse_credentials_too_short_fails() {
        let salt = "12345678901234567890123456789012";
        let data = vec![0u8; 10]; // way too short
        let result = parse_credential_bytes(&data, salt, false);
        assert!(result.is_err());
    }

    #[test]
    fn parse_credentials_with_certificate() {
        let salt = "12345678901234567890123456789012";
        let data = build_credentials_with_cert(salt, "certuser", "certpass", 12345);
        let (user, pass) = parse_credential_bytes(&data, salt, true).unwrap();
        assert_eq!(user, "certuser");
        assert_eq!(pass, "certpass");
    }

    #[test]
    fn hash_password_deterministic() {
        let h1 = hash_password("test123");
        let h2 = hash_password("test123");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex
    }

    #[test]
    fn hash_password_different_inputs() {
        let h1 = hash_password("password1");
        let h2 = hash_password("password2");
        assert_ne!(h1, h2);
    }
}
