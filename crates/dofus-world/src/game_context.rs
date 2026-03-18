use dofus_network::session::Session;
use dofus_protocol::messages::game::*;

/// Default spawn map: Incarnam start (map_id from Dofus data).
const DEFAULT_MAP_ID: f64 = 154010883.0;

/// Handle GameContextCreateRequestMessage: send minimal messages for the client to enter the game.
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

    tracing::info!("Game context created, map sent");
    Ok(())
}
