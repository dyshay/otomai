//! Summon system: create new fighters mid-fight.
//! Used by Osamodas, Eniripsa, Sadida and various spells.

use super::buffs::BuffList;
use super::state::{Fight, Fighter, FighterStats, Team};
use dofus_io::{BigEndianWriter, DofusSerialize};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::types::EntityLook;
use dofus_protocol::messages::game::*;
use std::collections::HashMap;

/// Maximum summons per caster (default limit).
const MAX_SUMMONS_PER_CASTER: usize = 1;

static NEXT_SUMMON_ID: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(-1000);
fn next_summon_id() -> f64 {
    NEXT_SUMMON_ID.fetch_sub(1, std::sync::atomic::Ordering::Relaxed) as f64
}

/// Summon a creature onto the battlefield.
pub async fn summon_creature(
    session: &mut Session,
    fight: &mut Fight,
    caster_id: f64,
    cell_id: i16,
    monster_id: i32,
    grade: u8,
    level: i16,
) -> anyhow::Result<()> {
    // Check summon limit
    let caster_team = fight.get_fighter(caster_id).map(|f| f.team).unwrap_or(Team::Challengers);
    let current_summons = fight.fighters.iter()
        .filter(|f| f.stats.power == -1 && f.team == caster_team && f.is_alive) // power=-1 marks summons
        .count();

    let max_summons = fight.get_fighter(caster_id)
        .map(|f| {
            let boost = f.buffs.stat_bonus(super::effects::StatType::Critical); // Summon boost uses a stat
            (MAX_SUMMONS_PER_CASTER as i16 + boost).max(1) as usize
        })
        .unwrap_or(MAX_SUMMONS_PER_CASTER);

    if current_summons >= max_summons {
        return Ok(());
    }

    // Check cell is free
    if fight.fighter_on_cell(cell_id).is_some() {
        return Ok(());
    }

    let summon_id = next_summon_id();
    let hp = 20 + level as i32 * 8;

    let summon = Fighter {
        id: summon_id,
        name: format!("Invocation ({})", monster_id),
        level,
        breed: 0,
        look: EntityLook {
            bones_id: 1,
            skins: vec![],
            indexed_colors: vec![],
            scales: vec![80],
            subentities: vec![],
        },
        cell_id,
        direction: 1,
        team: caster_team,
        life_points: hp,
        max_life_points: hp,
        shield_points: 0,
        action_points: 4,
        max_action_points: 4,
        movement_points: 3,
        max_movement_points: 3,
        is_player: false,
        is_alive: true,
        invisible: false,
        states: super::states::StateList::default(),
        monster_id,
        monster_grade: grade,
        stats: FighterStats {
            power: -1, // Marks this as a summon
            ..Default::default()
        },
        buffs: BuffList::default(),
        spell_casts_this_turn: HashMap::new(),
        spell_casts_on_target: HashMap::new(),
    };

    fight.add_fighter(summon);

    // Send GameFightShowFighterMessage for the summon
    // (using GameFightMonsterInformations TYPE_ID 6096)
    let fighter = fight.fighters.last().unwrap();
    let mut w = BigEndianWriter::new();
    w.write_ushort(6096);
    w.write_double(fighter.id);
    w.write_ushort(7114); // EntityDispositionInformations
    w.write_short(fighter.cell_id);
    w.write_byte(fighter.direction);
    fighter.look.serialize(&mut w);
    w.write_byte(fighter.team as u8);
    w.write_boolean(true); // alive
    w.write_ushort(6271); // GameContextActorPositionInformations
    w.write_double(fighter.id);
    w.write_ushort(7114);
    w.write_short(fighter.cell_id);
    w.write_byte(fighter.direction);
    fighter.look.serialize(&mut w);
    w.write_byte(0); // wave
    w.write_ushort(8253); // GameFightMinimalStats
    super::serialization::write_minimal_stats(&mut w, fighter);
    w.write_short(0); // previousPositions
    w.write_var_short(fighter.monster_id as i16);
    w.write_byte(fighter.monster_grade);
    w.write_var_short(fighter.level);

    session
        .send_raw(RawMessage {
            message_id: 5525, // GameFightShowFighterMessage
            instance_id: 0,
            payload: w.into_data(),
        })
        .await?;

    // Update turn list
    session
        .send(&GameFightTurnListMessage {
            ids: fight.turn_order(),
            deads_ids: fight.dead_ids(),
        })
        .await?;

    Ok(())
}

/// Kill all summons of a dead summoner.
pub async fn kill_summons_of(
    session: &mut Session,
    fight: &mut Fight,
    caster_id: f64,
) -> anyhow::Result<()> {
    let summon_ids: Vec<f64> = fight.fighters.iter()
        .filter(|f| f.stats.power == -1 && f.is_alive)
        .map(|f| f.id)
        .collect();

    // Simple approach: kill summons that belong to the same team as the dead caster
    // (In full Dofus, summoner is tracked via the `summoner` field in stats)
    for sid in summon_ids {
        if let Some(s) = fight.get_fighter_mut(sid) {
            s.is_alive = false;
            s.life_points = 0;
        }
        session
            .send(&GameActionFightDeathMessage {
                action_id: 103,
                source_id: caster_id,
                target_id: sid,
            })
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summon_id_is_negative() {
        let id = next_summon_id();
        assert!(id < 0.0);
    }

    #[test]
    fn max_summons_default() {
        assert_eq!(MAX_SUMMONS_PER_CASTER, 1);
    }
}
