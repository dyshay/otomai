//! Objective checking: talk_to_npc, go_to_map, defeat_monster, reach_level.

use super::objective_types;
use super::tracking::complete_step_if_done;
use crate::WorldState;
use dofus_database::repository;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

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
                let mut updated_objectives = objectives.clone();
                for obj in updated_objectives.iter_mut() {
                    if obj.get("id").and_then(|v| v.as_i64()).unwrap_or(0) as i32 == obj_id {
                        obj.as_object_mut().map(|o| o.insert("completed".to_string(), serde_json::json!(true)));
                    }
                }

                session.send(&QuestObjectiveValidatedMessage {
                    quest_id: quest.quest_id as i16,
                    objective_id: obj_id as i16,
                }).await?;

                let json = serde_json::Value::Array(updated_objectives);
                complete_step_if_done(session, state, character_id, quest.quest_id, quest.step_id, &json).await?;
                break;
            }
        }
    }
    Ok(())
}

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
                session.send(&QuestObjectiveValidatedMessage {
                    quest_id: quest.quest_id as i16,
                    objective_id: obj_id as i16,
                }).await?;
            }
        }
    }
    Ok(())
}

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

            if completed { continue; }

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
                session.send(&QuestObjectiveValidatedMessage {
                    quest_id: quest.quest_id as i16,
                    objective_id: obj_id as i16,
                }).await?;
            }
        }

        if updated {
            let json = serde_json::Value::Array(updated_objectives);
            complete_step_if_done(session, state, character_id, quest.quest_id, quest.step_id, &json).await?;
        }
    }
    Ok(())
}

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
                session.send(&QuestObjectiveValidatedMessage {
                    quest_id: quest.quest_id as i16,
                    objective_id: obj_id as i16,
                }).await?;
            }
        }

        if updated {
            let json = serde_json::Value::Array(updated_objectives);
            complete_step_if_done(session, state, character_id, quest.quest_id, quest.step_id, &json).await?;
        }
    }
    Ok(())
}
