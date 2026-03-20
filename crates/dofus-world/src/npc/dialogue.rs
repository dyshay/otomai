use crate::WorldState;
use dofus_database::repository;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use super::actors::npc_contextual_id;

/// State for an active NPC dialogue.
pub struct NpcDialogState {
    pub npc_id: i32,
    pub current_message_index: usize,
}

/// Handle NpcGenericActionRequestMessage — player interacts with NPC.
/// Action 3 = talk (dialogue). Other actions can be added later.
pub async fn handle_npc_action(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    msg: &NpcGenericActionRequestMessage,
) -> anyhow::Result<Option<NpcDialogState>> {
    // Verify NPC exists on this map
    let spawns = repository::list_npc_spawns_for_map(&state.pool, current_map_id).await?;
    let spawn = spawns
        .iter()
        .find(|s| npc_contextual_id(s.id) == msg.npc_id as f64 || s.npc_id == msg.npc_id);

    let spawn = match spawn {
        Some(s) => s,
        None => {
            tracing::warn!(character_id, npc_id = msg.npc_id, "NPC not found on map");
            return Ok(None);
        }
    };

    // Action 3 = talk
    if msg.npc_action_id == 3 {
        // Open dialogue
        session
            .send(&NpcDialogCreationMessage {
                map_id: current_map_id as f64,
                npc_id: spawn.npc_id,
            })
            .await?;

        // Get first dialogue message from D2O
        let (message_id, replies) = get_npc_dialog(state, spawn.npc_id, 0).await;

        session
            .send(&NpcDialogQuestionMessage {
                message_id,
                dialog_params: vec![],
                visible_replies: replies.clone(),
            })
            .await?;

        return Ok(Some(NpcDialogState {
            npc_id: spawn.npc_id,
            current_message_index: 0,
        }));
    }

    Ok(None)
}

/// Handle NpcDialogReplyMessage — player chose a dialogue option.
pub async fn handle_npc_dialog_reply(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    dialog_state: &mut NpcDialogState,
    reply_id: i32,
) -> anyhow::Result<bool> {
    // Get dialogue data
    if let Ok(Some(npc_data)) =
        repository::get_game_data(&state.pool, "Npcs", dialog_state.npc_id).await
    {
        let messages = npc_data
            .data
            .get("dialogMessages")
            .and_then(|v| v.as_array());
        let replies = npc_data
            .data
            .get("dialogReplies")
            .and_then(|v| v.as_array());

        if let (Some(msgs), Some(rpls)) = (messages, replies) {
            // Find which reply index was chosen
            if let Some(current_replies) = rpls.get(dialog_state.current_message_index) {
                let reply_list: Vec<i32> = current_replies
                    .as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_i64().map(|n| n as i32)).collect())
                    .unwrap_or_default();

                if let Some(reply_idx) = reply_list.iter().position(|&r| r == reply_id) {
                    let next_msg_idx = dialog_state.current_message_index + 1;

                    if next_msg_idx < msgs.len() {
                        // Next dialogue step
                        let next_msg_id = msgs[next_msg_idx]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as i32;

                        let next_replies: Vec<i32> = rpls
                            .get(next_msg_idx)
                            .and_then(|v| v.as_array())
                            .map(|a| a.iter().filter_map(|v| v.as_i64().map(|n| n as i32)).collect())
                            .unwrap_or_default();

                        dialog_state.current_message_index = next_msg_idx;

                        session
                            .send(&NpcDialogQuestionMessage {
                                message_id: next_msg_id,
                                dialog_params: vec![],
                                visible_replies: next_replies,
                            })
                            .await?;

                        return Ok(true); // dialogue continues
                    }
                }
            }
        }
    }

    // End of dialogue — close
    session
        .send(&LeaveDialogMessage { dialog_type: 2 }) // 2 = NPC dialog
        .await?;
    Ok(false) // dialogue ended
}

/// Get NPC dialogue message ID and replies for a given index.
async fn get_npc_dialog(
    state: &Arc<WorldState>,
    npc_id: i32,
    index: usize,
) -> (i32, Vec<i32>) {
    if let Ok(Some(npc_data)) = repository::get_game_data(&state.pool, "Npcs", npc_id).await {
        let messages = npc_data.data.get("dialogMessages").and_then(|v| v.as_array());
        let replies = npc_data.data.get("dialogReplies").and_then(|v| v.as_array());

        if let (Some(msgs), Some(rpls)) = (messages, replies) {
            let msg_id = msgs
                .get(index)
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;

            let reply_ids: Vec<i32> = rpls
                .get(index)
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_i64().map(|n| n as i32)).collect())
                .unwrap_or_default();

            return (msg_id, reply_ids);
        }
    }
    (0, vec![])
}
