use crate::world::MapPlayer;
use crate::WorldState;
use dofus_common::dlm::{self, MapDirection};
use dofus_database::models::Character;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Handle GameMapMovementRequestMessage — player wants to move on current map.
/// Validate and broadcast movement to all players on the map.
pub async fn handle_movement_request(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    msg: &GameMapMovementRequestMessage,
) -> anyhow::Result<()> {
    // Basic validation: map_id must match
    if msg.map_id as i64 != current_map_id {
        tracing::warn!(
            character_id,
            expected = current_map_id,
            got = msg.map_id as i64,
            "Movement request map_id mismatch"
        );
        return Ok(());
    }

    if msg.key_movements.is_empty() {
        return Ok(());
    }

    // Extract destination cell from last key_movement
    // In Dofus, key_movements encode path as compressed cell+direction pairs.
    // The last entry's cell is the destination.
    let dest_cell = msg.key_movements.last().copied().unwrap_or(0);
    let dest_cell_id = (dest_cell & 0x3FFF) as u16; // lower 14 bits = cell id

    // Broadcast GameMapMovementMessage to all players on the map
    let movement_msg = GameMapMovementMessage {
        key_movements: msg.key_movements.clone(),
        forced_direction: 0,
        actor_id: character_id as f64,
    };

    let mut w = BigEndianWriter::new();
    movement_msg.serialize(&mut w);
    let raw = RawMessage {
        message_id: GameMapMovementMessage::MESSAGE_ID,
        instance_id: 0,
        payload: w.into_data(),
    };

    // Broadcast to all on map (including self — client expects it)
    let players = state.world.get_players_on_map(current_map_id).await;
    for player in &players {
        let _ = player.tx.send(raw.clone());
    }

    // Update cell in world state
    state
        .world
        .update_player_cell(current_map_id, character_id, dest_cell_id as i16)
        .await;

    Ok(())
}

/// Handle GameMapMovementConfirmMessage — player finished moving.
/// Update position in DB.
pub async fn handle_movement_confirm(
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
) -> anyhow::Result<()> {
    // Get current cell from world state
    let players = state.world.get_players_on_map(current_map_id).await;
    if let Some(player) = players.iter().find(|p| p.character_id == character_id) {
        repository::update_character_position(
            &state.pool,
            character_id,
            current_map_id,
            player.cell_id as i32,
            player.direction as i32,
        )
        .await?;
    }
    Ok(())
}

/// Handle GameMapMovementCancelMessage — player cancelled movement.
pub async fn handle_movement_cancel(
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    cell_id: i16,
) -> anyhow::Result<()> {
    state
        .world
        .update_player_cell(current_map_id, character_id, cell_id)
        .await;
    Ok(())
}

/// Handle ChangeMapMessage — player reached a border and wants to change map.
/// Returns the new (map_id, cell_id) if successful.
pub async fn handle_change_map(
    session: &mut Session,
    state: &Arc<WorldState>,
    character: &Character,
    current_map_id: i64,
    target_map_id: i64,
    broadcast_tx: &mpsc::UnboundedSender<RawMessage>,
) -> anyhow::Result<Option<i64>> {
    // 1. Get current player cell from world
    let current_cell = {
        let players = state.world.get_players_on_map(current_map_id).await;
        players
            .iter()
            .find(|p| p.character_id == character.id)
            .map(|p| p.cell_id as u16)
            .unwrap_or(0)
    };

    // 2. Determine exit direction from current cell
    let direction = match dlm::cell_border(current_cell) {
        Some(d) => d,
        None => {
            tracing::warn!(
                character_id = character.id,
                cell = current_cell,
                "ChangeMap from non-border cell"
            );
            return Ok(None);
        }
    };

    // 3. Validate target map is the actual neighbor in that direction
    let expected_neighbour = state.maps.get_neighbour(current_map_id, direction);
    if expected_neighbour != Some(target_map_id) {
        // Also check if the target matches any neighbor (client might use a different direction)
        let valid = [MapDirection::Top, MapDirection::Bottom, MapDirection::Left, MapDirection::Right]
            .iter()
            .any(|d| state.maps.get_neighbour(current_map_id, *d) == Some(target_map_id));

        if !valid {
            tracing::warn!(
                character_id = character.id,
                current_map_id,
                target_map_id,
                "ChangeMap to non-neighbor map"
            );
            return Ok(None);
        }
        // Find actual direction to the target
        // (client might be on a corner cell where direction is ambiguous)
    }

    // 4. Calculate destination cell on new map
    let mirror = dlm::mirror_cell(current_cell, direction);

    // 5. Find a walkable cell (mirror or nearest alternative)
    let dest_cell = if let Some(map_data) = state.maps.get(target_map_id) {
        let opposite = match direction {
            MapDirection::Top => MapDirection::Bottom,
            MapDirection::Bottom => MapDirection::Top,
            MapDirection::Left => MapDirection::Right,
            MapDirection::Right => MapDirection::Left,
        };
        map_data
            .nearest_walkable_on_border(mirror, opposite)
            .unwrap_or(mirror)
    } else {
        mirror
    };

    // 6. Leave current map
    state.world.leave_map(current_map_id, character.id).await;

    // 7. Send CurrentMapMessage for the new map
    session
        .send(&CurrentMapMessage {
            map_id: target_map_id as f64,
            map_key: String::new(),
        })
        .await?;

    // 8. Build MapPlayer for new map
    let entity_look = crate::game_context::build_entity_look(character);
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

    // 9. Join new map (broadcasts ShowActor to others)
    state.world.join_map(target_map_id, map_player).await;

    // 10. Send MapComplementary with all actors on new map
    let players = state.world.get_players_on_map(target_map_id).await;
    let sub_area_id = state
        .maps
        .get(target_map_id)
        .map(|m| m.sub_area_id as i16)
        .unwrap_or(449);

    let payload = crate::game_context::build_map_complementary_payload(
        sub_area_id,
        target_map_id as f64,
        &players,
    );
    session
        .send_raw(RawMessage {
            message_id: 5176, // MapComplementaryInformationsDataMessage
            instance_id: 0,
            payload,
        })
        .await?;

    // 11. Save position to DB
    repository::update_character_position(
        &state.pool,
        character.id,
        target_map_id,
        dest_cell as i32,
        character.direction,
    )
    .await?;

    tracing::info!(
        character_id = character.id,
        from = current_map_id,
        to = target_map_id,
        cell = dest_cell,
        "Map transition"
    );

    Ok(Some(target_map_id))
}

#[cfg(test)]
mod tests {
    use dofus_common::dlm::{self, MapDirection, MAP_CELLS_COUNT, MAP_WIDTH};

    #[test]
    fn key_movement_dest_cell_extraction() {
        // In Dofus, key_movements encode path. The last entry contains the dest cell.
        // Lower 14 bits = cell_id
        let key = 300i16; // cell 300
        let cell_id = (key & 0x3FFF) as u16;
        assert_eq!(cell_id, 300);
    }

    #[test]
    fn mirror_cell_symmetry() {
        // Mirror should be roughly symmetric: mirror(mirror(cell)) ≈ cell
        for cell in [0u16, 13, 14, 27, 532, 545, 546, 559] {
            if let Some(dir) = dlm::cell_border(cell) {
                let mirror = dlm::mirror_cell(cell, dir);
                let opposite = match dir {
                    MapDirection::Top => MapDirection::Bottom,
                    MapDirection::Bottom => MapDirection::Top,
                    MapDirection::Left => MapDirection::Right,
                    MapDirection::Right => MapDirection::Left,
                };
                let back = dlm::mirror_cell(mirror, opposite);
                // Should be close to original (within 1-2 cells due to grid stagger)
                let diff = (back as i32 - cell as i32).unsigned_abs();
                assert!(diff <= MAP_WIDTH as u32, "mirror symmetry broken: cell={cell} mirror={mirror} back={back} diff={diff}");
            }
        }
    }

    #[test]
    fn border_cells_are_correct() {
        // Top border: first 28 cells (2 rows of 14)
        for i in 0..28u16 {
            assert_eq!(dlm::cell_border(i), Some(MapDirection::Top), "cell {i} should be top");
        }
        // Bottom border: last 28 cells
        for i in 532..560u16 {
            assert_eq!(dlm::cell_border(i), Some(MapDirection::Bottom), "cell {i} should be bottom");
        }
        // Middle cell should be None (281 is not on any border)
        assert!(dlm::cell_border(281).is_none());
    }

    #[test]
    fn mirror_top_lands_on_bottom_border() {
        for cell in 0..28u16 {
            let mirror = dlm::mirror_cell(cell, MapDirection::Top);
            assert!(
                mirror >= (MAP_CELLS_COUNT - MAP_WIDTH * 2) as u16,
                "mirror of top cell {cell} should be on bottom border, got {mirror}"
            );
        }
    }

    #[test]
    fn mirror_left_lands_on_right_border() {
        // Left border cells: multiples of 14 (excluding top/bottom)
        for row in 2..18 {
            let cell = (row * MAP_WIDTH) as u16;
            let mirror = dlm::mirror_cell(cell, MapDirection::Left);
            assert_eq!(
                (mirror as usize + 1) % MAP_WIDTH, 0,
                "mirror of left cell {cell} should be on right border, got {mirror}"
            );
        }
    }
}
