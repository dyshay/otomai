use crate::WorldState;
use dofus_database::models::NpcSpawn;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::types::{
    EntityDispositionInformations, EntityDispositionInformationsVariant, EntityLook,
    GameRolePlayNpcQuestFlag, GameRolePlayNpcWithQuestInformations,
};
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Negative contextual IDs for NPCs (convention: negative to distinguish from players).
/// Each NPC spawn gets a unique negative ID based on its spawn row ID.
fn npc_contextual_id(spawn_id: i32) -> f64 {
    -(spawn_id as f64) - 1000.0
}

/// Parse an EntityLook from the string format stored in Npcs.d2o / npc_spawns.
/// Format: "{bonesId|skin1,skin2|color1,color2|scale1}"
/// For simplicity, if empty or unparseable, return a default NPC look.
fn parse_npc_look(look_str: &str) -> EntityLook {
    if look_str.is_empty() {
        return EntityLook {
            bones_id: 1,
            skins: vec![],
            indexed_colors: vec![],
            scales: vec![100],
            subentities: vec![],
        };
    }

    // Strip outer braces
    let inner = look_str.trim_matches(|c| c == '{' || c == '}');
    let parts: Vec<&str> = inner.split('|').collect();

    let bones_id = parts.first().and_then(|s| s.parse::<i16>().ok()).unwrap_or(1);

    let skins = parts
        .get(1)
        .map(|s| {
            s.split(',')
                .filter_map(|v| v.parse::<i16>().ok())
                .collect()
        })
        .unwrap_or_default();

    let indexed_colors = parts
        .get(2)
        .map(|s| {
            s.split(',')
                .filter_map(|v| v.parse::<i32>().ok())
                .collect()
        })
        .unwrap_or_default();

    let scales = parts
        .get(3)
        .map(|s| {
            s.split(',')
                .filter_map(|v| v.parse::<i16>().ok())
                .collect()
        })
        .unwrap_or_else(|| vec![100]);

    EntityLook {
        bones_id,
        skins,
        indexed_colors,
        scales,
        subentities: vec![],
    }
}

/// Build NPC actor data for MapComplementary, including quest flags for a specific player.
pub fn build_npc_actor(
    spawn: &NpcSpawn,
    npc_look: &EntityLook,
    quests_to_start: Vec<i16>,
    quests_to_valid: Vec<i16>,
) -> GameRolePlayNpcWithQuestInformations {
    GameRolePlayNpcWithQuestInformations {
        contextual_id: npc_contextual_id(spawn.id),
        disposition: Box::new(EntityDispositionInformationsVariant::EntityDispositionInformations(
            EntityDispositionInformations {
                cell_id: spawn.cell_id as i16,
                direction: spawn.direction as u8,
            },
        )),
        look: npc_look.clone(),
        npc_id: spawn.npc_id as i16,
        sex: false,
        special_artwork_id: 0,
        quest_flag: GameRolePlayNpcQuestFlag {
            quests_to_valid_id: quests_to_valid,
            quests_to_start_id: quests_to_start,
        },
    }
}

/// Write NPC actors into the MapComplementary actor list.
/// Must be called during actor serialization (after player actors).
pub fn write_npc_actors(
    w: &mut BigEndianWriter,
    npcs: &[GameRolePlayNpcWithQuestInformations],
) {
    for npc in npcs {
        w.write_ushort(GameRolePlayNpcWithQuestInformations::TYPE_ID);
        npc.serialize(w);
    }
}

/// Get NPC look from D2O data (Npcs.d2o stored in game_data table).
/// Falls back to spawn's look field or default.
pub async fn get_npc_look(
    state: &Arc<WorldState>,
    npc_id: i32,
    spawn_look: &str,
) -> EntityLook {
    // Try spawn's own look first
    if !spawn_look.is_empty() {
        return parse_npc_look(spawn_look);
    }

    // Try D2O data
    if let Ok(Some(game_data)) =
        repository::get_game_data(&state.pool, "Npcs", npc_id).await
    {
        if let Some(look_str) = game_data.data.get("look").and_then(|v| v.as_str()) {
            return parse_npc_look(look_str);
        }
    }

    // Default
    EntityLook {
        bones_id: 1,
        skins: vec![],
        indexed_colors: vec![],
        scales: vec![100],
        subentities: vec![],
    }
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

/// State for an active NPC dialogue.
pub struct NpcDialogState {
    pub npc_id: i32,
    pub current_message_index: usize,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_npc_look_basic() {
        let look = parse_npc_look("{1|100,200|16777215|120}");
        assert_eq!(look.bones_id, 1);
        assert_eq!(look.skins, vec![100, 200]);
        assert_eq!(look.scales, vec![120]);
    }

    #[test]
    fn parse_npc_look_empty() {
        let look = parse_npc_look("");
        assert_eq!(look.bones_id, 1);
        assert!(look.skins.is_empty());
    }

    #[test]
    fn npc_contextual_id_is_negative() {
        assert!(npc_contextual_id(1) < 0.0);
        assert!(npc_contextual_id(100) < 0.0);
        assert_ne!(npc_contextual_id(1), npc_contextual_id(2));
    }
}
