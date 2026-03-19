use crate::{character_selection, game_context, ticket, WorldState};
use dofus_database::repository;
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
    session
        .send(&ProtocolRequired {
            version: state.config.protocol_version.clone(),
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

    // Player session state — set after character selection + game context
    let mut current_character_id: Option<i64> = None;
    let mut current_map_id: Option<i64> = None;

    // Broadcast channel — created now, used after entering the world
    let (broadcast_tx, mut broadcast_rx) = crate::world::new_broadcast_channel();

    // Step 6: Main game loop — dispatch TCP messages and broadcast messages
    loop {
        tokio::select! {
            // Incoming TCP message from this client
            result = session.recv() => {
                let raw = match result? {
                    Some(raw) => raw,
                    None => break, // disconnect
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
                            current_character_id = Some(sel.id);
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
                        if let Some(char_id) = current_character_id {
                            if let Some(character) = repository::get_character(&state.pool, char_id).await? {
                                game_context::handle_game_context_create(
                                    &mut session,
                                    &state,
                                    &character,
                                    &broadcast_tx,
                                )
                                .await?;

                                let map_id = if character.map_id == 0 { 154010883 } else { character.map_id };
                                current_map_id = Some(map_id);
                            }
                        }
                    }
                    Ok(msg) => {
                        tracing::debug!(%peer, message = %msg, "Unhandled message");
                    }
                    Err(e) => {
                        tracing::warn!(%peer, message_id = raw.message_id, error = %e, "Failed to parse message");
                    }
                }
            }

            // Broadcast message from another player (via world state)
            Some(raw_msg) = broadcast_rx.recv() => {
                session.send_raw(raw_msg).await?;
            }
        }
    }

    // Cleanup: remove player from map on disconnect
    if let (Some(char_id), Some(map_id)) = (current_character_id, current_map_id) {
        state.world.leave_map(map_id, char_id).await;
        tracing::info!(%peer, character_id = char_id, "Player left world");
    }

    tracing::info!(%peer, "Client disconnected");
    Ok(())
}
