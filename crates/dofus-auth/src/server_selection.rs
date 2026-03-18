use crate::crypto::encrypt_ticket_aes;
use crate::AuthState;
use dofus_database::repository;
use dofus_io::DofusMessage as _;
use dofus_protocol::enums::server_status;
use dofus_protocol::messages::auth::*;
use dofus_protocol::registry::ProtocolMessage;
use dofus_protocol::types::GameServerInformations;
use dofus_network::session::Session;
use std::sync::Arc;

/// Steps 7-9: Send server list, wait for selection, create ticket, redirect.
pub async fn handle_server_selection(
    session: &mut Session,
    state: &Arc<AuthState>,
    account_id: i64,
    username: &str,
    aes_key: &[u8],
    peer: std::net::SocketAddr,
) -> anyhow::Result<()> {
    // Step 7: Send server list
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
                characters_count: 0,
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

    // Step 8: Wait for ServerSelectionMessage (skip intermediate messages like ClientKeyMessage)
    let raw = loop {
        match session.recv().await? {
            Some(raw) if raw.message_id == ServerSelectionMessage::MESSAGE_ID => break raw,
            Some(raw) => {
                tracing::debug!(%peer, message_id = raw.message_id, "Skipping intermediate message");
                continue;
            }
            None => return Ok(()),
        }
    };

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
        account_id,
        server.id,
        &expires.to_rfc3339(),
    )
    .await?;

    let encrypted_ticket = encrypt_ticket_aes(aes_key, &ticket)?;
    session.send(&SelectedServerDataMessage {
        server_id: server.id as i16,
        address: server.address.clone(),
        ports: vec![server.port as i16],
        can_create_new_character: true,
        ticket: encrypted_ticket,
    }).await?;

    tracing::info!(%peer, %username, %ticket, "Client redirected to {}", server.name);
    Ok(())
}
