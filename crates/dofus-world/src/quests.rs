use crate::WorldState;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::types::QuestActiveInformations;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Quest objective type IDs (from QuestObjectiveTypes.d2o).
/// Only the types we handle in Phase 4. Others are noted for future phases.
pub mod objective_types {
    pub const TALK_TO_NPC: i32 = 0; // parameter0 = npc_id
    pub const GO_TO_MAP: i32 = 3; // mapId field on objective
    pub const DISCOVER_SUBAREA: i32 = 5; // parameter0 = subarea_id

    // Future phases:
    // DEFEAT_MONSTER = 1       // Phase 5: parameter0 = monster_id, parameter1 = count
    // COLLECT_ITEM = 2         // Phase 6: parameter0 = item_id, parameter1 = count
    // CRAFT_ITEM = 4           // Phase 6: parameter0 = item_id
    // HARVEST_RESOURCE = 6     // Phase 6: parameter0 = resource_id
    // REACH_LEVEL = 7          // Phase 5: parameter0 = level
    // WIN_FIGHT_ON_MAP = 8     // Phase 5: mapId field
}

/// QuestListMessage ID (has polymorphic active_quests vector).
const QUEST_LIST_MSG_ID: u16 = 7788;

/// Handle QuestListRequestMessage — send all quests for a character.
pub async fn handle_quest_list(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
) -> anyhow::Result<()> {
    let all_quests = repository::get_character_quests(&state.pool, character_id).await?;

    let finished_ids: Vec<i16> = all_quests
        .iter()
        .filter(|q| q.status == 1)
        .map(|q| q.quest_id as i16)
        .collect();
    let finished_counts: Vec<i16> = vec![1i16; finished_ids.len()];

    let reinit_ids: Vec<i16> = vec![];

    // Active quests — polymorphic vector of QuestActiveInformations (TYPE_ID 2513)
    let active: Vec<&dofus_database::models::CharacterQuest> =
        all_quests.iter().filter(|q| q.status == 0).collect();

    // Build the message manually (polymorphic active_quests field)
    let mut w = BigEndianWriter::new();
    // finished_quests_ids
    w.write_short(finished_ids.len() as i16);
    for id in &finished_ids {
        w.write_var_short(*id);
    }
    // finished_quests_counts
    w.write_short(finished_counts.len() as i16);
    for c in &finished_counts {
        w.write_var_short(*c);
    }
    // active_quests (polymorphic)
    w.write_short(active.len() as i16);
    for quest in &active {
        w.write_ushort(QuestActiveInformations::TYPE_ID);
        let info = QuestActiveInformations {
            quest_id: quest.quest_id as i16,
        };
        info.serialize(&mut w);
    }
    // reinit_done_quests_ids
    w.write_short(reinit_ids.len() as i16);

    session
        .send_raw(RawMessage {
            message_id: QUEST_LIST_MSG_ID,
            instance_id: 0,
            payload: w.into_data(),
        })
        .await?;
    Ok(())
}

/// Start a quest for a character.
pub async fn start_quest(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    quest_id: i32,
) -> anyhow::Result<()> {
    // Get quest data from D2O to find first step
    let step_id = get_first_step_id(state, quest_id).await.unwrap_or(0);
    let objectives = get_step_objectives(state, step_id).await;

    repository::start_quest(
        &state.pool,
        character_id,
        quest_id,
        step_id,
        &objectives,
    )
    .await?;

    session
        .send(&QuestStartedMessage {
            quest_id: quest_id as i16,
        })
        .await?;
    session
        .send(&QuestStepStartedMessage {
            quest_id: quest_id as i16,
            step_id: step_id as i16,
        })
        .await?;

    tracing::info!(character_id, quest_id, step_id, "Quest started");
    Ok(())
}

/// Check and validate a "talk to NPC" objective.
/// Called after a dialogue with an NPC completes.
pub async fn check_talk_to_npc_objective(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    npc_id: i32,
) -> anyhow::Result<()> {
    let active = repository::get_active_quests(&state.pool, character_id).await?;

    for quest in &active {
        let objectives = quest.objectives.as_array().cloned().unwrap_or_default();

        for obj_value in &objectives {
            let obj_type = obj_value.get("type").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let param0 = obj_value.get("param0").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let obj_id = obj_value.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let completed = obj_value.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);

            if obj_type == objective_types::TALK_TO_NPC && param0 == npc_id && !completed {
                // Mark objective as completed
                let mut updated_objectives = objectives.clone();
                for obj in updated_objectives.iter_mut() {
                    if obj.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32 == obj_id {
                        obj.as_object_mut().map(|o| o.insert("completed".to_string(), serde_json::json!(true)));
                    }
                }

                session
                    .send(&QuestObjectiveValidatedMessage {
                        quest_id: quest.quest_id as i16,
                        objective_id: obj_id as i16,
                    })
                    .await?;

                // Check if all objectives are completed
                let all_done = updated_objectives.iter().all(|o| {
                    o.get("completed").and_then(|v| v.as_bool()).unwrap_or(false)
                });

                let objectives_json = serde_json::Value::Array(updated_objectives);

                if all_done {
                    // Complete step — try next step or complete quest
                    let next_step = get_next_step_id(state, quest.quest_id, quest.step_id).await;

                    session
                        .send(&QuestStepValidatedMessage {
                            quest_id: quest.quest_id as i16,
                            step_id: quest.step_id as i16,
                        })
                        .await?;

                    match next_step {
                        Some(next_id) => {
                            let next_objectives = get_step_objectives(state, next_id).await;
                            repository::update_quest_step(
                                &state.pool, character_id, quest.quest_id,
                                next_id, &next_objectives,
                            ).await?;
                            session.send(&QuestStepStartedMessage {
                                quest_id: quest.quest_id as i16,
                                step_id: next_id as i16,
                            }).await?;
                        }
                        None => {
                            repository::complete_quest(
                                &state.pool, character_id, quest.quest_id,
                            ).await?;
                            session.send(&QuestValidatedMessage {
                                quest_id: quest.quest_id as i16,
                            }).await?;
                            tracing::info!(character_id, quest_id = quest.quest_id, "Quest completed");
                        }
                    }
                } else {
                    repository::update_quest_step(
                        &state.pool, character_id, quest.quest_id,
                        quest.step_id, &objectives_json,
                    ).await?;
                }

                break;
            }
        }
    }
    Ok(())
}

/// Check "go to map" / "discover subarea" objectives on map change.
pub async fn check_map_objectives(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    map_id: i64,
) -> anyhow::Result<()> {
    let active = repository::get_active_quests(&state.pool, character_id).await?;

    for quest in &active {
        let objectives = quest.objectives.as_array().cloned().unwrap_or_default();

        for obj_value in &objectives {
            let obj_type = obj_value.get("type").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let obj_map = obj_value.get("mapId").and_then(|v| v.as_i64()).unwrap_or(0);
            let obj_id = obj_value.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let completed = obj_value.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);

            if !completed && obj_type == objective_types::GO_TO_MAP && obj_map == map_id {
                session
                    .send(&QuestObjectiveValidatedMessage {
                        quest_id: quest.quest_id as i16,
                        objective_id: obj_id as i16,
                    })
                    .await?;
                // Simplified: just mark completed. Full step/quest completion
                // would follow the same pattern as check_talk_to_npc_objective.
            }
        }
    }
    Ok(())
}

// ─── D2O helpers ───────────────────────────────────────────────────

/// Get the first step ID of a quest from D2O.
async fn get_first_step_id(state: &Arc<WorldState>, quest_id: i32) -> Option<i32> {
    let quest_data = repository::get_game_data(&state.pool, "Quests", quest_id)
        .await
        .ok()??;
    quest_data
        .data
        .get("stepIds")
        .and_then(|v| v.as_array())
        .and_then(|a| a.first())
        .and_then(|v| v.as_i64())
        .map(|v| v as i32)
}

/// Get the next step ID after the current one.
async fn get_next_step_id(state: &Arc<WorldState>, quest_id: i32, current_step_id: i32) -> Option<i32> {
    let quest_data = repository::get_game_data(&state.pool, "Quests", quest_id)
        .await
        .ok()??;
    let steps = quest_data.data.get("stepIds")?.as_array()?;
    let current_idx = steps.iter().position(|v| v.as_i64() == Some(current_step_id as i64))?;
    steps.get(current_idx + 1).and_then(|v| v.as_i64()).map(|v| v as i32)
}

/// Build initial objectives JSON from a step's D2O data.
async fn get_step_objectives(state: &Arc<WorldState>, step_id: i32) -> serde_json::Value {
    let step_data = match repository::get_game_data(&state.pool, "QuestSteps", step_id).await {
        Ok(Some(d)) => d,
        _ => return serde_json::json!([]),
    };

    let objective_ids = step_data
        .data
        .get("objectiveIds")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut objectives = Vec::new();
    for obj_id_val in &objective_ids {
        let obj_id = obj_id_val.as_i64().unwrap_or(0) as i32;
        if let Ok(Some(obj_data)) =
            repository::get_game_data(&state.pool, "QuestObjectives", obj_id).await
        {
            let type_id = obj_data.data.get("typeId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let map_id = obj_data.data.get("mapId").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64;

            // Extract parameters
            let params = obj_data.data.get("parameters");
            let param0 = params
                .and_then(|p| p.get("parameter0"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;

            objectives.push(serde_json::json!({
                "id": obj_id,
                "type": type_id,
                "param0": param0,
                "mapId": map_id,
                "completed": false,
            }));
        }
    }

    serde_json::Value::Array(objectives)
}

/// Compute quest flags for a specific NPC and player.
/// Returns (quests_to_start, quests_to_valid) based on player's quest state.
pub async fn compute_npc_quest_flags(
    state: &Arc<WorldState>,
    character_id: i64,
    npc_id: i32,
) -> (Vec<i16>, Vec<i16>) {
    // This would query D2O to find quests where an objective references this NPC,
    // then check against the player's completed/active quests.
    // For now, return empty — quest flags will be populated as quests are added to the DB.
    let _ = (state, character_id, npc_id);
    (vec![], vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objective_type_constants() {
        assert_eq!(objective_types::TALK_TO_NPC, 0);
        assert_eq!(objective_types::GO_TO_MAP, 3);
        assert_eq!(objective_types::DISCOVER_SUBAREA, 5);
    }

    #[test]
    fn quest_list_empty_payload() {
        // Manually build empty quest list
        let mut w = BigEndianWriter::new();
        w.write_short(0); // finished_ids
        w.write_short(0); // finished_counts
        w.write_short(0); // active (polymorphic)
        w.write_short(0); // reinit_ids
        let data = w.into_data();
        assert_eq!(data.len(), 8); // 4 × short(0)
    }

    #[test]
    fn quest_started_serializes() {
        use dofus_io::{BigEndianReader, DofusDeserialize, DofusSerialize};

        let msg = QuestStartedMessage { quest_id: 42 };
        let mut w = BigEndianWriter::new();
        msg.serialize(&mut w);

        let mut r = BigEndianReader::new(w.into_data());
        let decoded = QuestStartedMessage::deserialize(&mut r).unwrap();
        assert_eq!(decoded.quest_id, 42);
    }
}
