use super::state::MapPlayer;
use dofus_io::{BigEndianWriter, DofusSerialize, DofusType};
use dofus_network::codec::RawMessage;
use dofus_protocol::generated::types::{
    ActorAlignmentInformations, ActorRestrictionsInformations, EntityDispositionInformations,
    EntityDispositionInformationsVariant, GameRolePlayCharacterInformations,
    HumanInformations, HumanInformationsVariant,
};

/// Message IDs for actor show/remove (polymorphic, sent as raw).
const GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID: u16 = 3267;
const GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID: u16 = 6344;

/// Build a GameRolePlayShowActorMessage (ID 3267) as raw bytes.
pub fn build_show_actor_raw_msg(player: &MapPlayer) -> RawMessage {
    build_show_actor_raw(player)
}

pub(crate) fn build_show_actor_raw(player: &MapPlayer) -> RawMessage {
    let info = build_character_informations(player);
    let mut w = BigEndianWriter::new();
    // Polymorphic: write type_id then serialize
    w.write_ushort(GameRolePlayCharacterInformations::TYPE_ID);
    info.serialize(&mut w);
    RawMessage {
        message_id: GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID,
        instance_id: 0,
        payload: w.into_data(),
    }
}

/// Build a GameContextRemoveElementMessage (ID 6344) as raw bytes.
pub(crate) fn build_remove_actor_raw(character_id: i64) -> RawMessage {
    let mut w = BigEndianWriter::new();
    w.write_double(character_id as f64);
    RawMessage {
        message_id: GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID,
        instance_id: 0,
        payload: w.into_data(),
    }
}

/// Build GameRolePlayCharacterInformations for a player on the map.
pub fn build_character_informations(player: &MapPlayer) -> GameRolePlayCharacterInformations {
    GameRolePlayCharacterInformations {
        contextual_id: player.character_id as f64,
        disposition: Box::new(EntityDispositionInformationsVariant::EntityDispositionInformations(
            EntityDispositionInformations {
                cell_id: player.cell_id,
                direction: player.direction,
            },
        )),
        look: player.entity_look.clone(),
        name: player.name.clone(),
        humanoid_info: Box::new(HumanInformationsVariant::HumanInformations(
            HumanInformations {
                restrictions: ActorRestrictionsInformations::default(),
                sex: player.sex,
                options: vec![],
            },
        )),
        account_id: player.account_id as i32,
        alignment_infos: ActorAlignmentInformations::default(),
    }
}

/// Build the actors portion for MapComplementaryInformationsDataMessage.
pub fn write_actors(w: &mut BigEndianWriter, players: &[MapPlayer]) {
    w.write_short(players.len() as i16);
    for player in players {
        let info = build_character_informations(player);
        w.write_ushort(GameRolePlayCharacterInformations::TYPE_ID);
        info.serialize(w);
    }
}

#[cfg(test)]
pub(crate) const GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID_TEST: u16 = GAME_ROLE_PLAY_SHOW_ACTOR_MSG_ID;
#[cfg(test)]
pub(crate) const GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID_TEST: u16 = GAME_CONTEXT_REMOVE_ELEMENT_MSG_ID;
