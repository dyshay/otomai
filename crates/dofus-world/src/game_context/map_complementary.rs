use crate::npc;
use crate::quests;
use crate::world::{self, MapPlayer};
use crate::WorldState;
use dofus_database::repository;
use dofus_io::{BigEndianWriter, DofusSerialize, DofusType};
use dofus_protocol::generated::types::GameRolePlayCharacterInformations;
use dofus_protocol::generated::types::GameRolePlayNpcWithQuestInformations;
use std::sync::Arc;

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
        w.write_ushort(GameRolePlayCharacterInformations::TYPE_ID);
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
