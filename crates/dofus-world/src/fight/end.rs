use crate::game_context;
use crate::WorldState;
use dofus_database::models::Character;
use dofus_io::{BigEndianWriter, DofusMessage};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use super::state::Fight;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Handle fight end: send results, XP, level up, return to roleplay.
pub async fn handle_fight_end(
    session: &mut Session,
    state: &Arc<WorldState>,
    fight: &Fight,
    character: &Character,
    broadcast_tx: &mpsc::UnboundedSender<RawMessage>,
) -> anyhow::Result<()> {
    let won = fight.challengers_won();

    // GameFightEndMessage with results
    let mut results_writer = BigEndianWriter::new();
    results_writer.write_int(fight.round * 30); // duration estimate in seconds
    results_writer.write_var_short(100); // rewardRate
    results_writer.write_var_short(0); // lootShareLimitMalus

    // Results — polymorphic vector (FightResultPlayerListEntry TYPE_ID 6765)
    results_writer.write_short(1); // 1 result entry (the player)
    results_writer.write_ushort(6765); // FightResultPlayerListEntry

    let outcome: i16 = if won { 2 } else { 0 }; // WIN=2, LOSE=0
    results_writer.write_var_short(outcome);
    results_writer.write_byte(0); // wave

    // FightLoot (TYPE_ID 7757)
    let xp_gained = if won { 50 * character.level as i64 } else { 0 };
    let kamas_gained = if won { 10 * character.level as i64 } else { 0 };
    results_writer.write_short(0); // objects count
    results_writer.write_var_long(kamas_gained);

    // Player-specific fields
    results_writer.write_double(character.id as f64); // id
    results_writer.write_boolean(won); // alive
    results_writer.write_var_short(character.level as i16); // level
    results_writer.write_short(0); // additional (polymorphic, empty)

    // namedPartyTeamsOutcomes
    results_writer.write_short(0);

    session
        .send_raw(RawMessage {
            message_id: GameFightEndMessage::MESSAGE_ID,
            instance_id: 0,
            payload: results_writer.into_data(),
        })
        .await?;

    // XP reward
    if won && xp_gained > 0 {
        session
            .send(&CharacterExperienceGainMessage {
                experience_character: xp_gained,
                experience_mount: 0,
                experience_guild: 0,
                experience_incarnation: 0,
            })
            .await?;

        // Update XP in DB
        let new_xp = character.experience + xp_gained;
        let _ = sqlx::query("UPDATE characters SET experience = $2 WHERE id = $1")
            .bind(character.id)
            .bind(new_xp)
            .execute(&state.pool)
            .await;

        // Level up check (simplified thresholds: level * 100 XP per level)
        let xp_for_next = character.level as i64 * 100;
        if new_xp >= xp_for_next && character.level < 200 {
            let new_level = character.level + 1;
            let _ = sqlx::query("UPDATE characters SET level = $2 WHERE id = $1")
                .bind(character.id)
                .bind(new_level)
                .execute(&state.pool)
                .await;

            session
                .send(&CharacterLevelUpMessage {
                    new_level: new_level as i16,
                })
                .await?;

            tracing::info!(
                character_id = character.id,
                new_level,
                "Character leveled up"
            );
        }
    }

    // Return to roleplay context
    session.send(&GameContextDestroyMessage {}).await?;
    session.send(&GameContextCreateMessage { context: 1 }).await?;

    // Re-join map
    game_context::handle_game_context_create(session, state, character, broadcast_tx).await?;

    Ok(())
}
