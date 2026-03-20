mod d2o;
mod flags;
mod objectives;
mod rewards;
mod tracking;

pub mod objective_types {
    pub const TALK_TO_NPC: i32 = 0;
    pub const DEFEAT_MONSTER: i32 = 1;
    pub const COLLECT_ITEM: i32 = 2;
    pub const GO_TO_MAP: i32 = 3;
    pub const CRAFT_ITEM: i32 = 4;
    pub const DISCOVER_SUBAREA: i32 = 5;
    pub const HARVEST_RESOURCE: i32 = 6;
    pub const REACH_LEVEL: i32 = 7;
    pub const WIN_FIGHT_ON_MAP: i32 = 8;
}

pub use flags::compute_npc_quest_flags;
pub use objectives::{check_defeat_monster_objectives, check_level_objectives, check_map_objectives, check_talk_to_npc_objective};
pub use tracking::{handle_quest_list, start_quest};

#[cfg(test)]
mod tests {
    use super::*;
    use dofus_io::BigEndianWriter;
    use dofus_protocol::messages::game::*;

    #[test]
    fn objective_type_constants() {
        assert_eq!(objective_types::TALK_TO_NPC, 0);
        assert_eq!(objective_types::GO_TO_MAP, 3);
        assert_eq!(objective_types::DISCOVER_SUBAREA, 5);
    }

    #[test]
    fn quest_list_empty_payload() {
        let mut w = BigEndianWriter::new();
        w.write_short(0);
        w.write_short(0);
        w.write_short(0);
        w.write_short(0);
        let data = w.into_data();
        assert_eq!(data.len(), 8);
    }

    #[test]
    fn quest_started_serializes() {
        use dofus_io::{BigEndianReader, BigEndianWriter, DofusDeserialize, DofusSerialize};
        let msg = QuestStartedMessage { quest_id: 42 };
        let mut w = BigEndianWriter::new();
        msg.serialize(&mut w);
        let mut r = BigEndianReader::new(w.into_data());
        let decoded = QuestStartedMessage::deserialize(&mut r).unwrap();
        assert_eq!(decoded.quest_id, 42);
    }
}
