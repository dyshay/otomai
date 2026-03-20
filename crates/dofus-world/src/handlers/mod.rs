mod approach;
mod combat;
mod roleplay;
pub mod session;

use crate::{ticket, WorldState};
use dofus_database::repository;
use dofus_io::DofusMessage as _;
use dofus_network::session::Session;
use dofus_protocol::messages::auth::ProtocolRequired;
use dofus_protocol::messages::game::*;
use dofus_protocol::registry::ProtocolMessage;
use session::PlayerSession;
use std::sync::Arc;

pub async fn handle_client(mut session: Session, state: Arc<WorldState>) -> anyhow::Result<()> {
    let peer = session.peer_addr()?;
    tracing::info!(%peer, "World client connected");

    // Handshake
    session
        .send(&ProtocolRequired {
            version: state.config.protocol_version.clone(),
        })
        .await?;
    session.send(&HelloGameMessage {}).await?;

    // Ticket
    let raw = match session.recv().await? {
        Some(raw) => raw,
        None => return Ok(()),
    };

    if raw.message_id != AuthenticationTicketMessage::MESSAGE_ID {
        tracing::warn!(%peer, message_id = raw.message_id, "Expected AuthenticationTicketMessage");
        return Ok(());
    }

    let ticket_msg = match ProtocolMessage::from_raw(raw.message_id, raw.payload) {
        Ok(ProtocolMessage::AuthenticationTicketMessage(m)) => m,
        _ => return Ok(()),
    };

    let account_id = match ticket::handle_ticket(&mut session, &state, &ticket_msg).await? {
        Some(id) => id,
        None => return Ok(()),
    };

    let mut ps = PlayerSession::new(account_id);

    // Main game loop
    loop {
        tokio::select! {
            result = session.recv() => {
                let raw = match result? {
                    Some(raw) => raw,
                    None => break,
                };

                match ProtocolMessage::from_raw(raw.message_id, raw.payload.clone()) {
                    Ok(ProtocolMessage::BasicPingMessage(ping)) => {
                        session.send(&BasicPongMessage { quiet: ping.quiet }).await?;
                    }
                    Ok(ref msg) => {
                        // Try each dispatcher in order
                        if !approach::dispatch(msg, &mut session, &state, &mut ps).await?
                        && !combat::dispatch(msg, &mut session, &state, &mut ps).await?
                        && !roleplay::dispatch(msg, &mut session, &state, &mut ps).await? {
                            tracing::debug!(%peer, message = %msg, "Unhandled message");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(%peer, message_id = raw.message_id, error = %e, "Failed to parse message");
                    }
                }
            }

            Some(raw_msg) = ps.broadcast_rx.recv() => {
                session.send_raw(raw_msg).await?;
            }
        }
    }

    // Cleanup
    if let (Some(char_id), Some(map_id)) = (ps.character_id, ps.map_id) {
        let players = state.world.get_players_on_map(map_id).await;
        if let Some(player) = players.iter().find(|p| p.character_id == char_id) {
            let _ = repository::update_character_position(
                &state.pool, char_id, map_id, player.cell_id as i32, player.direction as i32,
            ).await;
        }
        state.world.leave_map(map_id, char_id).await;
        tracing::info!(%peer, character_id = char_id, "Player left world");
    }

    tracing::info!(%peer, "Client disconnected");
    Ok(())
}
