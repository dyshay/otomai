//! D2O helpers for quest data lookups.

use crate::WorldState;
use dofus_database::repository;
use std::sync::Arc;

pub(super) async fn get_first_step_id(state: &Arc<WorldState>, quest_id: i32) -> Option<i32> {
    let quest_data = repository::get_game_data(&state.pool, "Quests", quest_id).await.ok()??;
    quest_data.data.get("stepIds")?.as_array()?.first()?.as_i64().map(|v| v as i32)
}

pub(super) async fn get_next_step_id(state: &Arc<WorldState>, quest_id: i32, current_step_id: i32) -> Option<i32> {
    let quest_data = repository::get_game_data(&state.pool, "Quests", quest_id).await.ok()??;
    let steps = quest_data.data.get("stepIds")?.as_array()?;
    let idx = steps.iter().position(|v| v.as_i64() == Some(current_step_id as i64))?;
    steps.get(idx + 1).and_then(|v| v.as_i64()).map(|v| v as i32)
}

pub(super) async fn get_step_objectives(state: &Arc<WorldState>, step_id: i32) -> serde_json::Value {
    let step_data = match repository::get_game_data(&state.pool, "QuestSteps", step_id).await {
        Ok(Some(d)) => d,
        _ => return serde_json::json!([]),
    };

    let objective_ids = step_data.data.get("objectiveIds").and_then(|v| v.as_array()).cloned().unwrap_or_default();

    let mut objectives = Vec::new();
    for obj_id_val in &objective_ids {
        let obj_id = obj_id_val.as_i64().unwrap_or(0) as i32;
        if let Ok(Some(obj_data)) = repository::get_game_data(&state.pool, "QuestObjectives", obj_id).await {
            let type_id = obj_data.data.get("typeId").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let map_id = obj_data.data.get("mapId").and_then(|v| v.as_f64()).unwrap_or(0.0) as i64;
            let param0 = obj_data.data.get("parameters")
                .and_then(|p| p.get("parameter0")).and_then(|v| v.as_i64()).unwrap_or(0) as i32;

            objectives.push(serde_json::json!({
                "id": obj_id, "type": type_id, "param0": param0, "mapId": map_id, "completed": false,
            }));
        }
    }

    serde_json::Value::Array(objectives)
}
