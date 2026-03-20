use crate::world::{self, MapPlayer};
use crate::{inventory, spells, stats, WorldState};
use dofus_database::models::Character;
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::generated::types::ActorRestrictionsInformations;
use dofus_protocol::messages::game::*;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::constants::{DEFAULT_MAP_ID, DEFAULT_SUB_AREA_ID, MAP_COMPLEMENTARY_MSG_ID, MAP_ENCRYPTION_KEY};
use super::entity_look::build_entity_look;
use super::map_complementary::{build_map_complementary_payload, build_npc_actors_for_map};

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
