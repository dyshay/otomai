//! Movement system: map movement, timing validation, map transitions.

pub mod timing;
pub mod transition;

pub use timing::MovementState;
pub use transition::handle_change_map;

use crate::WorldState;
use dofus_common::pathfinding;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use std::time::Instant;

pub async fn handle_movement_request(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    msg: &GameMapMovementRequestMessage,
) -> anyhow::Result<Option<MovementState>> {
    if msg.map_id as i64 != current_map_id {
        tracing::warn!(character_id, "Movement request map_id mismatch");
        return Ok(None);
    }
    if msg.key_movements.is_empty() { return Ok(None); }

    let decoded = timing::decode_path(&msg.key_movements);
    let dest_cell = decoded.last().map(|&(c, _)| c).unwrap_or(0);
    let path_cells: Vec<u16> = decoded.iter().map(|&(c, _)| c).collect();

    if let Some(map_data) = state.maps.get(current_map_id) {
        if !pathfinding::validate_path(&map_data, &path_cells) {
            tracing::warn!(character_id, map_id = current_map_id, "Invalid movement path");
            session.send(&GameMapNoMovementMessage { cell_x: 0, cell_y: 0 }).await?;
            return Ok(None);
        }
    }

    let expected_duration_ms = timing::expected_path_duration_ms(&msg.key_movements);

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
    for player in &players { let _ = player.tx.send(raw.clone()); }

    state.world.update_player_cell(current_map_id, character_id, dest_cell as i16).await;

    Ok(Some(MovementState {
        start_time: Instant::now(),
        path_cells,
        expected_duration_ms,
        dest_cell,
    }))
}

pub async fn handle_movement_confirm(
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    movement_state: Option<&MovementState>,
) -> anyhow::Result<bool> {
    if let Some(ms) = movement_state {
        let elapsed_ms = ms.start_time.elapsed().as_millis() as u64;
        let min_expected = (ms.expected_duration_ms as f64 * timing::TIMING_TOLERANCE) as u64;

        if elapsed_ms < min_expected && ms.expected_duration_ms > 0 {
            tracing::warn!(character_id, elapsed_ms, expected_ms = ms.expected_duration_ms, "Movement too fast");
            return Ok(false);
        }
    }

    let players = state.world.get_players_on_map(current_map_id).await;
    if let Some(player) = players.iter().find(|p| p.character_id == character_id) {
        repository::update_character_position(
            &state.pool, character_id, current_map_id, player.cell_id as i32, player.direction as i32,
        ).await?;
    }
    Ok(true)
}

pub async fn handle_movement_cancel(
    state: &Arc<WorldState>,
    character_id: i64,
    current_map_id: i64,
    cell_id: i16,
) -> anyhow::Result<()> {
    state.world.update_player_cell(current_map_id, character_id, cell_id).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use dofus_common::dlm::{self, MapDirection, MAP_CELLS_COUNT, MAP_WIDTH};

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
                assert!(diff <= MAP_WIDTH as u32);
            }
        }
    }

    #[test]
    fn border_cells_are_correct() {
        for i in 0..28u16 { assert_eq!(dlm::cell_border(i), Some(MapDirection::Top)); }
        for i in 532..560u16 { assert_eq!(dlm::cell_border(i), Some(MapDirection::Bottom)); }
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
