use super::damage;
use super::marks;
use super::state::{Element, Fight, FightPhase, Team};
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

const TURN_TIME_CENTISECONDS: i32 = 3000; // 30 seconds

/// Start the next fighter's turn (boxed for async recursion).
pub fn start_next_turn<'a>(
    session: &'a mut Session,
    fight: &'a mut Fight,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
    Box::pin(start_next_turn_inner(session, fight))
}

async fn start_next_turn_inner(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    if fight.should_end() || fight.phase == FightPhase::Ended {
        return Ok(());
    }

    // Tick buffs (decrement durations, remove expired)
    if let Some(f) = fight.current_fighter_mut() {
        f.buffs.tick();
    }

    // Apply poison damage at turn start
    let poisons: Vec<(f64, i32, Element)> = fight
        .current_fighter()
        .map(|f| {
            f.buffs.active_poisons()
                .iter()
                .map(|b| {
                    let elem = match b.effect_type {
                        super::effects::EffectType::Poison(e) => e,
                        _ => Element::Neutral,
                    };
                    (b.source_id, b.value, elem)
                })
                .collect()
        })
        .unwrap_or_default();

    let fighter_id = fight.current_fighter().map(|f| f.id).unwrap_or(0.0);
    for (source_id, poison_dmg, elem) in &poisons {
        damage::apply_damage(session, fight, *source_id, fighter_id, *poison_dmg, *elem).await?;
    }

    if fight.should_end() {
        fight.phase = FightPhase::Ended;
        return Ok(());
    }

    // Tick marks (decrement durations)
    fight.marks.tick();

    // Trigger glyphs for this fighter
    let fighter_id_for_glyphs = fight.current_fighter().map(|f| f.id).unwrap_or(0.0);
    marks::trigger_glyphs_for_turn(session, fight, fighter_id_for_glyphs).await?;

    if fight.should_end() {
        fight.phase = FightPhase::Ended;
        return Ok(());
    }

    // Reset AP/MP for current fighter (with buff bonuses)
    if let Some(f) = fight.current_fighter_mut() {
        f.reset_turn();
    }

    let fighter = match fight.current_fighter() {
        Some(f) => f.clone(),
        None => return Ok(()),
    };

    // GameFightTurnStartMessage (wait_time in centiseconds)
    session
        .send(&GameFightTurnStartMessage {
            id: fighter.id,
            wait_time: TURN_TIME_CENTISECONDS,
        })
        .await?;

    // If monster's turn, auto-play
    if !fighter.is_player {
        monster_auto_play(session, fight).await?;
    }

    Ok(())
}

/// Monster AI: move toward nearest player, then attack.
async fn monster_auto_play(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    let monster = match fight.current_fighter() {
        Some(f) if !f.is_player => f.clone(),
        _ => return end_turn(session, fight).await,
    };

    // Find nearest alive player
    let target = fight
        .fighters
        .iter()
        .filter(|f| f.is_player && f.is_alive)
        .min_by_key(|f| {
            dofus_common::pathfinding::distance(
                monster.cell_id as u16,
                f.cell_id as u16,
            )
        })
        .cloned();

    if let Some(target) = target {
        let dist = dofus_common::pathfinding::distance(
            monster.cell_id as u16,
            target.cell_id as u16,
        );

        // Move toward target if too far (range > 1)
        if dist > 1 && monster.movement_points > 0 {
            // Simplified: just move closer without full pathfinding
            // Deduct MP
            if let Some(f) = fight.current_fighter_mut() {
                let mp_used = (dist as i16 - 1).min(f.movement_points);
                f.movement_points -= mp_used;
            }
        }

        // Attack with base damage
        let base_damage = (5 + monster.level as i32).max(1);

        session
            .send(&SequenceStartMessage {
                sequence_type: 1,
                author_id: monster.id,
            })
            .await?;

        damage::apply_damage(
            session, fight, monster.id, target.id,
            base_damage, Element::Neutral,
        ).await?;

        session
            .send(&SequenceEndMessage {
                action_id: 300,
                author_id: monster.id,
                sequence_type: 1,
            })
            .await?;
    }

    end_turn(session, fight).await
}

/// End the current turn.
pub async fn end_turn(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    let fighter_id = fight.current_fighter().map(|f| f.id).unwrap_or(0.0);

    // GameFightTurnEndMessage
    session
        .send(&GameFightTurnEndMessage { id: fighter_id })
        .await?;

    if fight.should_end() {
        fight.phase = FightPhase::Ended;
        return Ok(());
    }

    let new_round = fight.advance_turn();

    if new_round {
        session
            .send(&GameFightNewRoundMessage {
                round_number: fight.round,
            })
            .await?;
    }

    start_next_turn(session, fight).await
}

/// Handle player ending their turn.
pub async fn handle_turn_finish(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    if let Some(f) = fight.current_fighter() {
        if !f.is_player {
            return Ok(());
        }
    }
    end_turn(session, fight).await
}

/// Handle movement request during combat.
pub async fn handle_fight_movement(
    session: &mut Session,
    fight: &mut Fight,
    player_id: f64,
    key_movements: &[i16],
) -> anyhow::Result<()> {
    let current = match fight.current_fighter() {
        Some(f) if f.id == player_id && f.is_player => f.clone(),
        _ => return Ok(()),
    };

    if key_movements.is_empty() {
        return Ok(());
    }

    // MP cost = number of cells moved (steps - 1, since first key is start cell)
    let mp_cost = (key_movements.len() as i16 - 1).max(0);
    if mp_cost > current.movement_points {
        return Ok(());
    }

    // Destination cell
    let dest_cell = key_movements.last().map(|&k| (k & 0x0FFF) as i16).unwrap_or(current.cell_id);

    // Deduct MP
    if let Some(f) = fight.current_fighter_mut() {
        f.movement_points -= mp_cost;
        f.cell_id = dest_cell;
    }

    // Broadcast movement
    session
        .send(&SequenceStartMessage {
            sequence_type: 2, // MOVEMENT
            author_id: player_id,
        })
        .await?;

    session
        .send(&GameMapMovementMessage {
            key_movements: key_movements.to_vec(),
            forced_direction: 0,
            actor_id: player_id,
        })
        .await?;

    // MP variation
    session
        .send(&GameActionFightPointsVariationMessage {
            action_id: 129, // ACTION_CHARACTER_MOVEMENT_POINTS_USE
            source_id: player_id,
            target_id: player_id,
            delta: -mp_cost,
        })
        .await?;

    session
        .send(&SequenceEndMessage {
            action_id: 129,
            author_id: player_id,
            sequence_type: 2,
        })
        .await?;

    // Check traps on destination cell
    marks::trigger_traps_on_cell(session, fight, dest_cell, player_id).await?;

    Ok(())
}
