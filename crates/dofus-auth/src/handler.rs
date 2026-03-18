use crate::identification;
use crate::server_selection;
use crate::AuthState;
use dofus_common::error::identification_failure;
use dofus_protocol::messages::auth::*;
use dofus_network::session::Session;
use std::net::IpAddr;
use std::sync::Arc;

pub async fn handle_client(mut session: Session, state: Arc<AuthState>) -> anyhow::Result<()> {
    let peer = session.peer_addr()?;
    let ip: IpAddr = peer.ip();

    // --- Rate limiting ---
    if !state.check_rate_limit(ip) {
        tracing::warn!(%peer, "Rate limited");
        session.send(&IdentificationFailedMessage {
            reason: identification_failure::TOO_MANY_ON_IP,
        }).await?;
        return Ok(());
    }

    // --- Maintenance mode ---
    if state.is_maintenance() {
        tracing::info!(%peer, "Rejected: maintenance mode");
        session.send(&IdentificationFailedMessage {
            reason: identification_failure::IN_MAINTENANCE,
        }).await?;
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

    // Step 2: Send HelloConnectMessage
    let salt = uuid::Uuid::new_v4().to_string();
    session.send(&HelloConnectMessage {
        salt: salt.clone(),
        key: state.signed_session_key.clone(),
    }).await?;

    // Steps 3-6: Identification
    let (account_id, username) = match identification::handle_identification(
        &mut session, &state, &salt, peer, ip,
    ).await? {
        Some(result) => result,
        None => return Ok(()),
    };

    // Steps 7-9: Server selection & redirect
    server_selection::handle_server_selection(
        &mut session, &state, account_id, &username, peer,
    ).await
}
