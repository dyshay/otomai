//! Approach phase: character list, selection, creation.

use super::session::PlayerSession;
use crate::{character_selection, emotes, game_context, WorldState};
use dofus_database::repository;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use dofus_protocol::registry::ProtocolMessage;
use std::sync::Arc;

pub async fn dispatch(
    msg: &ProtocolMessage,
    session: &mut Session,
    state: &Arc<WorldState>,
    ps: &mut PlayerSession,
) -> anyhow::Result<bool> {
    match msg {
        ProtocolMessage::CharactersListRequestMessage(_) => {
            character_selection::handle_characters_list_request(session, state, ps.account_id).await?;
        }
        ProtocolMessage::CharacterSelectionMessage(sel) => {
            tracing::info!(character_id = sel.id, "CharacterSelectionMessage received");
            if character_selection::handle_character_selection(session, state, ps.account_id, sel.id).await? {
                if let Some(c) = repository::get_character(&state.pool, sel.id).await? {
                    ps.character_name = Some(c.name.clone());
                }
                ps.character_id = Some(sel.id);
            }
        }
        ProtocolMessage::CharacterFirstSelectionMessage(sel) => {
            tracing::info!(character_id = sel.id, "CharacterFirstSelectionMessage received");
            if character_selection::handle_character_selection(session, state, ps.account_id, sel.id).await? {
                if let Some(c) = repository::get_character(&state.pool, sel.id).await? {
                    ps.character_name = Some(c.name.clone());
                }
                ps.character_id = Some(sel.id);
            }
        }
        ProtocolMessage::CharacterNameSuggestionRequestMessage(_) => {
            character_selection::handle_name_suggestion(session).await?;
        }
        ProtocolMessage::CharacterCreationRequestMessage(msg) => {
            character_selection::handle_character_creation(session, state, ps.account_id, msg).await?;
        }
        ProtocolMessage::GameContextCreateRequestMessage(_) => {
            if let Some(char_id) = ps.character_id {
                if let Some(character) = repository::get_character(&state.pool, char_id).await? {
                    game_context::handle_game_context_create(
                        session, state, &character, &ps.broadcast_tx,
                    ).await?;
                    let map_id = if character.map_id == 0 { 154010883 } else { character.map_id };
                    ps.map_id = Some(map_id);
                    emotes::send_emote_list(session).await?;
                }
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
