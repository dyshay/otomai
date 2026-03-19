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
    pub const TALK_TO_NPC: i32 = 0;
    pub const DEFEAT_MONSTER: i32 = 1;  // parameter0 = monster_id, parameter1 = count
    pub const COLLECT_ITEM: i32 = 2;    // parameter0 = item_id, parameter1 = count
    pub const GO_TO_MAP: i32 = 3;
    pub const CRAFT_ITEM: i32 = 4;      // parameter0 = item_id
    pub const DISCOVER_SUBAREA: i32 = 5;
    pub const HARVEST_RESOURCE: i32 = 6; // parameter0 = resource_id
    pub const REACH_LEVEL: i32 = 7;     // parameter0 = target_level
    pub const WIN_FIGHT_ON_MAP: i32 = 8; // mapId field
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

/// Check DEFEAT_MONSTER objectives after a fight ends.
/// Called with the list of monster IDs killed in the fight.
pub async fn check_defeat_monster_objectives(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    killed_monster_ids: &[i32],
    fight_map_id: i64,
) -> anyhow::Result<()> {
    let active = repository::get_active_quests(&state.pool, character_id).await?;

    for quest in &active {
        let objectives = quest.objectives.as_array().cloned().unwrap_or_default();
        let mut updated = false;
        let mut updated_objectives = objectives.clone();

        for (i, obj_value) in objectives.iter().enumerate() {
            let obj_type = obj_value.get("type").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let param0 = obj_value.get("param0").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let obj_id = obj_value.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let obj_map = obj_value.get("mapId").and_then(|v| v.as_i64()).unwrap_or(0);
            let completed = obj_value.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);

            if completed {
                continue;
            }

            let should_complete = match obj_type {
                objective_types::DEFEAT_MONSTER => killed_monster_ids.contains(&param0),
                objective_types::WIN_FIGHT_ON_MAP => obj_map == fight_map_id,
                _ => false,
            };

            if should_complete {
                if let Some(obj) = updated_objectives.get_mut(i) {
                    obj.as_object_mut().map(|o| o.insert("completed".to_string(), serde_json::json!(true)));
                }
                updated = true;

                session
                    .send(&QuestObjectiveValidatedMessage {
                        quest_id: quest.quest_id as i16,
                        objective_id: obj_id as i16,
                    })
                    .await?;
            }
        }

        if updated {
            let json = serde_json::Value::Array(updated_objectives);
            complete_step_if_done(session, state, character_id, quest.quest_id, quest.step_id, &json).await?;
        }
    }
    Ok(())
}

/// Check REACH_LEVEL objectives after a level up.
pub async fn check_level_objectives(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    new_level: i32,
) -> anyhow::Result<()> {
    let active = repository::get_active_quests(&state.pool, character_id).await?;

    for quest in &active {
        let objectives = quest.objectives.as_array().cloned().unwrap_or_default();
        let mut updated = false;
        let mut updated_objectives = objectives.clone();

        for (i, obj_value) in objectives.iter().enumerate() {
            let obj_type = obj_value.get("type").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let param0 = obj_value.get("param0").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let completed = obj_value.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);

            if !completed && obj_type == objective_types::REACH_LEVEL && new_level >= param0 {
                if let Some(obj) = updated_objectives.get_mut(i) {
                    obj.as_object_mut().map(|o| o.insert("completed".to_string(), serde_json::json!(true)));
                }
                updated = true;

                let obj_id = obj_value.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                session
                    .send(&QuestObjectiveValidatedMessage {
                        quest_id: quest.quest_id as i16,
                        objective_id: obj_id as i16,
                    })
                    .await?;
            }
        }

        if updated {
            let json = serde_json::Value::Array(updated_objectives);
            complete_step_if_done(session, state, character_id, quest.quest_id, quest.step_id, &json).await?;
        }
    }
    Ok(())
}

/// Helper: if all objectives in a step are done, advance or complete the quest.
async fn complete_step_if_done(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    quest_id: i32,
    step_id: i32,
    objectives: &serde_json::Value,
) -> anyhow::Result<()> {
    let all_done = objectives.as_array()
        .map(|arr| arr.iter().all(|o| o.get("completed").and_then(|v| v.as_bool()).unwrap_or(false)))
        .unwrap_or(false);

    if all_done {
        // Award step rewards
        award_step_rewards(session, state, character_id, step_id).await?;

        session
            .send(&QuestStepValidatedMessage {
                quest_id: quest_id as i16,
                step_id: step_id as i16,
            })
            .await?;

        match get_next_step_id(state, quest_id, step_id).await {
            Some(next_id) => {
                let next_objectives = get_step_objectives(state, next_id).await;
                repository::update_quest_step(&state.pool, character_id, quest_id, next_id, &next_objectives).await?;
                session.send(&QuestStepStartedMessage {
                    quest_id: quest_id as i16,
                    step_id: next_id as i16,
                }).await?;
            }
            None => {
                repository::complete_quest(&state.pool, character_id, quest_id).await?;
                session.send(&QuestValidatedMessage {
                    quest_id: quest_id as i16,
                }).await?;
            }
        }
    } else {
        repository::update_quest_step(&state.pool, character_id, quest_id, step_id, objectives).await?;
    }

    Ok(())
}

/// Award rewards for completing a quest step (from QuestStepRewards D2O).
async fn award_step_rewards(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    step_id: i32,
) -> anyhow::Result<()> {
    // QuestStepRewards are indexed by their own ID, not by stepId.
    // We need to scan for the right one matching our step.
    let all_rewards = repository::get_all_game_data(&state.pool, "QuestStepRewards").await?;
    let reward_data = all_rewards.iter().find(|r| {
        r.data.get("stepId").and_then(|v| v.as_i64()).unwrap_or(0) as i32 == step_id
    });

    let reward = match reward_data {
        Some(r) => r,
        None => return Ok(()), // No rewards for this step
    };

    let character = match repository::get_character(&state.pool, character_id).await? {
        Some(c) => c,
        None => return Ok(()),
    };

    // XP reward
    let xp_ratio = reward.data.get("experienceRatio").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if xp_ratio > 0.0 {
        let xp = (xp_ratio * character.level as f64).max(1.0) as i64;
        let new_xp = character.experience + xp;
        let _ = sqlx::query("UPDATE characters SET experience = $2 WHERE id = $1")
            .bind(character_id)
            .bind(new_xp)
            .execute(&state.pool)
            .await;

        session
            .send(&CharacterExperienceGainMessage {
                experience_character: xp,
                experience_mount: 0,
                experience_guild: 0,
                experience_incarnation: 0,
            })
            .await?;
    }

    // Kamas reward
    let kamas_ratio = reward.data.get("kamasRatio").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if kamas_ratio > 0.0 {
        let scale_with_level = reward.data.get("kamasScaleWithPlayerLevel")
            .and_then(|v| v.as_bool()).unwrap_or(false);
        let kamas = if scale_with_level {
            (kamas_ratio * character.level as f64).max(1.0) as i64
        } else {
            kamas_ratio as i64
        };

        let new_kamas = character.kamas + kamas;
        let _ = sqlx::query("UPDATE characters SET kamas = $2 WHERE id = $1")
            .bind(character_id)
            .bind(new_kamas)
            .execute(&state.pool)
            .await;
    }

    // Item rewards: [[item_id, quantity], ...]
    let items = reward.data.get("itemsReward").and_then(|v| v.as_array());
    if let Some(items) = items {
        for item_entry in items {
            if let Some(arr) = item_entry.as_array() {
                let item_id = arr.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let quantity = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                if item_id > 0 && quantity > 0 {
                    let _ = sqlx::query(
                        "INSERT INTO inventory_items (character_id, item_template_id, quantity)
                         VALUES ($1, $2, $3)"
                    )
                    .bind(character_id)
                    .bind(item_id)
                    .bind(quantity)
                    .execute(&state.pool)
                    .await;
                }
            }
        }
    }

    tracing::info!(character_id, step_id, "Quest step rewards awarded");
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
///
/// - quests_to_start: quests the player can accept from this NPC
/// - quests_to_valid: quests where the player has an active objective involving this NPC
pub async fn compute_npc_quest_flags(
    state: &Arc<WorldState>,
    character_id: i64,
    npc_id: i32,
) -> (Vec<i16>, Vec<i16>) {
    let mut to_start = Vec::new();
    let mut to_valid = Vec::new();

    // Load player quest state
    let completed_ids = repository::get_completed_quest_ids(&state.pool, character_id)
        .await
        .unwrap_or_default();
    let active_quests = repository::get_active_quests(&state.pool, character_id)
        .await
        .unwrap_or_default();
    let active_ids: Vec<i32> = active_quests.iter().map(|q| q.quest_id).collect();

    // Get character info for criterion evaluation
    let character = match repository::get_character(&state.pool, character_id).await {
        Ok(Some(c)) => c,
        _ => return (to_start, to_valid),
    };

    let ctx = dofus_common::criterion::CriterionContext {
        level: character.level,
        breed_id: character.breed_id,
        sex: character.sex,
        completed_quest_ids: completed_ids.clone(),
        active_quest_ids: active_ids.clone(),
    };

    // Check active quests: does any have a TALK_TO_NPC objective for this NPC?
    for quest in &active_quests {
        let objectives = quest.objectives.as_array().cloned().unwrap_or_default();
        for obj in &objectives {
            let obj_type = obj.get("type").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let param0 = obj.get("param0").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let completed = obj.get("completed").and_then(|v| v.as_bool()).unwrap_or(false);

            if obj_type == objective_types::TALK_TO_NPC && param0 == npc_id && !completed {
                to_valid.push(quest.quest_id as i16);
                break;
            }
        }
    }

    // Scan all quests in D2O to find ones this NPC can start
    // Look for quests where the first step has a TALK_TO_NPC objective with this NPC
    let all_quests = repository::get_all_game_data(&state.pool, "Quests")
        .await
        .unwrap_or_default();

    for quest_data in &all_quests {
        let quest_id = quest_data.object_id;

        // Skip already active or completed
        if active_ids.contains(&quest_id) || completed_ids.contains(&quest_id) {
            continue;
        }

        // Check startCriterion
        let start_criterion = quest_data.data.get("startCriterion")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !dofus_common::criterion::evaluate(start_criterion, &ctx) {
            continue;
        }

        // Check first step's objectives for TALK_TO_NPC with this NPC
        let step_ids = quest_data.data.get("stepIds")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        if let Some(first_step_id) = step_ids.first().and_then(|v| v.as_i64()) {
            if let Ok(Some(step_data)) = repository::get_game_data(
                &state.pool, "QuestSteps", first_step_id as i32,
            ).await {
                let obj_ids = step_data.data.get("objectiveIds")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                for obj_id_val in &obj_ids {
                    let obj_id = obj_id_val.as_i64().unwrap_or(0) as i32;
                    if let Ok(Some(obj_data)) = repository::get_game_data(
                        &state.pool, "QuestObjectives", obj_id,
                    ).await {
                        let type_id = obj_data.data.get("typeId")
                            .and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
                        let param0 = obj_data.data.get("parameters")
                            .and_then(|p| p.get("parameter0"))
                            .and_then(|v| v.as_i64()).unwrap_or(0) as i32;

                        if type_id == objective_types::TALK_TO_NPC && param0 == npc_id {
                            to_start.push(quest_id as i16);
                            break;
                        }
                    }
                }
            }
        }
    }

    (to_start, to_valid)
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
