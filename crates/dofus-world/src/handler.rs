use crate::WorldState;
use dofus_database::repository;
use dofus_io::DofusMessage as _;
use dofus_protocol::messages::game::*;
use dofus_protocol::registry::ProtocolMessage;
use dofus_network::session::Session;
use std::sync::Arc;

pub async fn handle_client(mut session: Session, state: Arc<WorldState>) -> anyhow::Result<()> {
    let peer = session.peer_addr()?;
    tracing::info!(%peer, "World client connected");

    // Step 1: Wait for AuthenticationTicketMessage
    let raw = match session.recv().await? {
        Some(raw) => raw,
        None => {
            tracing::warn!(%peer, "Client disconnected before ticket");
            return Ok(());
        }
    };

    if raw.message_id != AuthenticationTicketMessage::MESSAGE_ID {
        tracing::warn!(%peer, message_id = raw.message_id, "Expected AuthenticationTicketMessage");
        return Ok(());
    }

    let ticket_msg = match ProtocolMessage::from_raw(raw.message_id, raw.payload) {
        Ok(ProtocolMessage::AuthenticationTicketMessage(m)) => m,
        _ => {
            tracing::warn!(%peer, "Failed to parse AuthenticationTicketMessage");
            return Ok(());
        }
    };

    tracing::debug!(%peer, lang = %ticket_msg.lang, "Received ticket");

    // Step 2: Validate ticket
    let ticket = match repository::consume_ticket(&state.pool, &ticket_msg.ticket).await? {
        Some(t) => t,
        None => {
            tracing::warn!(%peer, "Invalid or expired ticket");
            return Ok(());
        }
    };

    tracing::info!(%peer, account_id = ticket.account_id, "Ticket validated");

    // Step 3: Main game loop (placeholder — just handle pings)
    loop {
        let raw = match session.recv().await? {
            Some(raw) => raw,
            None => break,
        };

        match ProtocolMessage::from_raw(raw.message_id, raw.payload.clone()) {
            Ok(ProtocolMessage::BasicPingMessage(ping)) => {
                let pong = BasicPongMessage { quiet: ping.quiet };
                session.send(&pong).await?;
            }
            Ok(msg) => {
                tracing::debug!(%peer, message = %msg, "Unhandled message");
            }
            Err(e) => {
                tracing::warn!(%peer, message_id = raw.message_id, error = %e, "Failed to parse message");
            }
        }
    }

    tracing::info!(%peer, "Client disconnected");
    Ok(())
}
