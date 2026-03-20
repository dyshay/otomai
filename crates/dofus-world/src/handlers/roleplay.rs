//! Roleplay dispatch: movement, chat, emotes, social, NPCs, quests.

use super::session::PlayerSession;
use crate::{chat, emotes, movement, npc, quests, social, WorldState};
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
        // Movement
        ProtocolMessage::GameMapMovementRequestMessage(msg) if ps.fight.is_none() => {
            if let (Some(char_id), Some(map_id)) = (ps.character_id, ps.map_id) {
                ps.movement = movement::handle_movement_request(session, state, char_id, map_id, msg).await?;
            }
        }
        ProtocolMessage::GameMapMovementConfirmMessage(_) => {
            if let (Some(char_id), Some(map_id)) = (ps.character_id, ps.map_id) {
                movement::handle_movement_confirm(state, char_id, map_id, ps.movement.as_ref()).await?;
                ps.movement = None;
            }
        }
        ProtocolMessage::GameMapMovementCancelMessage(msg) => {
            if let (Some(char_id), Some(map_id)) = (ps.character_id, ps.map_id) {
                movement::handle_movement_cancel(state, char_id, map_id, msg.cell_id).await?;
                ps.movement = None;
            }
        }
        ProtocolMessage::ChangeMapMessage(msg) => {
            if let Some(char_id) = ps.character_id {
                if let Some(map_id) = ps.map_id {
                    if let Some(character) = repository::get_character(&state.pool, char_id).await? {
                        if let Some(new_map_id) = movement::handle_change_map(
                            session, state, &character, map_id, msg.map_id as i64, &ps.broadcast_tx,
                        ).await? {
                            ps.map_id = Some(new_map_id);
                            quests::check_map_objectives(session, state, char_id, new_map_id).await?;
                        }
                    }
                }
            }
        }
        ProtocolMessage::GameMapChangeOrientationRequestMessage(msg) => {
            if let (Some(char_id), Some(_)) = (ps.character_id, ps.map_id) {
                session.send(&GameMapChangeOrientationMessage {
                    orientation: dofus_protocol::generated::types::ActorOrientation {
                        id: char_id as f64,
                        direction: msg.direction,
                    },
                }).await?;
            }
        }

        // Chat
        ProtocolMessage::ChatClientMultiMessage(msg) => {
            if let (Some(char_id), Some(map_id), Some(ref name)) = (ps.character_id, ps.map_id, &ps.character_name) {
                chat::handle_chat_multi(session, state, char_id, name, ps.account_id, map_id, msg).await?;
            }
        }
        ProtocolMessage::ChatClientPrivateMessage(msg) => {
            if let (Some(char_id), Some(ref name)) = (ps.character_id, &ps.character_name) {
                chat::handle_chat_private(session, state, char_id, name, ps.account_id, msg).await?;
            }
        }

        // Emotes
        ProtocolMessage::EmotePlayRequestMessage(msg) => {
            if let (Some(char_id), Some(map_id)) = (ps.character_id, ps.map_id) {
                emotes::handle_emote_play(state, char_id, ps.account_id, map_id, msg).await?;
            }
        }

        // Social
        ProtocolMessage::FriendsGetListMessage(_) => {
            social::handle_friends_get_list(session, state, ps.account_id).await?;
        }
        ProtocolMessage::FriendAddRequestMessage(msg) => {
            social::handle_friend_add(session, state, ps.account_id, &msg.name).await?;
        }
        ProtocolMessage::FriendDeleteRequestMessage(msg) => {
            social::handle_friend_delete(session, state, ps.account_id, msg.account_id).await?;
        }
        ProtocolMessage::IgnoredGetListMessage(_) => {
            social::handle_ignored_get_list(session, state, ps.account_id).await?;
        }
        ProtocolMessage::IgnoredAddRequestMessage(msg) => {
            social::handle_ignored_add(session, state, ps.account_id, &msg.name).await?;
        }
        ProtocolMessage::IgnoredDeleteRequestMessage(msg) => {
            social::handle_ignored_delete(session, state, ps.account_id, msg.account_id).await?;
        }

        // NPCs + Dialogues
        ProtocolMessage::NpcGenericActionRequestMessage(msg) => {
            if let (Some(char_id), Some(map_id)) = (ps.character_id, ps.map_id) {
                ps.dialog = npc::handle_npc_action(session, state, char_id, map_id, msg).await?;
            }
        }
        ProtocolMessage::NpcDialogReplyMessage(msg) => {
            if let (Some(char_id), Some(ref mut dialog)) = (ps.character_id, &mut ps.dialog) {
                let continues = npc::handle_npc_dialog_reply(session, state, char_id, dialog, msg.reply_id).await?;
                if !continues {
                    let npc_id = dialog.npc_id;
                    ps.dialog = None;
                    quests::check_talk_to_npc_objective(session, state, char_id, npc_id).await?;
                }
            }
        }
        ProtocolMessage::LeaveDialogRequestMessage(_) => {
            if ps.dialog.is_some() {
                session.send(&LeaveDialogMessage { dialog_type: 2 }).await?;
                ps.dialog = None;
            }
        }

        // Quests
        ProtocolMessage::QuestListRequestMessage(_) => {
            if let Some(char_id) = ps.character_id {
                quests::handle_quest_list(session, state, char_id).await?;
            }
        }

        _ => return Ok(false),
    }
    Ok(true)
}
