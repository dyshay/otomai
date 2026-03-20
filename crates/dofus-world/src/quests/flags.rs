//! NPC quest flags: compute which quests an NPC can start/validate for a player.

use super::objective_types;
use crate::WorldState;
use dofus_database::repository;
use std::sync::Arc;

pub async fn compute_npc_quest_flags(
    state: &Arc<WorldState>,
    character_id: i64,
    npc_id: i32,
) -> (Vec<i16>, Vec<i16>) {
    let mut to_start = Vec::new();
    let mut to_valid = Vec::new();

    let completed_ids = repository::get_completed_quest_ids(&state.pool, character_id).await.unwrap_or_default();
    let active_quests = repository::get_active_quests(&state.pool, character_id).await.unwrap_or_default();
    let active_ids: Vec<i32> = active_quests.iter().map(|q| q.quest_id).collect();

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

    // Active quests with TALK_TO_NPC objective for this NPC
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

    // Scan D2O for quests this NPC can start
    let all_quests = repository::get_all_game_data(&state.pool, "Quests").await.unwrap_or_default();

    for quest_data in &all_quests {
        let quest_id = quest_data.object_id;
        if active_ids.contains(&quest_id) || completed_ids.contains(&quest_id) { continue; }

        let start_criterion = quest_data.data.get("startCriterion").and_then(|v| v.as_str()).unwrap_or("");
        if !dofus_common::criterion::evaluate(start_criterion, &ctx) { continue; }

        let step_ids = quest_data.data.get("stepIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();

        if let Some(first_step_id) = step_ids.first().and_then(|v| v.as_i64()) {
            if let Ok(Some(step_data)) = repository::get_game_data(&state.pool, "QuestSteps", first_step_id as i32).await {
                let obj_ids = step_data.data.get("objectiveIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();

                for obj_id_val in &obj_ids {
                    let obj_id = obj_id_val.as_i64().unwrap_or(0) as i32;
                    if let Ok(Some(obj_data)) = repository::get_game_data(&state.pool, "QuestObjectives", obj_id).await {
                        let type_id = obj_data.data.get("typeId").and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
                        let param0 = obj_data.data.get("parameters").and_then(|p| p.get("parameter0")).and_then(|v| v.as_i64()).unwrap_or(0) as i32;

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
