use crate::messages::{auth, game};
use dofus_io::{BigEndianReader, DofusDeserialize, DofusMessage};
use std::fmt;

/// Macro that generates the ProtocolMessage enum, message_id(), from_raw(), and Display
/// from a flat list of (Module::Type) entries. Adding a new message = one line.
/// IDs are resolved from each type's MESSAGE_ID const (set by protocol-gen).
macro_rules! protocol_registry {
    ( $( $variant:ident ( $module:ident :: $ty:ident ) ),* $(,)? ) => {
        #[derive(Debug)]
        pub enum ProtocolMessage {
            $( $variant($module::$ty), )*
            Unknown(u16, Vec<u8>),
        }

        impl ProtocolMessage {
            pub fn message_id(&self) -> u16 {
                match self {
                    $( Self::$variant(_) => $module::$ty::MESSAGE_ID, )*
                    Self::Unknown(id, _) => *id,
                }
            }

            pub fn from_raw(message_id: u16, payload: Vec<u8>) -> anyhow::Result<Self> {
                let mut reader = BigEndianReader::new(payload.clone());
                match message_id {
                    $( id if id == $module::$ty::MESSAGE_ID =>
                        Ok(Self::$variant($module::$ty::deserialize(&mut reader)?)), )*
                    _ => Ok(Self::Unknown(message_id, payload)),
                }
            }
        }

        impl fmt::Display for ProtocolMessage {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self {
                    Self::Unknown(id, data) => write!(f, "Unknown({}, {} bytes)", id, data.len()),
                    other => write!(f, "{:?}", other),
                }
            }
        }
    };
}

protocol_registry! {
    // Auth flow
    ProtocolRequired(auth::ProtocolRequired),
    HelloConnectMessage(auth::HelloConnectMessage),
    IdentificationMessage(auth::IdentificationMessage),
    IdentificationSuccessMessage(auth::IdentificationSuccessMessage),
    IdentificationFailedMessage(auth::IdentificationFailedMessage),
    ServersListMessage(auth::ServersListMessage),
    ServerSelectionMessage(auth::ServerSelectionMessage),
    SelectedServerDataMessage(auth::SelectedServerDataMessage),
    // Game flow
    AuthenticationTicketMessage(game::AuthenticationTicketMessage),
    BasicPingMessage(game::BasicPingMessage),
    BasicPongMessage(game::BasicPongMessage),
    CharactersListRequestMessage(game::CharactersListRequestMessage),
    CharacterSelectionMessage(game::CharacterSelectionMessage),
    GameContextCreateRequestMessage(game::GameContextCreateRequestMessage),
    CharacterNameSuggestionRequestMessage(game::CharacterNameSuggestionRequestMessage),
    CharacterCreationRequestMessage(game::CharacterCreationRequestMessage),
    // Phase 1 — Enter world
    CharacterStatsListMessage(game::CharacterStatsListMessage),
    InventoryContentMessage(game::InventoryContentMessage),
    InventoryWeightMessage(game::InventoryWeightMessage),
    SpellListMessage(game::SpellListMessage),
    SetCharacterRestrictionsMessage(game::SetCharacterRestrictionsMessage),
    CharacterLoadingCompleteMessage(game::CharacterLoadingCompleteMessage),
    LifePointsRegenBeginMessage(game::LifePointsRegenBeginMessage),
    ServerExperienceModificatorMessage(game::ServerExperienceModificatorMessage),
    GameContextRemoveElementMessage(game::GameContextRemoveElementMessage),
    // Phase 2 — Movement + map transitions
    GameMapMovementRequestMessage(game::GameMapMovementRequestMessage),
    GameMapMovementMessage(game::GameMapMovementMessage),
    GameMapMovementConfirmMessage(game::GameMapMovementConfirmMessage),
    GameMapMovementCancelMessage(game::GameMapMovementCancelMessage),
    ChangeMapMessage(game::ChangeMapMessage),
    GameMapChangeOrientationRequestMessage(game::GameMapChangeOrientationRequestMessage),
    GameMapChangeOrientationMessage(game::GameMapChangeOrientationMessage),
}

#[cfg(test)]
mod tests {
    use super::*;
    use dofus_io::{BigEndianWriter, DofusSerialize};

    fn serialize_msg<M: DofusSerialize>(msg: &M) -> Vec<u8> {
        let mut w = BigEndianWriter::new();
        msg.serialize(&mut w);
        w.into_data()
    }

    #[test]
    fn registry_parse_hello_connect() {
        let msg = auth::HelloConnectMessage {
            salt: "test-salt".to_string(),
            key: vec![1, 2, 3],
        };
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(auth::HelloConnectMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::HelloConnectMessage(m) => {
                assert_eq!(m.salt, "test-salt");
                assert_eq!(m.key, vec![1, 2, 3]);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn registry_parse_identification_failed() {
        let msg = auth::IdentificationFailedMessage { reason: 2 };
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(auth::IdentificationFailedMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::IdentificationFailedMessage(m) => assert_eq!(m.reason, 2),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn registry_parse_server_selection() {
        let msg = auth::ServerSelectionMessage { server_id: 7 };
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(auth::ServerSelectionMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::ServerSelectionMessage(m) => assert_eq!(m.server_id, 7),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn registry_unknown_message() {
        let parsed = ProtocolMessage::from_raw(65535, vec![1, 2, 3]).unwrap();
        match parsed {
            ProtocolMessage::Unknown(id, data) => {
                assert_eq!(id, 65535);
                assert_eq!(data, vec![1, 2, 3]);
            }
            _ => panic!("Should be Unknown"),
        }
    }

    #[test]
    fn registry_message_id_matches() {
        let msg = auth::IdentificationFailedMessage { reason: 5 };
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(auth::IdentificationFailedMessage::MESSAGE_ID, payload).unwrap();
        assert_eq!(parsed.message_id(), auth::IdentificationFailedMessage::MESSAGE_ID);
    }

    #[test]
    fn registry_display_unknown() {
        let msg = ProtocolMessage::Unknown(9999, vec![0; 10]);
        let s = format!("{}", msg);
        assert!(s.contains("9999"));
        assert!(s.contains("10"));
    }

    #[test]
    fn registry_parse_characters_list_request() {
        let msg = game::CharactersListRequestMessage {};
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(game::CharactersListRequestMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::CharactersListRequestMessage(_) => {}
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn registry_parse_character_selection() {
        let msg = game::CharacterSelectionMessage { id: 123456789 };
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(game::CharacterSelectionMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::CharacterSelectionMessage(m) => assert_eq!(m.id, 123456789),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn registry_parse_game_context_create_request() {
        let msg = game::GameContextCreateRequestMessage {};
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(game::GameContextCreateRequestMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::GameContextCreateRequestMessage(_) => {}
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn registry_parse_character_name_suggestion_request() {
        let msg = game::CharacterNameSuggestionRequestMessage {};
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(game::CharacterNameSuggestionRequestMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::CharacterNameSuggestionRequestMessage(_) => {}
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn registry_parse_character_creation_request() {
        let msg = game::CharacterCreationRequestMessage {
            name: "Xelor".to_string(),
            breed: 11,
            sex: true,
            colors: [0xFF0000, 0x00FF00, 0x0000FF, 0, 0],
            cosmetic_id: 42,
        };
        let payload = serialize_msg(&msg);
        let parsed = ProtocolMessage::from_raw(game::CharacterCreationRequestMessage::MESSAGE_ID, payload).unwrap();
        match parsed {
            ProtocolMessage::CharacterCreationRequestMessage(m) => {
                assert_eq!(m.name, "Xelor");
                assert_eq!(m.breed, 11);
                assert_eq!(m.sex, true);
                assert_eq!(m.colors, [0xFF0000, 0x00FF00, 0x0000FF, 0, 0]);
                assert_eq!(m.cosmetic_id, 42);
            }
            _ => panic!("Wrong variant"),
        }
    }
}
