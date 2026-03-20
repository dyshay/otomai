use crate::game_context;
use crate::WorldState;
use dofus_database::models::Character;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use super::buffs::BuffList;
use super::serialization::send_show_fighter;
use super::state::{Element, Fight, FightPhase, Fighter, FighterStats, Team};
use super::states;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use dofus_network::codec::RawMessage;

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
    let player_stats = super::serialization::build_player_stats(character);

    let player_hp = crate::stats::base_hp(character);
    let player_fighter = Fighter {
        id: character.id as f64,
        name: character.name.clone(),
        level: character.level as i16,
        breed: character.breed_id as u8,
        look: entity_look,
        cell_id: 300,
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
        states: states::StateList::default(),
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
        states: states::StateList::default(),
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

    super::turns::start_next_turn(session, fight).await
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
