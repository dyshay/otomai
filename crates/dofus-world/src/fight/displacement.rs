//! Forced displacement effects: push, pull, teleport, exchange, slide.

use super::state::{Fight, Element};
use dofus_common::pathfinding;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Push a target away from the source by N cells.
pub async fn apply_push(
    session: &mut Session,
    fight: &mut Fight,
    source_id: f64,
    target_id: f64,
    distance: i32,
) -> anyhow::Result<()> {
    let (source_cell, target_cell) = match (fight.get_fighter(source_id), fight.get_fighter(target_id)) {
        (Some(s), Some(t)) => (s.cell_id as u16, t.cell_id as u16),
        _ => return Ok(()),
    };

    let dir = match pathfinding::direction_between(source_cell, target_cell) {
        Some(d) => d,
        None => return Ok(()),
    };

    // Move target cell by cell in the push direction
    let mut current = target_cell;
    for _ in 0..distance {
        let neighbours = pathfinding::cell_neighbours(current);
        if let Some(&(next, _)) = neighbours.iter().find(|&&(_, d)| d == dir) {
            // Check walkability and no fighter on cell
            if next < 560 && fight.fighter_on_cell(next as i16).is_none() {
                current = next;
            } else {
                break; // Hit wall or fighter
            }
        } else {
            break; // Edge of map
        }
    }

    if current != target_cell {
        if let Some(target) = fight.get_fighter_mut(target_id) {
            target.cell_id = current as i16;
        }

        session
            .send(&GameActionFightSlideMessage {
                action_id: 5,
                source_id,
                target_id,
                start_cell_id: target_cell as i16,
                end_cell_id: current as i16,
            })
            .await?;
    }

    Ok(())
}

/// Pull a target toward the source by N cells.
pub async fn apply_pull(
    session: &mut Session,
    fight: &mut Fight,
    source_id: f64,
    target_id: f64,
    distance: i32,
) -> anyhow::Result<()> {
    let (source_cell, target_cell) = match (fight.get_fighter(source_id), fight.get_fighter(target_id)) {
        (Some(s), Some(t)) => (s.cell_id as u16, t.cell_id as u16),
        _ => return Ok(()),
    };

    // Direction from target toward source (reverse of push)
    let dir = match pathfinding::direction_between(target_cell, source_cell) {
        Some(d) => d,
        None => return Ok(()),
    };

    let mut current = target_cell;
    for _ in 0..distance {
        let neighbours = pathfinding::cell_neighbours(current);
        if let Some(&(next, _)) = neighbours.iter().find(|&&(_, d)| d == dir) {
            if next != source_cell as u16 && fight.fighter_on_cell(next as i16).is_none() {
                current = next;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    if current != target_cell {
        if let Some(target) = fight.get_fighter_mut(target_id) {
            target.cell_id = current as i16;
        }

        session
            .send(&GameActionFightSlideMessage {
                action_id: 6,
                source_id,
                target_id,
                start_cell_id: target_cell as i16,
                end_cell_id: current as i16,
            })
            .await?;
    }

    Ok(())
}

/// Teleport a fighter to a specific cell.
pub async fn apply_teleport(
    session: &mut Session,
    fight: &mut Fight,
    source_id: f64,
    target_id: f64,
    dest_cell: i16,
) -> anyhow::Result<()> {
    if fight.fighter_on_cell(dest_cell).is_some() {
        return Ok(()); // Cell occupied
    }

    if let Some(target) = fight.get_fighter_mut(target_id) {
        target.cell_id = dest_cell;
    }

    session
        .send(&GameActionFightTeleportOnSameMapMessage {
            action_id: 4,
            source_id,
            target_id,
            cell_id: dest_cell,
        })
        .await?;

    Ok(())
}

/// Exchange positions of two fighters.
pub async fn apply_exchange(
    session: &mut Session,
    fight: &mut Fight,
    source_id: f64,
    target_id: f64,
) -> anyhow::Result<()> {
    let (cell_a, cell_b) = match (fight.get_fighter(source_id), fight.get_fighter(target_id)) {
        (Some(a), Some(b)) => (a.cell_id, b.cell_id),
        _ => return Ok(()),
    };

    if let Some(a) = fight.get_fighter_mut(source_id) {
        a.cell_id = cell_b;
    }
    if let Some(b) = fight.get_fighter_mut(target_id) {
        b.cell_id = cell_a;
    }

    session
        .send(&GameActionFightExchangePositionsMessage {
            action_id: 8,
            source_id,
            target_id,
            caster_cell_id: cell_b,
            target_cell_id: cell_a,
        })
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_for_push() {
        // Adjacent cells should have a direction
        let dir = pathfinding::direction_between(300, 301);
        assert!(dir.is_some());
    }
}
