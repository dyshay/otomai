use crate::world::MapPlayer;
use crate::WorldState;
use dofus_common::dlm::{self, MapDirection};
use dofus_common::pathfinding;
use dofus_database::models::Character;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

// ─── Movement timing constants (from AnimatedMovementBehavior.as) ──

/// Time per cell in milliseconds for running (fastest legitimate speed).
/// Directions: 0=E, 1=SE, 2=S, 3=SW, 4=W, 5=NW, 6=N, 7=NE
const RUN_MS_PER_CELL: [u64; 8] = [
    255, // 0 E  (horizontal diagonal)
    170, // 1 SE (linear)
    150, // 2 S  (vertical diagonal)
    170, // 3 SW (linear)
    255, // 4 W  (horizontal diagonal)
    170, // 5 NW (linear)
    150, // 6 N  (vertical diagonal)
    170, // 7 NE (linear)
];

/// Tolerance factor — allow 20% faster than theoretical minimum.
/// Accounts for network latency, frame timing, client rounding.
const TIMING_TOLERANCE: f64 = 0.8;

/// Extract cell ID from a key_movement entry.
/// Lower 12 bits = cell_id (0-4095, only 0-559 used).
fn cell_from_key(key: i16) -> u16 {
    (key as u16) & 0x0FFF
}

/// Extract direction from a key_movement entry.
/// Bits 12-14 encode direction (0-7).
fn direction_from_key(key: i16) -> u8 {
    ((key as u16 >> 12) & 0x07) as u8
}

/// Decode client key_movements into a list of (cell_id, direction) pairs.
fn decode_path(key_movements: &[i16]) -> Vec<(u16, u8)> {
    key_movements
        .iter()
        .map(|&k| (cell_from_key(k), direction_from_key(k)))
        .collect()
}

/// Calculate the minimum expected duration of a path in milliseconds.
/// Uses running speed (fastest legitimate).
fn expected_path_duration_ms(key_movements: &[i16]) -> u64 {
    if key_movements.len() <= 1 {
        return 0;
    }

    let mut total_ms = 0u64;
    for &key in &key_movements[1..] {
        // Each step after the first contributes time
        let dir = direction_from_key(key) as usize;
        total_ms += RUN_MS_PER_CELL[dir.min(7)];
    }
    total_ms
}

/// State tracked during an active movement.
pub struct MovementState {
    pub start_time: Instant,
    pub path_cells: Vec<u16>,
    pub expected_duration_ms: u64,
    pub dest_cell: u16,
}

/// Handle GameMapMovementRequestMessage — player wants to move on current map.
/// Validates the path and broadcasts movement to all players.
/// Returns MovementState for timing validation on confirm.
pub async fn handle_movement_request(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    msg: &GameMapMovementRequestMessage,
) -> anyhow::Result<Option<MovementState>> {
    // Basic validation: map_id must match
    if msg.map_id as i64 != current_map_id {
        tracing::warn!(character_id, "Movement request map_id mismatch");
        return Ok(None);
    }

    if msg.key_movements.is_empty() {
        return Ok(None);
    }

    // Decode the path
    let decoded = decode_path(&msg.key_movements);
    let dest_cell = decoded.last().map(|&(c, _)| c).unwrap_or(0);
    let path_cells: Vec<u16> = decoded.iter().map(|&(c, _)| c).collect();

    // Validate path walkability if we have map data
    if let Some(map_data) = state.maps.get(current_map_id) {
        if !pathfinding::validate_path(&map_data, &path_cells) {
            tracing::warn!(
                character_id,
                map_id = current_map_id,
                "Invalid movement path (unwalkable or non-adjacent cells)"
            );
            // Send no-movement to reject
            session
                .send(&GameMapNoMovementMessage {
                    cell_x: 0,
                    cell_y: 0,
                })
                .await?;
            return Ok(None);
        }
    }

    // Calculate expected duration
    let expected_duration_ms = expected_path_duration_ms(&msg.key_movements);

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

    let players = state.world.get_players_on_map(current_map_id).await;
    for player in &players {
        let _ = player.tx.send(raw.clone());
    }

    // Update cell in world state to destination
    state
        .world
        .update_player_cell(current_map_id, character_id, dest_cell as i16)
        .await;

    Ok(Some(MovementState {
        start_time: Instant::now(),
        path_cells,
        expected_duration_ms,
        dest_cell,
    }))
}

/// Handle GameMapMovementConfirmMessage — player finished moving.
/// Validates timing and saves position to DB.
/// Returns true if the movement was valid.
pub async fn handle_movement_confirm(
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    movement_state: Option<&MovementState>,
) -> anyhow::Result<bool> {
    // Timing validation
    if let Some(ms) = movement_state {
        let elapsed_ms = ms.start_time.elapsed().as_millis() as u64;
        let min_expected = (ms.expected_duration_ms as f64 * TIMING_TOLERANCE) as u64;

        if elapsed_ms < min_expected && ms.expected_duration_ms > 0 {
            tracing::warn!(
                character_id,
                elapsed_ms,
                expected_ms = ms.expected_duration_ms,
                min_ms = min_expected,
                path_len = ms.path_cells.len(),
                "Movement too fast (possible speedhack)"
            );
            // Don't save position — player is cheating
            return Ok(false);
        }
    }

    // Save position to DB
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
    Ok(true)
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
/// Returns the new map_id if successful.
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

    // 3. Validate target map is a neighbor
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

    // 7. Send CurrentMapMessage
    session
        .send(&CurrentMapMessage {
            map_id: target_map_id as f64,
            map_key: crate::constants::MAP_ENCRYPTION_KEY.to_string(),
        })
        .await?;

    // 8. Build MapPlayer and join new map
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

    state.world.join_map(target_map_id, map_player).await;

    // 9. Send MapComplementary
    let players = state.world.get_players_on_map(target_map_id).await;
    let sub_area_id = state
        .maps
        .get(target_map_id)
        .map(|m| m.sub_area_id as i16)
        .unwrap_or(449);

    let npc_actors = crate::game_context::build_npc_actors_for_map(
        state, character.id, target_map_id,
    ).await;
    let payload = crate::game_context::build_map_complementary_payload(
        sub_area_id,
        target_map_id as f64,
        &players,
        &npc_actors,
    );
    session
        .send_raw(RawMessage {
            message_id: 5176,
            instance_id: 0,
            payload,
        })
        .await?;

    // 10. Save position to DB
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
    use super::*;
    use dofus_common::dlm::{self, MapDirection, MAP_CELLS_COUNT, MAP_WIDTH};

    #[test]
    fn decode_key_movement() {
        // Direction 1 (SE), cell 300: (1 << 12) | 300 = 4096 + 300 = 4396
        let key = ((1u16 << 12) | 300u16) as i16;
        assert_eq!(cell_from_key(key), 300);
        assert_eq!(direction_from_key(key), 1);
    }

    #[test]
    fn decode_all_directions() {
        for dir in 0u8..8 {
            let cell = 200u16;
            let key = (((dir as u16) << 12) | cell) as i16;
            assert_eq!(direction_from_key(key), dir, "direction {dir}");
            assert_eq!(cell_from_key(key), cell, "cell for direction {dir}");
        }
    }

    #[test]
    fn path_duration_empty() {
        assert_eq!(expected_path_duration_ms(&[]), 0);
        assert_eq!(expected_path_duration_ms(&[300]), 0); // single cell = no movement
    }

    #[test]
    fn path_duration_linear_run() {
        // 5 cells moving SE (direction 1): 4 steps × 170ms = 680ms
        let keys: Vec<i16> = (0..5)
            .map(|i| ((1u16 << 12) | (100 + i)) as i16)
            .collect();
        let duration = expected_path_duration_ms(&keys);
        assert_eq!(duration, 4 * 170); // 680ms
    }

    #[test]
    fn path_duration_mixed_directions() {
        // 3 steps: SE (170ms), E (255ms), N (150ms) = 575ms
        let keys = vec![
            ((1u16 << 12) | 100) as i16, // start (SE)
            ((1u16 << 12) | 101) as i16, // SE: 170ms
            ((0u16 << 12) | 102) as i16, // E: 255ms
            ((6u16 << 12) | 103) as i16, // N: 150ms
        ];
        let duration = expected_path_duration_ms(&keys);
        assert_eq!(duration, 170 + 255 + 150);
    }

    #[test]
    fn timing_tolerance_allows_slight_fast() {
        // 10 cells SE: expected = 9 * 170 = 1530ms
        // With 0.8 tolerance: min = 1224ms
        let expected = 9 * 170u64;
        let min_expected = (expected as f64 * TIMING_TOLERANCE) as u64;
        assert_eq!(min_expected, 1224);
        // 1300ms elapsed > 1224ms → OK
        assert!(1300 >= min_expected);
        // 1000ms elapsed < 1224ms → SPEEDHACK
        assert!(1000 < min_expected);
    }

    #[test]
    fn mirror_cell_symmetry() {
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
                let diff = (back as i32 - cell as i32).unsigned_abs();
                assert!(diff <= MAP_WIDTH as u32, "mirror symmetry broken: cell={cell}");
            }
        }
    }

    #[test]
    fn border_cells_are_correct() {
        for i in 0..28u16 {
            assert_eq!(dlm::cell_border(i), Some(MapDirection::Top));
        }
        for i in 532..560u16 {
            assert_eq!(dlm::cell_border(i), Some(MapDirection::Bottom));
        }
        assert!(dlm::cell_border(281).is_none());
    }

    #[test]
    fn mirror_top_lands_on_bottom_border() {
        for cell in 0..28u16 {
            let mirror = dlm::mirror_cell(cell, MapDirection::Top);
            assert!(mirror >= (MAP_CELLS_COUNT - MAP_WIDTH * 2) as u16);
        }
    }

    #[test]
    fn mirror_left_lands_on_right_border() {
        for row in 2..18 {
            let cell = (row * MAP_WIDTH) as u16;
            let mirror = dlm::mirror_cell(cell, MapDirection::Left);
            assert_eq!((mirror as usize + 1) % MAP_WIDTH, 0);
        }
    }
}
