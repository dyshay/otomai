use crate::messages::{auth, game};
use dofus_io::{BigEndianReader, DofusDeserialize, DofusMessage};
use std::fmt;

/// A dynamically-typed protocol message (subset used by handlers).
#[derive(Debug)]
pub enum ProtocolMessage {
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
    // Unknown
    Unknown(u16, Vec<u8>),
}

impl ProtocolMessage {
    pub fn message_id(&self) -> u16 {
        match self {
            Self::ProtocolRequired(_) => auth::ProtocolRequired::MESSAGE_ID,
            Self::HelloConnectMessage(_) => auth::HelloConnectMessage::MESSAGE_ID,
            Self::IdentificationMessage(_) => auth::IdentificationMessage::MESSAGE_ID,
            Self::IdentificationSuccessMessage(_) => auth::IdentificationSuccessMessage::MESSAGE_ID,
            Self::IdentificationFailedMessage(_) => auth::IdentificationFailedMessage::MESSAGE_ID,
            Self::ServersListMessage(_) => auth::ServersListMessage::MESSAGE_ID,
            Self::ServerSelectionMessage(_) => auth::ServerSelectionMessage::MESSAGE_ID,
            Self::SelectedServerDataMessage(_) => auth::SelectedServerDataMessage::MESSAGE_ID,
            Self::AuthenticationTicketMessage(_) => game::AuthenticationTicketMessage::MESSAGE_ID,
            Self::BasicPingMessage(_) => game::BasicPingMessage::MESSAGE_ID,
            Self::BasicPongMessage(_) => game::BasicPongMessage::MESSAGE_ID,
            Self::Unknown(id, _) => *id,
        }
    }

    pub fn from_raw(message_id: u16, payload: Vec<u8>) -> anyhow::Result<Self> {
        let mut reader = BigEndianReader::new(payload.clone());
        match message_id {
            id if id == auth::ProtocolRequired::MESSAGE_ID =>
                Ok(Self::ProtocolRequired(auth::ProtocolRequired::deserialize(&mut reader)?)),
            id if id == auth::HelloConnectMessage::MESSAGE_ID =>
                Ok(Self::HelloConnectMessage(auth::HelloConnectMessage::deserialize(&mut reader)?)),
            id if id == auth::IdentificationMessage::MESSAGE_ID =>
                Ok(Self::IdentificationMessage(auth::IdentificationMessage::deserialize(&mut reader)?)),
            id if id == auth::IdentificationSuccessMessage::MESSAGE_ID =>
                Ok(Self::IdentificationSuccessMessage(auth::IdentificationSuccessMessage::deserialize(&mut reader)?)),
            id if id == auth::IdentificationFailedMessage::MESSAGE_ID =>
                Ok(Self::IdentificationFailedMessage(auth::IdentificationFailedMessage::deserialize(&mut reader)?)),
            id if id == auth::ServersListMessage::MESSAGE_ID =>
                Ok(Self::ServersListMessage(auth::ServersListMessage::deserialize(&mut reader)?)),
            id if id == auth::ServerSelectionMessage::MESSAGE_ID =>
                Ok(Self::ServerSelectionMessage(auth::ServerSelectionMessage::deserialize(&mut reader)?)),
            id if id == auth::SelectedServerDataMessage::MESSAGE_ID =>
                Ok(Self::SelectedServerDataMessage(auth::SelectedServerDataMessage::deserialize(&mut reader)?)),
            id if id == game::AuthenticationTicketMessage::MESSAGE_ID =>
                Ok(Self::AuthenticationTicketMessage(game::AuthenticationTicketMessage::deserialize(&mut reader)?)),
            id if id == game::BasicPingMessage::MESSAGE_ID =>
                Ok(Self::BasicPingMessage(game::BasicPingMessage::deserialize(&mut reader)?)),
            id if id == game::BasicPongMessage::MESSAGE_ID =>
                Ok(Self::BasicPongMessage(game::BasicPongMessage::deserialize(&mut reader)?)),
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
