use crate::{character_selection, game_context, ticket, WorldState};
use dofus_io::DofusMessage as _;
use dofus_protocol::messages::auth::ProtocolRequired;
use dofus_protocol::messages::game::*;
use dofus_protocol::registry::ProtocolMessage;
use dofus_network::session::Session;
use std::sync::Arc;

pub async fn handle_client(mut session: Session, state: Arc<WorldState>) -> anyhow::Result<()> {
    let peer = session.peer_addr()?;
    tracing::info!(%peer, "World client connected");

    // Step 1: Send ProtocolRequired
    let proto_version: i32 = state.config.protocol_version.parse().unwrap_or(1966);
    session
        .send(&ProtocolRequired {
            required_version: proto_version,
            current_version: proto_version,
        })
        .await?;

    // Step 2: Send HelloGameMessage
    session.send(&HelloGameMessage {}).await?;

    // Step 3: Wait for AuthenticationTicketMessage
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

    // Step 4-5: Validate ticket, send acceptance + capabilities + time
    let account_id = match ticket::handle_ticket(&mut session, &state, &ticket_msg).await? {
        Some(id) => id,
        None => return Ok(()),
    };

    // Step 6: Main game loop — dispatch messages
    loop {
        let raw = match session.recv().await? {
            Some(raw) => raw,
            None => break,
        };

        match ProtocolMessage::from_raw(raw.message_id, raw.payload.clone()) {
            Ok(ProtocolMessage::BasicPingMessage(ping)) => {
                session.send(&BasicPongMessage { quiet: ping.quiet }).await?;
            }
            Ok(ProtocolMessage::CharactersListRequestMessage(_)) => {
                character_selection::handle_characters_list_request(
                    &mut session,
                    &state,
                    account_id,
                )
                .await?;
            }
            Ok(ProtocolMessage::CharacterSelectionMessage(sel)) => {
                if character_selection::handle_character_selection(
                    &mut session,
                    &state,
                    account_id,
                    sel.id,
                )
                .await?
                {
                    // Character selected — wait for GameContextCreateRequestMessage
                    // (the client may also send other messages in between)
                }
            }
            Ok(ProtocolMessage::CharacterNameSuggestionRequestMessage(_)) => {
                character_selection::handle_name_suggestion(&mut session).await?;
            }
            Ok(ProtocolMessage::CharacterCreationRequestMessage(msg)) => {
                character_selection::handle_character_creation(
                    &mut session,
                    &state,
                    account_id,
                    &msg,
                )
                .await?;
            }
            Ok(ProtocolMessage::GameContextCreateRequestMessage(_)) => {
                game_context::handle_game_context_create(&mut session).await?;
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
