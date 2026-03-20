//! Map transitions: ChangeMapMessage handling.

use crate::game_context;
use crate::world::MapPlayer;
use crate::WorldState;
use dofus_common::dlm::{self, MapDirection};
use dofus_database::models::Character;
use dofus_database::repository;
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use tokio::sync::mpsc;

pub async fn handle_change_map(
    session: &mut Session,
    state: &Arc<WorldState>,
    character: &Character,
    current_map_id: i64,
    target_map_id: i64,
    broadcast_tx: &mpsc::UnboundedSender<RawMessage>,
) -> anyhow::Result<Option<i64>> {
    let current_cell = {
        let players = state.world.get_players_on_map(current_map_id).await;
        players.iter().find(|p| p.character_id == character.id).map(|p| p.cell_id as u16).unwrap_or(0)
    };

    let direction = match dlm::cell_border(current_cell) {
        Some(d) => d,
        None => {
            tracing::warn!(character_id = character.id, cell = current_cell, "ChangeMap from non-border cell");
            return Ok(None);
        }
    };

    let valid = [MapDirection::Top, MapDirection::Bottom, MapDirection::Left, MapDirection::Right]
        .iter()
        .any(|d| state.maps.get_neighbour(current_map_id, *d) == Some(target_map_id));

    if !valid {
        tracing::warn!(character_id = character.id, current_map_id, target_map_id, "ChangeMap to non-neighbor map");
        return Ok(None);
    }

    let mirror = dlm::mirror_cell(current_cell, direction);
    let dest_cell = if let Some(map_data) = state.maps.get(target_map_id) {
        let opposite = match direction {
            MapDirection::Top => MapDirection::Bottom,
            MapDirection::Bottom => MapDirection::Top,
            MapDirection::Left => MapDirection::Right,
            MapDirection::Right => MapDirection::Left,
        };
        map_data.nearest_walkable_on_border(mirror, opposite).unwrap_or(mirror)
    } else {
        mirror
    };

    state.world.leave_map(current_map_id, character.id).await;

    session.send(&CurrentMapMessage {
        map_id: target_map_id as f64,
        map_key: crate::constants::MAP_ENCRYPTION_KEY.to_string(),
    }).await?;

    let entity_look = game_context::build_entity_look(character);
    let map_player = MapPlayer {
        character_id: character.id,
        account_id: character.account_id,
        name: character.name.clone(),
        entity_look,
        cell_id: dest_cell as i16,
        direction: character.direction as u8,
        level: character.level as i16,
        breed: character.breed_id as u8,
        sex: character.sex != 0,
        tx: broadcast_tx.clone(),
    };

    state.world.join_map(target_map_id, map_player).await;

    let players = state.world.get_players_on_map(target_map_id).await;
    let sub_area_id = state.maps.get(target_map_id).map(|m| m.sub_area_id as i16).unwrap_or(449);
    let npc_actors = game_context::build_npc_actors_for_map(state, character.id, target_map_id).await;

    let payload = game_context::build_map_complementary_payload(sub_area_id, target_map_id as f64, &players, &npc_actors);
    session.send_raw(RawMessage {
        message_id: crate::constants::MAP_COMPLEMENTARY_MSG_ID,
        instance_id: 0,
        payload,
    }).await?;

    repository::update_character_position(&state.pool, character.id, target_map_id, dest_cell as i32, character.direction).await?;

    tracing::info!(character_id = character.id, from = current_map_id, to = target_map_id, cell = dest_cell, "Map transition");
    Ok(Some(target_map_id))
}
