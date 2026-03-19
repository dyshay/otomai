use crate::WorldState;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Chat channel IDs (from ChatActivableChannelsEnum.as).
const CHANNEL_GLOBAL: u8 = 0;
const CHANNEL_TEAM: u8 = 1;
const CHANNEL_GUILD: u8 = 2;
const CHANNEL_PARTY: u8 = 4;
const CHANNEL_SALES: u8 = 5;
const CHANNEL_SEEK: u8 = 6;
const CHANNEL_NOOB: u8 = 7;
const CHANNEL_ADMIN: u8 = 8;

/// Handle ChatClientMultiMessage — player sends a message to a channel.
pub async fn handle_chat_multi(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    character_name: &str,
    account_id: i64,
    current_map_id: i64,
    msg: &ChatClientMultiMessage,
) -> anyhow::Result<()> {
    if msg.content.is_empty() || msg.content.len() > 512 {
        return Ok(());
    }

    let timestamp = chrono::Utc::now().timestamp() as i32;

    let server_msg = ChatServerMessage {
        channel: msg.channel,
        content: msg.content.clone(),
        timestamp,
        fingerprint: String::new(),
        sender_id: character_id as f64,
        sender_name: character_name.to_string(),
        prefix: String::new(),
        sender_account_id: account_id as i32,
    };

    let mut w = BigEndianWriter::new();
    server_msg.serialize(&mut w);
    let raw = RawMessage {
        message_id: ChatServerMessage::MESSAGE_ID,
        instance_id: 0,
        payload: w.into_data(),
    };

    match msg.channel {
        // Global = broadcast to current map only
        CHANNEL_GLOBAL | CHANNEL_NOOB => {
            state.world.broadcast_to_map(current_map_id, raw).await;
        }
        // Trade + Recruitment = broadcast to all players
        CHANNEL_SALES | CHANNEL_SEEK => {
            state.world.broadcast_global(raw).await;
        }
        // Team/Guild/Party/Admin — not implemented yet, echo back
        _ => {
            session.send_raw(raw).await?;
        }
    }

    Ok(())
}

/// Handle ChatClientPrivateMessage — whisper to another player.
pub async fn handle_chat_private(
    session: &mut Session,
    state: &Arc<WorldState>,
    character_id: i64,
    character_name: &str,
    account_id: i64,
    msg: &ChatClientPrivateMessage,
) -> anyhow::Result<()> {
    if msg.content.is_empty() || msg.content.len() > 512 {
        return Ok(());
    }

    let timestamp = chrono::Utc::now().timestamp() as i32;

    // Find the target player
    let target = state.world.find_player_by_name(&msg.receiver).await;

    match target {
        Some(target_player) => {
            // Send to target
            let server_msg = ChatServerMessage {
                channel: 9, // PSEUDO_CHANNEL_PRIVATE
                content: msg.content.clone(),
                timestamp,
                fingerprint: String::new(),
                sender_id: character_id as f64,
                sender_name: character_name.to_string(),
                prefix: String::new(),
                sender_account_id: account_id as i32,
            };

            let mut w = BigEndianWriter::new();
            server_msg.serialize(&mut w);
            let raw = RawMessage {
                message_id: ChatServerMessage::MESSAGE_ID,
                instance_id: 0,
                payload: w.into_data(),
            };
            let _ = target_player.tx.send(raw);

            // Send copy to sender
            let copy_msg = ChatServerCopyMessage {
                channel: 9,
                content: msg.content.clone(),
                timestamp,
                fingerprint: String::new(),
                receiver_id: target_player.character_id,
                receiver_name: target_player.name.clone(),
            };
            session.send(&copy_msg).await?;
        }
        None => {
            // Player not found — send error via info channel
            let err_msg = ChatServerMessage {
                channel: 10, // PSEUDO_CHANNEL_INFO
                content: format!("Le joueur \"{}\" n'est pas connecté.", msg.receiver),
                timestamp,
                fingerprint: String::new(),
                sender_id: 0.0,
                sender_name: String::new(),
                prefix: String::new(),
                sender_account_id: 0,
            };
            session.send(&err_msg).await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_constants() {
        assert_eq!(CHANNEL_GLOBAL, 0);
        assert_eq!(CHANNEL_SALES, 5);
        assert_eq!(CHANNEL_SEEK, 6);
    }

    #[test]
    fn chat_server_message_serializes() {
        use dofus_io::{BigEndianReader, DofusDeserialize};

        let msg = ChatServerMessage {
            channel: 0,
            content: "Hello world".to_string(),
            timestamp: 1234567890,
            fingerprint: String::new(),
            sender_id: 42.0,
            sender_name: "TestPlayer".to_string(),
            prefix: String::new(),
            sender_account_id: 1,
        };

        let mut w = BigEndianWriter::new();
        msg.serialize(&mut w);
        let data = w.into_data();

        let mut r = BigEndianReader::new(data);
        let decoded = ChatServerMessage::deserialize(&mut r).unwrap();
        assert_eq!(decoded.channel, 0);
        assert_eq!(decoded.content, "Hello world");
        assert_eq!(decoded.sender_name, "TestPlayer");
        assert_eq!(decoded.sender_id, 42.0);
    }
}
