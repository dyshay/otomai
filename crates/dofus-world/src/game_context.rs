use dofus_io::{BigEndianWriter, DofusMessage};
use dofus_network::codec::RawMessage;
use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Default spawn map: Incarnam statue (Astrub map as fallback).
const DEFAULT_MAP_ID: f64 = 154010883.0;
const DEFAULT_SUB_AREA_ID: i16 = 449; // Incarnam

/// MapComplementaryInformationsDataMessage ID (polymorphic, not in generated structs).
const MAP_COMPLEMENTARY_MSG_ID: u16 = 5176;

/// Build an empty MapComplementaryInformationsDataMessage payload.
/// All vectors empty, no actors/houses/fights/obstacles.
fn build_empty_map_complementary(sub_area_id: i16, map_id: f64) -> Vec<u8> {
    let mut w = BigEndianWriter::new();
    w.write_var_short(sub_area_id); // subAreaId
    w.write_double(map_id); // mapId
    w.write_short(0); // houses (polymorphic, count=0)
    w.write_short(0); // actors (polymorphic, count=0)
    w.write_short(0); // interactiveElements (polymorphic, count=0)
    w.write_short(0); // statedElements (count=0)
    w.write_short(0); // obstacles (count=0)
    w.write_short(0); // fights (count=0)
    w.write_boolean(false); // hasAggressiveMonsters
    // FightStartingPositions (inline): two empty vectors
    w.write_short(0); // positionsForChallengers
    w.write_short(0); // positionsForDefenders
    w.into_data()
}

/// Handle GameContextCreateRequestMessage: send messages for the client to enter the game.
pub async fn handle_game_context_create(session: &mut Session) -> anyhow::Result<()> {
    // GameContextCreateMessage (context=1 = ROLE_PLAY)
    session
        .send(&GameContextCreateMessage { context: 1 })
        .await?;

    // CurrentMapMessage — tells the client which map to load
    session
        .send(&CurrentMapMessage {
            map_id: DEFAULT_MAP_ID,
            map_key: String::new(),
        })
        .await?;

    // MapComplementaryInformationsDataMessage — unblocks the loading screen
    let payload = build_empty_map_complementary(DEFAULT_SUB_AREA_ID, DEFAULT_MAP_ID);
    session
        .send_raw(RawMessage {
            message_id: MAP_COMPLEMENTARY_MSG_ID,
            instance_id: 0,
            payload,
        })
        .await?;

    tracing::info!("Game context created, map {DEFAULT_MAP_ID} sent");
    Ok(())
}
