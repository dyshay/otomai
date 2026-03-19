pub mod buffs;
pub mod damage;
pub mod displacement;
pub mod effects;
pub mod marks;
pub mod spells;
pub mod state;
pub mod turns;

use crate::game_context;
use crate::world::MapPlayer;
use crate::WorldState;
use dofus_database::models::Character;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use buffs::BuffList;
use state::{Element, Fight, FightPhase, Fighter, FighterStats, Team};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use std::sync::atomic::{AtomicI16, Ordering};
static NEXT_FIGHT_ID: AtomicI16 = AtomicI16::new(1);
fn next_fight_id() -> i16 {
    NEXT_FIGHT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Initiate a PvE fight.
pub async fn start_pve_fight(
    session: &mut Session,
    state: &Arc<WorldState>,
    character: &Character,
    current_map_id: i64,
    monster_group_id: f64,
    broadcast_tx: &mpsc::UnboundedSender<RawMessage>,
) -> anyhow::Result<Option<Fight>> {
    let fight_id = next_fight_id();

    // 1. Notify: GameFightStartingMessage
    session
        .send(&GameFightStartingMessage {
            fight_type: 0, // PvM
            fight_id,
            attacker_id: character.id as f64,
            defender_id: monster_group_id,
            contains_boss: false,
        })
        .await?;

    // 2. Remove from roleplay
    state.world.leave_map(current_map_id, character.id).await;

    // 3. Context switch to fight
    session.send(&GameContextDestroyMessage {}).await?;
    session.send(&GameContextCreateMessage { context: 2 }).await?;

    // 4. GameFightJoinMessage
    session
        .send(&GameFightJoinMessage {
            is_team_phase: true,
            can_be_cancelled: false,
            can_say_ready: true,
            is_fight_started: false,
            time_max_before_fight_start: 45,
            fight_type: 0,
        })
        .await?;

    // 5. Build fighters
    let entity_look = game_context::build_entity_look(character);
    let player_stats = build_player_stats(character);

    let player_hp = crate::stats::base_hp(character);
    let player_fighter = Fighter {
        id: character.id as f64,
        name: character.name.clone(),
        level: character.level as i16,
        breed: character.breed_id as u8,
        look: entity_look,
        cell_id: 300, // will be set to placement pos
        direction: 1,
        team: Team::Challengers,
        life_points: player_hp,
        max_life_points: player_hp,
        action_points: 6,
        max_action_points: 6,
        movement_points: 3,
        max_movement_points: 3,
        is_player: true,
        is_alive: true,
        monster_id: 0,
        monster_grade: 0,
        stats: player_stats,
        shield_points: 0,
        invisible: false,
        buffs: BuffList::default(),
        spell_casts_this_turn: HashMap::new(),
        spell_casts_on_target: HashMap::new(),
    };

    // Monster: try to load from D2O, fallback to defaults scaled by level
    let monster_level = character.level.max(1);
    let monster_hp = 30 + monster_level as i32 * 15;
    let monster_fighter = Fighter {
        id: -1.0,
        name: "Monstre".to_string(),
        level: monster_level as i16,
        breed: 0,
        look: dofus_protocol::generated::types::EntityLook {
            bones_id: 1,
            skins: vec![],
            indexed_colors: vec![],
            scales: vec![100],
            subentities: vec![],
        },
        cell_id: 420,
        direction: 5,
        team: Team::Defenders,
        life_points: monster_hp,
        max_life_points: monster_hp,
        action_points: 6,
        max_action_points: 6,
        movement_points: 3,
        max_movement_points: 3,
        is_player: false,
        is_alive: true,
        monster_id: 0,
        monster_grade: 1,
        stats: FighterStats::default(),
        shield_points: 0,
        invisible: false,
        buffs: BuffList::default(),
        spell_casts_this_turn: HashMap::new(),
        spell_casts_on_target: HashMap::new(),
    };

    let mut fight = Fight::new(fight_id, current_map_id);

    // Placement positions from DLM (FightStartingPositions)
    let (challenger_pos, defender_pos) = if let Some(map_data) = state.maps.get(current_map_id) {
        // Use walkable cells as placement positions
        let challengers: Vec<i16> = (280..310)
            .filter(|&c| (c as usize) < map_data.cells.len() && map_data.cells[c as usize].is_walkable())
            .map(|c| c as i16)
            .take(8)
            .collect();
        let defenders: Vec<i16> = (400..430)
            .filter(|&c| (c as usize) < map_data.cells.len() && map_data.cells[c as usize].is_walkable())
            .map(|c| c as i16)
            .take(8)
            .collect();
        (challengers, defenders)
    } else {
        ((280..288).collect(), (400..408).collect())
    };

    // Set fighters to first position of their team
    let mut pf = player_fighter;
    pf.cell_id = *challenger_pos.first().unwrap_or(&300);
    let mut mf = monster_fighter;
    mf.cell_id = *defender_pos.first().unwrap_or(&420);

    fight.challenger_positions = challenger_pos.clone();
    fight.defender_positions = defender_pos.clone();
    fight.add_fighter(pf);
    fight.add_fighter(mf);

    // 6. Placement positions
    session
        .send(&GameFightPlacementPossiblePositionsMessage {
            positions_for_challengers: challenger_pos,
            positions_for_defenders: defender_pos,
            team_number: 0,
        })
        .await?;

    // 7. Show fighters
    for fighter in &fight.fighters {
        send_show_fighter(session, fighter).await?;
    }

    // 8. Turn list
    session
        .send(&GameFightTurnListMessage {
            ids: fight.turn_order(),
            deads_ids: vec![],
        })
        .await?;

    Ok(Some(fight))
}

/// Handle ready signal → start fight.
pub async fn handle_fight_ready(
    session: &mut Session,
    fight: &mut Fight,
) -> anyhow::Result<()> {
    fight.phase = FightPhase::Fighting;

    session.send(&GameFightStartMessage { idols: vec![] }).await?;

    // Update turn list
    session
        .send(&GameFightTurnListMessage {
            ids: fight.turn_order(),
            deads_ids: fight.dead_ids(),
        })
        .await?;

    turns::start_next_turn(session, fight).await
}

/// Handle placement position change.
pub async fn handle_placement_position(
    session: &mut Session,
    fight: &mut Fight,
    player_id: f64,
    cell_id: i16,
) -> anyhow::Result<()> {
    if fight.phase != FightPhase::Placement {
        return Ok(());
    }

    // Validate cell is in challenger positions
    if !fight.challenger_positions.contains(&cell_id) {
        return Ok(());
    }

    if let Some(f) = fight.get_fighter_mut(player_id) {
        f.cell_id = cell_id;
    }

    // Send disposition update
    session
        .send(&GameEntityDispositionMessage {
            disposition: dofus_protocol::generated::types::IdentifiedEntityDispositionInformations {
                cell_id,
                direction: 1,
                id: player_id,
            },
        })
        .await?;

    Ok(())
}

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

/// Build FighterStats from a DB Character.
fn build_player_stats(c: &Character) -> FighterStats {
    let get = |key: &str| c.stats.get(key).and_then(|v| v.as_i64()).unwrap_or(0) as i16;
    FighterStats {
        strength: get("strength"),
        intelligence: get("intelligence"),
        chance: get("chance"),
        agility: get("agility"),
        power: 0,
        ..Default::default()
    }
}

/// Send GameFightShowFighterMessage for a fighter (polymorphic, raw build).
async fn send_show_fighter(session: &mut Session, fighter: &Fighter) -> anyhow::Result<()> {
    let mut w = BigEndianWriter::new();

    if fighter.is_player {
        w.write_ushort(7807); // GameFightCharacterInformations
    } else {
        w.write_ushort(6096); // GameFightMonsterInformations
    }

    // contextual_id
    w.write_double(fighter.id);
    // disposition (EntityDispositionInformations, TYPE_ID 7114)
    w.write_ushort(7114);
    w.write_short(fighter.cell_id);
    w.write_byte(fighter.direction);
    // look
    fighter.look.serialize(&mut w);
    // spawnInfo (GameContextBasicSpawnInformation, TYPE_ID 7069)
    w.write_byte(fighter.team as u8);
    w.write_boolean(fighter.is_alive);
    // informations (GameContextActorPositionInformations, TYPE_ID 6271)
    w.write_ushort(6271);
    w.write_double(fighter.id);
    w.write_ushort(7114);
    w.write_short(fighter.cell_id);
    w.write_byte(fighter.direction);
    fighter.look.serialize(&mut w);

    w.write_byte(0); // wave
    // GameFightMinimalStats (TYPE_ID 8253)
    w.write_ushort(8253);
    write_minimal_stats(&mut w, fighter);
    w.write_short(0); // previousPositions count

    if fighter.is_player {
        w.write_utf(&fighter.name);
        w.write_byte(0); // PlayerStatus statusId
        w.write_var_short(0); // leagueId
        w.write_int(0); // ladderPosition
        w.write_boolean(false); // hiddenInPrefight
        w.write_var_short(fighter.level);
        // AlignmentInfos
        w.write_byte(0);
        w.write_byte(0);
        w.write_byte(0);
        w.write_double(0.0);
        w.write_byte(fighter.breed);
        w.write_boolean(false);
    } else {
        w.write_var_short(fighter.monster_id as i16); // creatureGenericId
        w.write_byte(fighter.monster_grade); // creatureGrade
        w.write_var_short(fighter.level); // creatureLevel
    }

    session
        .send_raw(RawMessage {
            message_id: 5525, // GameFightShowFighterMessage
            instance_id: 0,
            payload: w.into_data(),
        })
        .await?;

    Ok(())
}

fn write_minimal_stats(w: &mut BigEndianWriter, f: &Fighter) {
    w.write_var_int(f.life_points);
    w.write_var_int(f.max_life_points);
    w.write_var_int(f.max_life_points); // baseMaxLifePoints
    w.write_var_int(0); // permanentDamagePercent
    w.write_var_int(0); // shieldPoints
    w.write_var_short(f.action_points);
    w.write_var_short(f.max_action_points);
    w.write_var_short(f.movement_points);
    w.write_var_short(f.max_movement_points);
    w.write_double(0.0); // summoner
    w.write_boolean(false); // summoned
    // Resists (neutral, earth, water, air, fire) percent + flat = 10
    for _ in 0..10 { w.write_var_short(0); }
    w.write_var_short(0); // criticalDamageFixedResist
    w.write_var_short(0); // pushDamageFixedResist
    // PvP resists = 10
    for _ in 0..10 { w.write_var_short(0); }
    w.write_var_short(0); // dodgePALostProbability
    w.write_var_short(0); // dodgePMLostProbability
    w.write_var_short(0); // tackleBlock
    w.write_var_short(0); // tackleEvade
    w.write_var_short(0); // fixedDamageReflection
    w.write_byte(0); // invisibilityState
    w.write_var_short(0); // meleeDamageReceivedPercent
    w.write_var_short(0); // rangedDamageReceivedPercent
    w.write_var_short(0); // weaponDamageReceivedPercent
    w.write_var_short(0); // spellDamageReceivedPercent
}
