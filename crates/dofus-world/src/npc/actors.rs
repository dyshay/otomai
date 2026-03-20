use dofus_database::models::NpcSpawn;
use dofus_io::{BigEndianWriter, DofusSerialize, DofusType};
use dofus_protocol::generated::types::{
    EntityDispositionInformations, EntityDispositionInformationsVariant, EntityLook,
    GameRolePlayNpcQuestFlag, GameRolePlayNpcWithQuestInformations,
};

/// Negative contextual IDs for NPCs (convention: negative to distinguish from players).
/// Each NPC spawn gets a unique negative ID based on its spawn row ID.
pub fn npc_contextual_id(spawn_id: i32) -> f64 {
    -(spawn_id as f64) - 1000.0
}

/// Build NPC actor data for MapComplementary, including quest flags for a specific player.
pub fn build_npc_actor(
    spawn: &NpcSpawn,
    npc_look: &EntityLook,
    quests_to_start: Vec<i16>,
    quests_to_valid: Vec<i16>,
) -> GameRolePlayNpcWithQuestInformations {
    GameRolePlayNpcWithQuestInformations {
        contextual_id: npc_contextual_id(spawn.id),
        disposition: Box::new(EntityDispositionInformationsVariant::EntityDispositionInformations(
            EntityDispositionInformations {
                cell_id: spawn.cell_id as i16,
                direction: spawn.direction as u8,
            },
        )),
        look: npc_look.clone(),
        npc_id: spawn.npc_id as i16,
        sex: false,
        special_artwork_id: 0,
        quest_flag: GameRolePlayNpcQuestFlag {
            quests_to_valid_id: quests_to_valid,
            quests_to_start_id: quests_to_start,
        },
    }
}

/// Write NPC actors into the MapComplementary actor list.
/// Must be called during actor serialization (after player actors).
pub fn write_npc_actors(
    w: &mut BigEndianWriter,
    npcs: &[GameRolePlayNpcWithQuestInformations],
) {
    for npc in npcs {
        w.write_ushort(GameRolePlayNpcWithQuestInformations::TYPE_ID);
        npc.serialize(w);
    }
}
