use super::super::state::{Element, SpellData, SpellEffect};
use crate::WorldState;
use dofus_database::repository;
use std::sync::Arc;

/// Load spell data from SpellLevels D2O in game_data table.
pub async fn load_spell_data(
    state: &Arc<WorldState>,
    spell_id: i32,
    spell_level: i32,
) -> Option<SpellData> {
    // First get the Spell to find the SpellLevel ID
    let spell_data = repository::get_game_data(&state.pool, "Spells", spell_id).await.ok()??;
    let level_ids = spell_data.data.get("spellLevels")?.as_array()?;

    let level_idx = (spell_level as usize).saturating_sub(1).min(level_ids.len().saturating_sub(1));
    let level_id = level_ids.get(level_idx)?.as_i64()? as i32;

    // Load the SpellLevel
    let level_data = repository::get_game_data(&state.pool, "SpellLevels", level_id).await.ok()??;
    let d = &level_data.data;

    let effects = parse_effects(d.get("effects"));
    let critical_effects = parse_effects(d.get("criticalEffect"));

    Some(SpellData {
        spell_id,
        level: spell_level,
        ap_cost: d.get("apCost").and_then(|v| v.as_i64()).unwrap_or(3) as i16,
        min_range: d.get("minRange").and_then(|v| v.as_i64()).unwrap_or(1) as i16,
        range: d.get("range").and_then(|v| v.as_i64()).unwrap_or(6) as i16,
        cast_in_line: d.get("castInLine").and_then(|v| v.as_bool()).unwrap_or(false),
        cast_in_diagonal: d.get("castInDiagonal").and_then(|v| v.as_bool()).unwrap_or(false),
        cast_test_los: d.get("castTestLos").and_then(|v| v.as_bool()).unwrap_or(false),
        max_cast_per_turn: d.get("maxCastPerTurn").and_then(|v| v.as_i64()).unwrap_or(0) as i16,
        max_cast_per_target: d.get("maxCastPerTarget").and_then(|v| v.as_i64()).unwrap_or(0) as i16,
        need_free_cell: d.get("needFreeCell").and_then(|v| v.as_bool()).unwrap_or(false),
        need_taken_cell: d.get("needTakenCell").and_then(|v| v.as_bool()).unwrap_or(false),
        critical_hit_probability: d.get("criticalHitProbability").and_then(|v| v.as_i64()).unwrap_or(0) as i16,
        effects,
        critical_effects,
    })
}

fn parse_effects(val: Option<&serde_json::Value>) -> Vec<SpellEffect> {
    let arr = match val.and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return vec![],
    };

    arr.iter()
        .filter_map(|e| {
            let effect_id = e.get("effectId").and_then(|v| v.as_i64())? as i32;
            let dice_num = e.get("diceNum").or_else(|| e.get("value"))
                .and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let dice_side = e.get("diceSide").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let value = e.get("diceConst").or_else(|| e.get("parameter2"))
                .and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let duration = e.get("duration").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

            let element = super::super::effects::element_of(effect_id).unwrap_or(Element::Neutral);

            Some(SpellEffect {
                effect_id,
                dice_num,
                dice_side,
                value,
                duration,
                element,
            })
        })
        .collect()
}
