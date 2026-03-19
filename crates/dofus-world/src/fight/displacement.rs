//! Forced displacement effects: push, pull, teleport, exchange.
//! Includes collision damage when pushed into wall/fighter.

use super::damage;
use super::state::{Element, Fight};
use dofus_common::pathfinding;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Collision damage per remaining cell of push (Dofus formula).
/// damage = (push_level + caster_push_bonus) * remaining_cells / 2
const BASE_COLLISION_DAMAGE_PER_CELL: i32 = 8;

/// Push result: how far the target actually moved.
struct PushResult {
    start_cell: u16,
    end_cell: u16,
    cells_moved: i32,
    cells_remaining: i32, // Cells that couldn't be traversed (collision)
    collided_with_fighter: Option<f64>, // Fighter ID hit during collision
}

/// Push a target away from the source by N cells.
/// Returns collision info for damage calculation.
pub async fn apply_push(
    session: &mut Session,
    fight: &mut Fight,
    source_id: f64,
    target_id: f64,
    distance: i32,
) -> anyhow::Result<()> {
    // Check gravity state (can't be pushed)
    if fight.get_fighter(target_id).map(|f| f.states.has_gravity()).unwrap_or(false) {
        return Ok(());
    }

    let (source_cell, target_cell) = match (fight.get_fighter(source_id), fight.get_fighter(target_id)) {
        (Some(s), Some(t)) => (s.cell_id as u16, t.cell_id as u16),
        _ => return Ok(()),
    };

    let dir = match pathfinding::direction_between(source_cell, target_cell) {
        Some(d) => d,
        None => return Ok(()),
    };

    let result = compute_push(fight, target_cell, dir, distance);

    // Apply movement
    if result.end_cell != target_cell {
        if let Some(target) = fight.get_fighter_mut(target_id) {
            target.cell_id = result.end_cell as i16;
        }

        session
            .send(&GameActionFightSlideMessage {
                action_id: 5,
                source_id,
                target_id,
                start_cell_id: target_cell as i16,
                end_cell_id: result.end_cell as i16,
            })
            .await?;
    }

    // Apply collision damage if push was blocked
    if result.cells_remaining > 0 {
        let collision_dmg = BASE_COLLISION_DAMAGE_PER_CELL * result.cells_remaining;

        // Damage to pushed target
        damage::apply_damage(
            session, fight, source_id, target_id,
            collision_dmg, Element::Neutral,
        ).await?;

        // If collided with another fighter, they take half collision damage
        if let Some(collided_id) = result.collided_with_fighter {
            let collided_dmg = collision_dmg / 2;
            damage::apply_damage(
                session, fight, source_id, collided_id,
                collided_dmg, Element::Neutral,
            ).await?;
        }
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

    let dir = match pathfinding::direction_between(target_cell, source_cell) {
        Some(d) => d,
        None => return Ok(()),
    };

    let mut current = target_cell;
    for _ in 0..distance {
        let neighbours = pathfinding::cell_neighbours(current);
        if let Some(&(next, _)) = neighbours.iter().find(|&&(_, d)| d == dir) {
            if next != source_cell && fight.fighter_on_cell(next as i16).is_none() {
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
        return Ok(());
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

    if let Some(a) = fight.get_fighter_mut(source_id) { a.cell_id = cell_b; }
    if let Some(b) = fight.get_fighter_mut(target_id) { b.cell_id = cell_a; }

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

/// Compute push trajectory and detect collisions.
fn compute_push(fight: &Fight, start: u16, dir: u8, distance: i32) -> PushResult {
    let mut current = start;
    let mut moved = 0;
    let mut collided_with = None;

    for _ in 0..distance {
        let neighbours = pathfinding::cell_neighbours(current);
        if let Some(&(next, _)) = neighbours.iter().find(|&&(_, d)| d == dir) {
            // Check for fighter on next cell
            if let Some(blocker) = fight.fighter_on_cell(next as i16) {
                collided_with = Some(blocker.id);
                break;
            }
            // Check walkability (cell exists and is valid)
            if next >= 560 {
                break;
            }
            current = next;
            moved += 1;
        } else {
            break; // Edge of map = wall collision
        }
    }

    PushResult {
        start_cell: start,
        end_cell: current,
        cells_moved: moved,
        cells_remaining: distance - moved,
        collided_with_fighter: collided_with,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collision_damage_calculation() {
        // 3 cells remaining = 3 * 8 = 24 collision damage
        let dmg = BASE_COLLISION_DAMAGE_PER_CELL * 3;
        assert_eq!(dmg, 24);
    }

    #[test]
    fn push_result_no_collision() {
        let fight = Fight::new(1, 100);
        let result = compute_push(&fight, 300, 0, 3); // Push east 3 cells
        // On empty fight, no fighters to collide with
        assert!(result.collided_with_fighter.is_none());
    }
}
