pub mod state;
pub mod turns;
pub mod spells;
pub mod damage;

use crate::game_context;
use crate::world::MapPlayer;
use crate::WorldState;
use dofus_database::models::Character;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use state::{Fight, FightPhase, Fighter, Team};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Fight IDs — auto-incrementing.
use std::sync::atomic::{AtomicI16, Ordering};
static NEXT_FIGHT_ID: AtomicI16 = AtomicI16::new(1);

fn next_fight_id() -> i16 {
    NEXT_FIGHT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Initiate a PvE fight against a monster group.
pub async fn start_pve_fight(
    session: &mut Session,
    state: &Arc<WorldState>,
    character: &Character,
    current_map_id: i64,
    monster_group_id: f64,
    broadcast_tx: &mpsc::UnboundedSender<RawMessage>,
) -> anyhow::Result<Option<Fight>> {
    let fight_id = next_fight_id();

    // 1. GameFightStartingMessage — notify map players
    let starting_msg = GameFightStartingMessage {
        fight_type: 0, // PvM
        fight_id,
        attacker_id: character.id as f64,
        defender_id: monster_group_id,
        contains_boss: false,
    };
    session.send(&starting_msg).await?;

    // 2. Remove player from roleplay map
    state
        .world
        .leave_map(current_map_id, character.id)
        .await;

    // 3. GameContextDestroyMessage + GameContextCreateMessage (context=2 = FIGHT)
    session
        .send(&GameContextDestroyMessage {})
        .await?;
    session
        .send(&GameContextCreateMessage { context: 2 })
        .await?;

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

    // 5. Create Fight state
    let entity_look = game_context::build_entity_look(character);
    let player_fighter = Fighter {
        id: character.id as f64,
        name: character.name.clone(),
        level: character.level as i16,
        breed: character.breed_id as u8,
        look: entity_look,
        cell_id: character.cell_id as i16,
        team: Team::Challengers,
        life_points: crate::stats::base_hp(character),
        max_life_points: crate::stats::base_hp(character),
        action_points: 6,
        max_action_points: 6,
        movement_points: 3,
        max_movement_points: 3,
        is_player: true,
        is_alive: true,
        monster_id: 0,
        monster_grade: 0,
    };

    // Create a simple monster fighter (from D2O if available, else defaults)
    let monster_fighter = Fighter {
        id: -1.0,
        name: "Monster".to_string(),
        level: character.level as i16,
        breed: 0,
        look: dofus_protocol::generated::types::EntityLook {
            bones_id: 1,
            skins: vec![],
            indexed_colors: vec![],
            scales: vec![100],
            subentities: vec![],
        },
        cell_id: 400,
        team: Team::Defenders,
        life_points: 50 + (character.level as i32 * 10),
        max_life_points: 50 + (character.level as i32 * 10),
        action_points: 6,
        max_action_points: 6,
        movement_points: 3,
        max_movement_points: 3,
        is_player: false,
        is_alive: true,
        monster_id: 0,
        monster_grade: 1,
    };

    let mut fight = Fight::new(fight_id, current_map_id);
    fight.add_fighter(player_fighter);
    fight.add_fighter(monster_fighter);

    // 6. Send placement positions
    // Use some default positions for now
    let challenger_positions: Vec<i16> = (280..295).collect();
    let defender_positions: Vec<i16> = (400..415).collect();

    session
        .send(&GameFightPlacementPossiblePositionsMessage {
            positions_for_challengers: challenger_positions,
            positions_for_defenders: defender_positions,
            team_number: 0,
        })
        .await?;

    // 7. Send fighter informations (polymorphic — using raw)
    send_fighters_info(session, &fight).await?;

    Ok(Some(fight))
}

/// Send fighter informations to the client via GameFightShowFighterMessage-like flow.
/// Uses raw messages since the types are polymorphic.
async fn send_fighters_info(session: &mut Session, fight: &Fight) -> anyhow::Result<()> {
    for fighter in &fight.fighters {
        let mut w = BigEndianWriter::new();

        if fighter.is_player {
            // GameFightShowFighterMessage (ID 5525) with GameFightCharacterInformations
            w.write_ushort(7807); // GameFightCharacterInformations TYPE_ID
            w.write_double(fighter.id); // contextual_id
            // disposition (EntityDispositionInformations, TYPE_ID 7114)
            w.write_ushort(7114);
            w.write_short(fighter.cell_id);
            w.write_byte(1); // direction
            // look
            fighter.look.serialize(&mut w);
            // spawnInfo (GameContextBasicSpawnInformation)
            w.write_byte(fighter.team as u8); // teamId
            w.write_boolean(fighter.is_alive); // alive
            // informations (GameContextActorPositionInformations, TYPE_ID 6271)
            w.write_ushort(6271);
            w.write_double(fighter.id); // id
            // disposition
            w.write_ushort(7114);
            w.write_short(fighter.cell_id);
            w.write_byte(1);
            // look
            fighter.look.serialize(&mut w);

            w.write_byte(0); // wave
            // stats — GameFightMinimalStats (TYPE_ID 8253)
            w.write_ushort(8253);
            write_minimal_stats(&mut w, fighter);
            w.write_short(0); // previousPositions count
            // Character-specific fields
            w.write_utf(&fighter.name);
            // PlayerStatus
            w.write_byte(0); // statusId
            w.write_var_short(0); // leagueId
            w.write_int(0); // ladderPosition
            w.write_boolean(false); // hiddenInPrefight
            w.write_var_short(fighter.level);
            // AlignmentInfos
            w.write_byte(0); // alignmentSide
            w.write_byte(0); // alignmentValue
            w.write_byte(0); // alignmentGrade
            w.write_double(0.0); // characterPower
            w.write_byte(fighter.breed);
            w.write_boolean(false); // sex
        } else {
            // GameFightMonsterInformations TYPE_ID 6096
            w.write_ushort(6096);
            w.write_double(fighter.id);
            w.write_ushort(7114);
            w.write_short(fighter.cell_id);
            w.write_byte(1);
            fighter.look.serialize(&mut w);
            // spawnInfo
            w.write_byte(fighter.team as u8);
            w.write_boolean(fighter.is_alive);
            w.write_ushort(6271);
            w.write_double(fighter.id);
            w.write_ushort(7114);
            w.write_short(fighter.cell_id);
            w.write_byte(1);
            fighter.look.serialize(&mut w);

            w.write_byte(0); // wave
            w.write_ushort(8253);
            write_minimal_stats(&mut w, fighter);
            w.write_short(0); // previousPositions
            w.write_var_short(fighter.monster_id as i16);
            w.write_byte(fighter.monster_grade);
            w.write_var_short(fighter.level);
        }

        session
            .send_raw(RawMessage {
                message_id: 5525, // GameFightShowFighterMessage
                instance_id: 0,
                payload: w.into_data(),
            })
            .await?;
    }
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
    // All resist fields = 0
    for _ in 0..10 {
        w.write_var_short(0);
    }
    w.write_var_short(0); // criticalDamageFixedResist
    w.write_var_short(0); // pushDamageFixedResist
    for _ in 0..10 {
        w.write_var_short(0); // PvP resists
    }
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

/// Handle the "ready" signal during placement phase.
pub async fn handle_fight_ready(
    session: &mut Session,
    fight: &mut Fight,
    player_id: f64,
) -> anyhow::Result<()> {
    fight.phase = FightPhase::Fighting;

    // GameFightStartMessage (empty idols)
    session
        .send(&GameFightStartMessage { idols: vec![] })
        .await?;

    // Start first turn
    turns::start_next_turn(session, fight).await?;

    Ok(())
}
