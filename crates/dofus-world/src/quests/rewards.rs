//! Quest step rewards: XP, kamas, items from QuestStepRewards D2O.

use crate::WorldState;
use dofus_database::repository;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

pub(super) async fn award_step_rewards(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    step_id: i32,
) -> anyhow::Result<()> {
    let all_rewards = repository::get_all_game_data(&state.pool, "QuestStepRewards").await?;
    let reward_data = all_rewards.iter().find(|r| {
        r.data.get("stepId").and_then(|v| v.as_i64()).unwrap_or(0) as i32 == step_id
    });

    let reward = match reward_data {
        Some(r) => r,
        None => return Ok(()),
    };

    let character = match repository::get_character(&state.pool, character_id).await? {
        Some(c) => c,
        None => return Ok(()),
    };

    // XP
    let xp_ratio = reward.data.get("experienceRatio").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if xp_ratio > 0.0 {
        let xp = (xp_ratio * character.level as f64).max(1.0) as i64;
        let new_xp = character.experience + xp;
        let _ = sqlx::query("UPDATE characters SET experience = $2 WHERE id = $1")
            .bind(character_id).bind(new_xp).execute(&state.pool).await;

        session.send(&CharacterExperienceGainMessage {
            experience_character: xp, experience_mount: 0, experience_guild: 0, experience_incarnation: 0,
        }).await?;
    }

    // Kamas
    let kamas_ratio = reward.data.get("kamasRatio").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if kamas_ratio > 0.0 {
        let scale = reward.data.get("kamasScaleWithPlayerLevel").and_then(|v| v.as_bool()).unwrap_or(false);
        let kamas = if scale { (kamas_ratio * character.level as f64).max(1.0) as i64 } else { kamas_ratio as i64 };
        let new_kamas = character.kamas + kamas;
        let _ = sqlx::query("UPDATE characters SET kamas = $2 WHERE id = $1")
            .bind(character_id).bind(new_kamas).execute(&state.pool).await;
    }

    // Items
    if let Some(items) = reward.data.get("itemsReward").and_then(|v| v.as_array()) {
        for item_entry in items {
            if let Some(arr) = item_entry.as_array() {
                let item_id = arr.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32;
                let quantity = arr.get(1).and_then(|v| v.as_i64()).unwrap_or(1) as i32;
                if item_id > 0 && quantity > 0 {
                    let _ = sqlx::query("INSERT INTO inventory_items (character_id, item_template_id, quantity) VALUES ($1, $2, $3)")
                        .bind(character_id).bind(item_id).bind(quantity).execute(&state.pool).await;
                }
            }
        }
    }

    tracing::info!(character_id, step_id, "Quest step rewards awarded");
    Ok(())
}
