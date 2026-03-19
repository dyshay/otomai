use super::state::{Fight, FightPhase, Team};
use super::damage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

const TURN_TIME_MS: i32 = 30000; // 30 seconds per turn

/// Start the next fighter's turn.
pub fn start_next_turn<'a>(session: &'a mut Session, fight: &'a mut Fight) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>> {
    Box::pin(async move { start_next_turn_inner(session, fight).await })
}

async fn start_next_turn_inner(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    if fight.should_end() || fight.phase == FightPhase::Ended {
        return Ok(());
    }

    let fighter = match fight.current_fighter() {
        Some(f) => f.clone(),
        None => return Ok(()),
    };

    // Reset AP/MP for this turn
    if let Some(f) = fight.current_fighter_mut() {
        f.action_points = f.max_action_points;
        f.movement_points = f.max_movement_points;
    }

    // GameFightTurnStartMessage
    session
        .send(&GameFightTurnStartMessage {
            id: fighter.id,
            wait_time: TURN_TIME_MS,
        })
        .await?;

    // If it's a monster's turn, auto-play (simple AI: attack nearest player)
    if !fighter.is_player {
        monster_auto_play(session, fight).await?;
    }

    Ok(())
}

/// Simple monster AI: find nearest player and attack.
async fn monster_auto_play(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    let monster = match fight.current_fighter() {
        Some(f) if !f.is_player => f.clone(),
        _ => return Ok(()),
    };

    // Find a living player to attack
    let target = fight
        .fighters
        .iter()
        .find(|f| f.is_player && f.is_alive)
        .cloned();

    if let Some(target) = target {
        // Simple attack: deal damage based on monster level
        let base_damage = 5 + monster.level as i32;
        damage::apply_damage(session, fight, monster.id, target.id, base_damage, 0).await?;
    }

    // End monster's turn
    end_turn(session, fight).await?;

    Ok(())
}

/// End the current turn and advance to next fighter.
pub async fn end_turn(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    let fighter_id = fight
        .current_fighter()
        .map(|f| f.id)
        .unwrap_or(0.0);

    session
        .send(&GameFightTurnEndMessage { id: fighter_id })
        .await?;

    // Check if fight should end
    if fight.should_end() {
        fight.phase = FightPhase::Ended;
        return Ok(());
    }

    // Advance to next fighter
    let new_round = fight.advance_turn();

    if new_round {
        session
            .send(&GameFightNewRoundMessage {
                round_number: fight.round,
            })
            .await?;
    }

    // Start next turn
    start_next_turn(session, fight).await?;

    Ok(())
}

/// Handle player ending their turn (GameFightTurnFinishMessage).
pub async fn handle_turn_finish(session: &mut Session, fight: &mut Fight) -> anyhow::Result<()> {
    // Verify it's the player's turn
    if let Some(f) = fight.current_fighter() {
        if !f.is_player {
            return Ok(());
        }
    }

    end_turn(session, fight).await
}
