use super::state::Fight;
use super::{damage, turns};
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Handle a spell cast by the player during their turn.
pub async fn handle_spell_cast(
    session: &mut Session,
    fight: &mut Fight,
    player_id: f64,
    spell_id: i16,
    cell_id: i16,
) -> anyhow::Result<()> {
    // Verify it's the player's turn
    let current = match fight.current_fighter() {
        Some(f) if f.id == player_id && f.is_player => f.clone(),
        _ => return Ok(()),
    };

    // Check AP cost (simplified: all spells cost 3 AP for now)
    // TODO: Load from SpellLevels D2O
    let ap_cost = 3i16;
    if current.action_points < ap_cost {
        return Ok(());
    }

    // Deduct AP
    if let Some(f) = fight.current_fighter_mut() {
        f.action_points -= ap_cost;
    }

    // Send AP variation
    session
        .send(&SequenceStartMessage {
            sequence_type: 1, // SPELL
            author_id: player_id,
        })
        .await?;

    // Send spell cast message
    session
        .send(&GameActionFightSpellCastMessage {
            action_id: 300, // ACTION_FIGHT_CAST_SPELL
            source_id: player_id,
            silent_cast: false,
            verbose_cast: true,
            target_id: 0.0,
            destination_cell_id: cell_id,
            critical: 0, // NORMAL
            spell_id,
            spell_level: 1,
            portals_ids: vec![],
        })
        .await?;

    // Find target on the cell
    let target = fight
        .fighters
        .iter()
        .find(|f| f.cell_id == cell_id && f.is_alive && f.id != player_id)
        .cloned();

    if let Some(target) = target {
        // Calculate and apply damage (simplified: 10-20 base damage per spell)
        let base_damage = 10 + (spell_id as i32 % 10);
        damage::apply_damage(session, fight, player_id, target.id, base_damage, 0).await?;
    }

    // AP variation message
    session
        .send(&GameActionFightPointsVariationMessage {
            action_id: 168, // ACTION_CHARACTER_ACTION_POINTS_USE
            source_id: player_id,
            target_id: player_id,
            delta: -ap_cost,
        })
        .await?;

    session
        .send(&SequenceEndMessage {
            action_id: 300,
            author_id: player_id,
            sequence_type: 1,
        })
        .await?;

    Ok(())
}
