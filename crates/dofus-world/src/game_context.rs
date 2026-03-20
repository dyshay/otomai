use crate::npc;
use crate::world::{self, MapPlayer, World};
use crate::{inventory, quests, spells, stats, WorldState};
use dofus_database::models::Character;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::types::ActorRestrictionsInformations;
use dofus_protocol::generated::types::EntityLook;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::constants::{BREED_SKINS, DEFAULT_MAP_ID, DEFAULT_SUB_AREA_ID, MAP_COMPLEMENTARY_MSG_ID, MAP_ENCRYPTION_KEY};

/// Build an EntityLook from a DB Character.
pub fn build_entity_look(c: &Character) -> EntityLook {
    let indexed_colors: Vec<i32> = c
        .colors
        .as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    v.as_i64()
                        .map(|color| (((i + 1) as i32) << 24) | ((color as i32) & 0x00FFFFFF))
                })
                .collect()
        })
        .unwrap_or_default();

    let bones_id: i16 = if c.sex == 0 { 1 } else { 2 };
    let breed_idx = (c.breed_id as usize).saturating_sub(1).min(BREED_SKINS.len() - 1);
    let skin_id = if c.sex == 0 {
        BREED_SKINS[breed_idx].0
    } else {
        BREED_SKINS[breed_idx].1
    };

    EntityLook {
        bones_id,
        skins: vec![skin_id],
        indexed_colors,
        scales: vec![100],
        subentities: vec![],
    }
}

use dofus_database::repository;
use dofus_protocol::generated::types::GameRolePlayNpcWithQuestInformations;

/// Load and build NPC actors for a map, computing quest flags per player.
pub async fn build_npc_actors_for_map(
    state: &Arc<WorldState>,
    character_id: i64,
    map_id: i64,
) -> Vec<GameRolePlayNpcWithQuestInformations> {
    let spawns = match repository::list_npc_spawns_for_map(&state.pool, map_id).await {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let mut actors = Vec::with_capacity(spawns.len());
    for spawn in &spawns {
        let look = npc::get_npc_look(state, spawn.npc_id, &spawn.look).await;
        let (to_start, to_valid) =
            quests::compute_npc_quest_flags(state, character_id, spawn.npc_id).await;
        actors.push(npc::build_npc_actor(spawn, &look, to_start, to_valid));
    }
    actors
}

/// Build MapComplementaryInformationsDataMessage payload with actors.
pub fn build_map_complementary_payload(
    sub_area_id: i16,
    map_id: f64,
    players: &[MapPlayer],
    npcs: &[GameRolePlayNpcWithQuestInformations],
) -> Vec<u8> {
    let mut w = BigEndianWriter::new();
    w.write_var_short(sub_area_id);
    w.write_double(map_id);
    w.write_short(0); // houses (count=0)

    // actors — total count includes players + NPCs
    let total_actors = players.len() + npcs.len();
    w.write_short(total_actors as i16);
    // Write player actors
    for player in players {
        let info = world::build_character_informations(player);
        w.write_ushort(dofus_protocol::generated::types::GameRolePlayCharacterInformations::TYPE_ID);
        info.serialize(&mut w);
    }
    // Write NPC actors
    npc::write_npc_actors(&mut w, npcs);

    w.write_short(0); // interactiveElements (count=0)
    w.write_short(0); // statedElements (count=0)
    w.write_short(0); // obstacles (count=0)
    w.write_short(0); // fights (count=0)
    w.write_boolean(false); // hasAggressiveMonsters
    // FightStartingPositions
    w.write_short(0); // positionsForChallengers
    w.write_short(0); // positionsForDefenders
    w.into_data()
}

/// Handle GameContextCreateRequestMessage: enter the game world.
///
/// Sends the full Phase 1 message sequence and joins the map.
pub async fn handle_game_context_create(
    session: &mut Session,
    state: &Arc<WorldState>,
    character: &Character,
    broadcast_tx: &mpsc::UnboundedSender<RawMessage>,
) -> anyhow::Result<()> {
    let map_id = if character.map_id == 0 {
        DEFAULT_MAP_ID
    } else {
        character.map_id
    };
    let map_id_f64 = map_id as f64;

    // 1. GameContextCreateMessage (context=1 = ROLE_PLAY)
    session
        .send(&GameContextCreateMessage { context: 1 })
        .await?;

    // 2. CurrentMapMessage
    session
        .send(&CurrentMapMessage {
            map_id: map_id_f64,
            map_key: MAP_ENCRYPTION_KEY.to_string(),
        })
        .await?;

    // Build the MapPlayer for the world state
    let entity_look = build_entity_look(character);
    let map_player = MapPlayer {
        character_id: character.id,
        account_id: character.account_id,
        name: character.name.clone(),
        entity_look: entity_look.clone(),
        cell_id: character.cell_id as i16,
        direction: character.direction as u8,
        level: character.level as i16,
        breed: character.breed_id as u8,
        sex: character.sex != 0,
        tx: broadcast_tx.clone(),
    };

    // Join the map (broadcasts ShowActor to existing players)
    state.world.join_map(map_id, map_player).await;

    // Get all players on the map (including self) for MapComplementary
    let players = state.world.get_players_on_map(map_id).await;
    let sub_area_id = state
        .maps
        .get(map_id)
        .map(|m| m.sub_area_id as i16)
        .unwrap_or(DEFAULT_SUB_AREA_ID);

    // Load NPC actors for this map
    let npc_actors = build_npc_actors_for_map(state, character.id, map_id).await;

    // 3. MapComplementaryInformationsDataMessage — send with empty actors
    //    then add player via ShowActorMessage (more reliable serialization)
    let payload = build_map_complementary_payload(sub_area_id, map_id_f64, &[], &npc_actors);
    session
        .send_raw(RawMessage {
            message_id: MAP_COMPLEMENTARY_MSG_ID,
            instance_id: 0,
            payload,
        })
        .await?;

    // 4. CharacterStatsListMessage
    stats::send_stats(session, character).await?;

    // 5. InventoryContentMessage + InventoryWeightMessage
    inventory::send_inventory_content(session, character).await?;
    inventory::send_inventory_weight(session).await?;

    // 6. SpellListMessage
    spells::send_spell_list(session, state, character.id).await?;

    // 7. SetCharacterRestrictionsMessage (no restrictions)
    session
        .send(&SetCharacterRestrictionsMessage {
            actor_id: character.id as f64,
            restrictions: ActorRestrictionsInformations::default(),
        })
        .await?;

    // 8. CharacterLoadingCompleteMessage
    session
        .send(&CharacterLoadingCompleteMessage {})
        .await?;

    // 9. LifePointsRegenBeginMessage
    stats::send_regen_begin(session).await?;

    // 10. ServerExperienceModificatorMessage (×1 rate)
    session
        .send(&ServerExperienceModificatorMessage {
            experience_percent: 100,
        })
        .await?;

    // 11. Send the player's own actor via GameRolePlayShowActorMessage
    //     This uses the auto-generated serialization which is more reliable
    //     than embedding actors in MapComplementary
    let player_on_map = players.iter().find(|p| p.character_id == character.id);
    if let Some(player) = player_on_map {
        let show_raw = world::build_show_actor_raw_msg(player);
        session.send_raw(show_raw).await?;
    }

    tracing::info!(
        character_id = character.id,
        map_id,
        "Player entered world"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dofus_io::BigEndianReader;

    fn make_character(breed_id: i32, sex: i32, colors: serde_json::Value) -> Character {
        Character {
            id: 1,
            account_id: 1,
            name: "TestChar".to_string(),
            breed_id,
            sex,
            level: 1,
            experience: 0,
            kamas: 0,
            map_id: 154010883,
            cell_id: 297,
            direction: 3,
            colors,
            stats: serde_json::json!({}),
            created_at: Utc::now(),
            last_login: None,
        }
    }

    #[test]
    fn entity_look_male_iop() {
        let c = make_character(8, 0, serde_json::json!([0xFF0000, 0x00FF00]));
        let look = build_entity_look(&c);

        assert_eq!(look.bones_id, 1); // male
        assert_eq!(look.skins, vec![150]); // Iop male skin
        assert_eq!(look.scales, vec![100]);
        // Color 0: (1 << 24) | 0xFF0000 = 0x01FF0000
        assert_eq!(look.indexed_colors[0], 0x01FF0000u32 as i32);
        // Color 1: (2 << 24) | 0x00FF00 = 0x0200FF00
        assert_eq!(look.indexed_colors[1], 0x0200FF00u32 as i32);
    }

    #[test]
    fn entity_look_female_cra() {
        let c = make_character(9, 1, serde_json::json!([]));
        let look = build_entity_look(&c);

        assert_eq!(look.bones_id, 2); // female
        assert_eq!(look.skins, vec![180]); // Cra female skin
        assert!(look.indexed_colors.is_empty());
    }

    #[test]
    fn entity_look_all_breeds() {
        for breed_id in 1..=18 {
            let c = make_character(breed_id, 0, serde_json::json!([]));
            let look = build_entity_look(&c);
            assert_eq!(look.skins[0], BREED_SKINS[(breed_id - 1) as usize].0);

            let c = make_character(breed_id, 1, serde_json::json!([]));
            let look = build_entity_look(&c);
            assert_eq!(look.skins[0], BREED_SKINS[(breed_id - 1) as usize].1);
        }
    }

    #[test]
    fn map_complementary_empty_is_valid() {
        let data = build_map_complementary_payload(449, 154010883.0, &[], &[]);
        let mut r = BigEndianReader::new(data);

        let sub_area = r.read_var_short().unwrap();
        assert_eq!(sub_area, 449);

        let map_id = r.read_double().unwrap();
        assert_eq!(map_id, 154010883.0);

        let houses = r.read_short().unwrap();
        assert_eq!(houses, 0);

        let actors = r.read_short().unwrap();
        assert_eq!(actors, 0);

        let interactives = r.read_short().unwrap();
        assert_eq!(interactives, 0);

        let stated = r.read_short().unwrap();
        assert_eq!(stated, 0);

        let obstacles = r.read_short().unwrap();
        assert_eq!(obstacles, 0);

        let fights = r.read_short().unwrap();
        assert_eq!(fights, 0);

        let aggressive = r.read_boolean().unwrap();
        assert!(!aggressive);
    }

    #[test]
    fn map_complementary_with_player() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let player = MapPlayer {
            character_id: 42,
            account_id: 1,
            name: "Hero".to_string(),
            entity_look: EntityLook {
                bones_id: 1,
                skins: vec![150],
                indexed_colors: vec![],
                scales: vec![100],
                subentities: vec![],
            },
            cell_id: 300,
            direction: 3,
            level: 1,
            breed: 8,
            sex: false,
            tx,
        };

        let data = build_map_complementary_payload(449, 154010883.0, &[player], &[]);
        let mut r = BigEndianReader::new(data);

        let _sub_area = r.read_var_short().unwrap();
        let _map_id = r.read_double().unwrap();
        let _houses = r.read_short().unwrap();

        // Actors: count should be 1
        let actor_count = r.read_short().unwrap();
        assert_eq!(actor_count, 1);

        // TYPE_ID for GameRolePlayCharacterInformations = 5268
        let type_id = r.read_ushort().unwrap();
        assert_eq!(type_id, 5268);

        // contextual_id = 42.0
        let ctx_id = r.read_double().unwrap();
        assert_eq!(ctx_id, 42.0);
    }

    #[test]
    fn default_map_used_when_zero() {
        // Verify the constant is correct
        assert_eq!(DEFAULT_MAP_ID, 154010883);
        assert_eq!(DEFAULT_SUB_AREA_ID, 449);
    }
}
