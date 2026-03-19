use crate::WorldState;
use dofus_io::{BigEndianWriter, DofusMessage, DofusSerialize};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;
use std::sync::Arc;

/// Default emotes available to all players.
/// IDs from Emoticons.d2o — basic set.
const DEFAULT_EMOTES: &[u8] = &[
    1,  // Assis
    2,  // Se fait remarquer
    3,  // Pointe du doigt
    4,  // Au secours
    5,  // Se retourne
    6,  // Bat des bras
    7,  // S'incline
    8,  // Applaudit
    9,  // En colère
    10, // Bâille
    11, // Perdu
    14, // Peur
    15, // Courageux
    16, // Coucou
    19, // Croise les bras
    21, // Pierre
    22, // Papier
    23, // Ciseaux
    24, // Montre
];

/// Send EmoteListMessage — available emotes on game context enter.
pub async fn send_emote_list(session: &mut Session) -> anyhow::Result<()> {
    session
        .send(&EmoteListMessage {
            emote_ids: DEFAULT_EMOTES.to_vec(),
        })
        .await?;
    Ok(())
}

/// Handle EmotePlayRequestMessage — broadcast emote to map.
pub async fn handle_emote_play(
    state: &Arc<WorldState>,
    character_id: i64,
    account_id: i64,
    current_map_id: i64,
    msg: &EmotePlayRequestMessage,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().timestamp_millis() as f64;

    let play_msg = EmotePlayMessage {
        emote_id: msg.emote_id,
        emote_start_time: now,
        actor_id: character_id as f64,
        account_id: account_id as i32,
    };

    let mut w = BigEndianWriter::new();
    play_msg.serialize(&mut w);
    let raw = RawMessage {
        message_id: EmotePlayMessage::MESSAGE_ID,
        instance_id: 0,
        payload: w.into_data(),
    };

    state.world.broadcast_to_map(current_map_id, raw).await;
    Ok(())
}
