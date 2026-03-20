//! Spell cast validation: AP, range, LOS, cooldowns.

use super::super::state::{Fight, SpellData};
use crate::WorldState;
use dofus_common::pathfinding;
use std::sync::Arc;

/// Validate whether a spell cast is legal. Returns Ok(()) if valid, Err with reason if not.
pub fn validate_cast(
    fight: &Fight,
    state: &Arc<WorldState>,
    player_id: f64,
    spell_id: i16,
    cell_id: i16,
    spell_data: &SpellData,
) -> Result<(), &'static str> {
    let current = match fight.current_fighter() {
        Some(f) if f.id == player_id && f.is_player => f,
        _ => return Err("not_your_turn"),
    };

    // AP cost
    if current.action_points < spell_data.ap_cost {
        return Err("not_enough_ap");
    }

    // Range check
    let dist = pathfinding::distance(current.cell_id as u16, cell_id as u16);
    if (dist as i16) < spell_data.min_range || (dist as i16) > spell_data.range {
        return Err("out_of_range");
    }

    // LOS check
    if spell_data.cast_test_los {
        if let Some(map_data) = state.maps.get(fight.map_id) {
            if !pathfinding::has_line_of_sight(&map_data, current.cell_id as u16, cell_id as u16) {
                return Err("no_line_of_sight");
            }
        }
    }

    // Max cast per turn
    if spell_data.max_cast_per_turn > 0 {
        let count = current.spell_casts_this_turn.get(&(spell_id as i32)).copied().unwrap_or(0);
        if count >= spell_data.max_cast_per_turn {
            return Err("max_cast_per_turn");
        }
    }

    Ok(())
}
