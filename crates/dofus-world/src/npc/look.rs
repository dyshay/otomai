use crate::WorldState;
use dofus_database::repository;
use dofus_protocol::generated::types::EntityLook;
use std::sync::Arc;

/// Parse an EntityLook from the string format stored in Npcs.d2o / npc_spawns.
/// Format: "{bonesId|skin1,skin2|color1,color2|scale1}"
/// For simplicity, if empty or unparseable, return a default NPC look.
pub fn parse_npc_look(look_str: &str) -> EntityLook {
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
